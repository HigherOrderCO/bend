use super::{loader::PackageLoader, normalize_path, BoundSource, ImportCtx, ImportType, ImportsMap};
use crate::{
  diagnostics::Diagnostics,
  fun::{load_book::do_parse_book, parser::ParseBook, Name},
};
use indexmap::{IndexMap, IndexSet};
use std::{cell::RefCell, collections::VecDeque, path::PathBuf};

#[derive(Default)]
pub struct Packages {
  /// Map from source name to parsed book.
  pub books: IndexMap<Name, RefCell<ParseBook>>,
  /// Already loaded ADTs information to be used when applying ADT binds.
  pub loaded_adts: IndexMap<Name, IndexMap<Name, Vec<Name>>>,
  /// Queue of books indexes that still needs to load its imports.
  load_queue: VecDeque<usize>,
}

impl Packages {
  pub fn new(book: ParseBook) -> Self {
    Self {
      books: IndexMap::from([(book.source.clone(), book.into())]),
      load_queue: VecDeque::new(),
      loaded_adts: IndexMap::new(),
    }
  }

  /// Loads each import statement recursively into a Source -> ParseBook map.
  /// Inserts into the ImportsMap of each book all the imported names.
  pub fn load_imports(
    &mut self,
    loader: &mut impl PackageLoader,
    diag: &mut Diagnostics,
  ) -> Result<ParseBook, Diagnostics> {
    diag.start_pass();

    self.load_imports_go(0, None, loader)?;

    while let Some(idx) = self.load_queue.pop_front() {
      let parent_dir = {
        let book = self.books[idx].borrow();
        book.source.rsplit_once('/').map(|(s, _)| Name::new(s))
      };
      self.load_imports_go(idx, parent_dir, loader)?;
    }

    for idx in 0..self.books.len() {
      self.load_binds(idx, diag);
    }

    let (_, book) = self.books.swap_remove_index(0).unwrap();

    diag.fatal(book.into_inner())
  }

  fn load_imports_go(
    &mut self,
    idx: usize,
    dir: Option<Name>,
    loader: &mut impl PackageLoader,
  ) -> Result<(), Diagnostics> {
    let mut sources = IndexMap::new();

    {
      let mut book = self.books[idx].borrow_mut();
      let names = &mut book.import_ctx.imports;

      for import in names {
        if import.relative {
          if let Some(ref dir) = dir {
            let path = format!("{}/{}", dir, import.path);
            let normalized = normalize_path(&PathBuf::from(path));
            import.path = Name::new(normalized.to_string_lossy());
          }
        }

        let loaded = loader.load(import)?;
        sources.extend(loaded);
      }
    }

    for (psrc, code) in sources {
      let module = do_parse_book(&code, &PathBuf::from(psrc.as_ref()), ParseBook::default())?;
      self.load_queue.push_back(self.books.len());
      self.books.insert(psrc, module.into());
    }

    Ok(())
  }

  /// Maps the `ImportType` of each import to the top level names it relates,
  /// checks if it is valid, resolves `BoundSource::Either`, and adds to the book ImportMap.
  fn load_binds(&mut self, idx: usize, diag: &mut Diagnostics) {
    let book = &mut self.books[idx].borrow_mut();
    let ImportCtx { imports, map } = &mut book.import_ctx;

    for import in imports {
      match (&mut import.src, &import.imp_type) {
        (BoundSource::Either(src, pkgs), ImportType::Single(nam, alias)) => {
          if self.unique_top_level_names(src).contains(nam) {
            let err = format!("Both file '{src}.bend' and folder '{src}' contains the import '{nam}'");
            diag.add_book_error(err);
            continue;
          }

          self.add_file_from_dir(pkgs, nam, alias, map, diag);
          import.src = BoundSource::Dir(std::mem::take(pkgs));
        }

        (BoundSource::Either(src, pkgs), ImportType::List(names)) => {
          for (name, alias) in names {
            let added = self.add_file_from_dir(pkgs, name, alias, map, diag);

            if !added {
              if !self.unique_top_level_names(src).contains(name) {
                let err = format!("Package '{src}' does not contain the top level name '{name}'");
                diag.add_book_error(err);
                continue;
              }

              pkgs.insert(name.clone(), src.clone());
              map.add_aliased_bind(src, name, alias.as_ref(), diag)
            }
          }

          import.src = BoundSource::Dir(std::mem::take(pkgs));
        }

        (BoundSource::Either(src, pkgs), ImportType::Glob) => {
          let names = self.unique_top_level_names(src);
          let mut error = false;

          for nam in pkgs.keys() {
            if names.contains(nam) {
              let err = format!("Both file '{src}.bend' and folder '{src}' contains the import '{nam}'");
              diag.add_book_error(err);
              error = true;
            }
          }

          if error {
            continue;
          }

          self.add_glob_from_dir(pkgs, map, diag);

          for sub in &names {
            pkgs.insert(sub.clone(), src.clone());
          }

          map.add_binds(&names, src, diag);

          import.src = BoundSource::Dir(std::mem::take(pkgs));
        }

        (BoundSource::File(src), ImportType::Single(nam, alias)) => {
          if !self.unique_top_level_names(src).contains(nam) {
            let err = format!("Package '{src}' does not contain the top level name '{nam}'");
            diag.add_book_error(err);
            continue;
          }

          map.add_aliased_bind(src, nam, alias.as_ref(), diag)
        }

        (BoundSource::File(src), ImportType::List(names)) => {
          let src_names = self.unique_top_level_names(src);
          let mut error = false;

          for (sub, _) in names {
            if !src_names.contains(sub) {
              let err = format!("Package '{src}' does not contain the top level name '{sub}'");
              diag.add_book_error(err);
              error = true;
            }
          }

          if error {
            continue;
          }

          map.add_aliased_binds(names, src, diag);
        }

        (BoundSource::File(src), ImportType::Glob) => {
          let names = self.unique_top_level_names(src);
          map.add_binds(&names, src, diag);
        }

        (BoundSource::Dir(pkgs), ImportType::Single(nam, alias)) => {
          self.add_file_from_dir(pkgs, nam, alias, map, diag);
        }

        (BoundSource::Dir(pkgs), ImportType::List(names)) => {
          for (nam, alias) in names {
            self.add_file_from_dir(pkgs, nam, alias, map, diag);
          }
        }

        (BoundSource::Dir(pkgs), ImportType::Glob) => {
          self.add_glob_from_dir(pkgs, map, diag);
        }

        (BoundSource::None, _) => unreachable!(),
      }
    }
  }

  fn add_file_from_dir(
    &self,
    pkgs: &IndexMap<Name, Name>,
    nam: &Name,
    alias: &Option<Name>,
    map: &mut ImportsMap,
    diag: &mut Diagnostics,
  ) -> bool {
    if let Some(src) = pkgs.get(nam) {
      let names = self.unique_top_level_names(src);
      map.add_nested_binds(src, nam, alias.as_ref(), names, diag);
      true
    } else {
      false
    }
  }

  fn add_glob_from_dir(&self, pkgs: &IndexMap<Name, Name>, map: &mut ImportsMap, diag: &mut Diagnostics) {
    for (nam, src) in pkgs {
      let names = self.unique_top_level_names(src);
      map.add_nested_binds(src, nam, None, names, diag);
    }
  }

  fn unique_top_level_names(&self, src: &Name) -> IndexSet<Name> {
    let bound_book = self.books.get(src).unwrap().borrow();
    bound_book.top_level_names().cloned().collect()
  }
}
