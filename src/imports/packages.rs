use super::{loader::PackageLoader, BoundSource, Import, ImportType};
use crate::{
  diagnostics::Diagnostics,
  fun::{load_book::do_parse_book, parser::ParseBook, Name},
};
use indexmap::IndexMap;
use itertools::Itertools;
use std::{
  collections::{HashSet, VecDeque},
  path::{Component, Path, PathBuf},
};

#[derive(Default)]
pub struct Packages {
  pub books: IndexMap<Name, ParseBook>,
  pub loaded_adts: IndexMap<Name, IndexMap<Name, Vec<Name>>>,
  pub load_queue: VecDeque<usize>,
}

impl Packages {
  pub fn new(book: ParseBook) -> Self {
    Self {
      books: IndexMap::from([(Name::default(), book)]),
      load_queue: VecDeque::new(),
      loaded_adts: IndexMap::new(),
    }
  }

  fn get_book(&self, idx: usize) -> &ParseBook {
    self.books.get_index(idx).unwrap().1
  }

  fn get_book_mut(&mut self, idx: usize) -> &mut ParseBook {
    self.books.get_index_mut(idx).unwrap().1
  }

  pub fn load_imports(
    &mut self,
    loader: &mut impl PackageLoader,
    diag: &mut Diagnostics,
  ) -> Result<ParseBook, Diagnostics> {
    diag.start_pass();

    self.load_imports_go(0, None, loader)?;

    while let Some(idx) = self.load_queue.pop_front() {
      let psrc = &self.get_book(idx).source;
      let parent_dir = psrc.rsplit_once('/').map(|(s, _)| Name::new(s));
      self.load_imports_go(idx, parent_dir, loader)?;
    }

    for idx in 0..self.books.len() {
      self.load_binds(idx, diag);
    }

    let (_, book) = self.books.swap_remove_index(0).unwrap();

    diag.fatal(book)
  }

  fn load_imports_go(
    &mut self,
    idx: usize,
    dir: Option<Name>,
    loader: &mut impl PackageLoader,
  ) -> Result<(), Diagnostics> {
    let names = &mut self.get_book_mut(idx).import_ctx.imports;
    let mut sources = IndexMap::new();

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

    for (psrc, code) in sources {
      let module = do_parse_book(&code, &psrc, ParseBook::default())?;
      self.load_queue.push_back(self.books.len());
      self.books.insert(psrc, module);
    }

    Ok(())
  }

  fn load_binds(&mut self, idx: usize, diag: &mut Diagnostics) {
    let book = self.get_book(idx);
    let imports = book.import_ctx.imports.clone();

    for Import { imp_type, src: pkgs, .. } in imports {
      match (pkgs, imp_type) {
        (BoundSource::File(src), ImportType::Simple(nam, alias)) => {
          let bound_book = self.books.get(&src).unwrap();

          if !bound_book.top_level_names().contains(&nam) {
            let err = format!("Package '{src}' does not contain the top level name '{nam}'");
            diag.add_book_error(err);
          }

          let book = self.get_book_mut(idx);

          let src = format!("{}/{}", src, nam);
          let aliased = alias.unwrap_or(nam);
          book.import_ctx.map.add_bind(aliased, &src, diag);
        }

        (BoundSource::File(src), ImportType::List(names)) => {
          let bound_book = self.books.get(&src).unwrap();

          for (sub, _) in &names {
            if !bound_book.top_level_names().contains(&sub) {
              let err = format!("Package '{src}' does not contain the top level name '{sub}'");
              diag.add_book_error(err);
            }
          }

          let book = self.get_book_mut(idx);

          for (sub, alias) in names {
            let src = format!("{}/{}", src, sub);
            let aliased = alias.unwrap_or(sub);
            book.import_ctx.map.add_bind(aliased, &src, diag);
          }
        }

        (BoundSource::File(src), ImportType::Glob) => {
          let bound_book = self.books.get(&src).unwrap();
          let names: HashSet<_> = bound_book.top_level_names().cloned().collect();

          let book = self.get_book_mut(idx);

          for sub in names {
            let src = format!("{}/{}", src, sub);
            book.import_ctx.map.add_bind(sub, &src, diag);
          }
        }

        (BoundSource::Folder(mut src), ImportType::Simple(nam, alias)) => {
          let src = src.pop().unwrap();
          self.add_book_bind(idx, src, nam, alias, diag);
        }

        (BoundSource::Folder(pkgs), ImportType::List(names)) => {
          for (src, (nam, alias)) in pkgs.into_iter().zip_eq(names) {
            self.add_book_bind(idx, src, nam, alias, diag);
          }
        }

        (BoundSource::Folder(pkgs), ImportType::Glob) => {
          for src in pkgs {
            let nam = Name::new(src.split('/').last().unwrap());
            self.add_book_bind(idx, src, nam, None, diag);
          }
        }

        (BoundSource::None, _) => unreachable!(),
      }
    }
  }

  fn add_book_bind(&mut self, idx: usize, src: Name, nam: Name, alias: Option<Name>, diag: &mut Diagnostics) {
    let bound_book = self.books.get(&src).unwrap();
    let names: HashSet<_> = bound_book.top_level_names().cloned().collect();

    let aliased = alias.as_ref().unwrap_or(&nam);

    let book = self.get_book_mut(idx);

    for name in &names {
      if name != &nam {
        let src = format!("{}/{}", src, name);
        let bind = Name::new(format!("{aliased}/{name}"));
        book.import_ctx.map.add_bind(bind, &src, diag);
      }
    }

    if names.contains(&nam) {
      let src = format!("{}/{}", src, nam);
      book.import_ctx.map.add_bind(aliased.clone(), &src, diag);
    }
  }
}

// Taken from 'cargo/util/paths.rs'
fn normalize_path(path: &Path) -> PathBuf {
  let mut components = path.components().peekable();
  let mut ret = if let Some(c @ Component::Prefix(..)) = components.peek().cloned() {
    components.next();
    PathBuf::from(c.as_os_str())
  } else {
    PathBuf::new()
  };

  for component in components {
    match component {
      Component::Prefix(..) => unreachable!(),
      Component::RootDir => {
        ret.push(component.as_os_str());
      }
      Component::CurDir => {}
      Component::ParentDir => {
        ret.pop();
      }
      Component::Normal(c) => {
        ret.push(c);
      }
    }
  }
  ret
}
