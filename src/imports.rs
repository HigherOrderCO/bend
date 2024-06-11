use crate::{
  diagnostics::{Diagnostics, DiagnosticsConfig, WarningType},
  fun::{load_book::do_parse_book, parser::ParseBook, Adt, Book, Definition, Name, Rule, Source, Term},
  imp::{Expr, Stmt},
};
use indexmap::{map::Entry, IndexMap};
use itertools::Itertools;
use std::{
  collections::{HashSet, VecDeque},
  path::PathBuf,
};

#[derive(Debug, Clone, Default)]
pub struct Imports {
  /// Imports declared in the program source.
  names: Vec<(Name, ImportType)>,

  /// Map from bound names to source package.
  map: ImportsMap,

  /// Imported packages names.
  pkgs: Vec<Name>,
}

impl Imports {
  pub fn add_import(&mut self, import: Name, import_type: ImportType) {
    self.names.push((import, import_type));
  }

  pub fn to_names(self) -> Vec<(Name, ImportType)> {
    self.names
  }
}

#[derive(Debug, Clone)]
pub enum ImportType {
  Simple(Option<Name>),
  List(Vec<(Name, Option<Name>)>),
  Glob,
}

#[derive(Debug, Clone, Default)]
struct ImportsMap {
  binds: IndexMap<Name, usize>,
  sources: Vec<Name>,
}

impl ImportsMap {
  fn iter(&self) -> impl DoubleEndedIterator<Item = (&Name, &Name)> {
    self.binds.iter().map(|(n, u)| (n, &self.sources[*u]))
  }

  fn add_bind(&mut self, name: Name, alias: Option<Name>, src: &str, diag: &mut Diagnostics) {
    let aliased = alias.unwrap_or(name);

    if let Some(old) = self.binds.get(&aliased) {
      let old = &self.sources[*old];
      let warn = format!("The import '{src}' shadows the imported name '{old}'");
      diag.add_book_warning(warn, WarningType::ImportShadow);
    }

    self.binds.insert(aliased, self.sources.len());
    self.sources.push(Name::new(src));
  }
}

#[derive(Default)]
struct Packages {
  books: IndexMap<Name, ParseBook>,
  loaded_adts: IndexMap<Name, IndexMap<Name, Vec<Name>>>,
  load_queue: VecDeque<usize>,
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
    let names = self.get_book(idx).imports.names.clone();

    for (src, imp_type) in names {
      // TODO: Would this be correct when handling non-local imports? Do we want relative online sources?
      // TODO: Normalize paths when using `..`
      let src = dir.as_ref().map_or(src.clone(), |dir| Name::new(format!("{}/{}", dir, src)));

      let mut sources = IndexMap::new();
      let packages = loader.load(src.clone(), &imp_type, &mut sources)?;

      self.get_book_mut(idx).imports.pkgs.extend(packages);

      for (psrc, code) in sources {
        let module = do_parse_book(&code, &psrc, ParseBook::default())?;

        self.load_queue.push_back(self.books.len());
        self.books.insert(psrc, module);
      }
    }
    Ok(())
  }

  fn load_binds(&mut self, idx: usize, diag: &mut Diagnostics) {
    let book = self.get_book(idx);
    let pkgs = book.imports.pkgs.iter();
    let names = book.imports.names.iter();
    let binds = pkgs.zip(names).map(|(src, (_, imp))| (src.clone(), imp.clone())).collect_vec();

    for (src, imp_type) in binds {
      match imp_type {
        ImportType::Simple(alias) => {
          let name = Name::new(src.split('/').last().unwrap());
          let src = format!("{}/{}", src, name);

          let book = self.get_book_mut(idx);
          book.imports.map.add_bind(name, alias, &src, diag);
        }
        ImportType::List(names) => {
          let bound_book = self.books.get(&src).unwrap();

          for (sub, _) in &names {
            if !bound_book.top_level_names().contains(&sub) {
              let err = format!("Package '{src}' does not contain the top level name '{sub}'");
              diag.add_book_error(err);
              continue;
            }
          }

          let book = self.get_book_mut(idx);

          for (sub, alias) in names {
            let src = format!("{}/{}", src, sub);
            book.imports.map.add_bind(sub, alias, &src, diag);
          }
        }
        ImportType::Glob => {
          let bound_book = self.books.get(&src).unwrap();
          let names = bound_book.top_level_names().cloned().collect_vec();

          let book = self.get_book_mut(idx);

          for sub in names {
            let src = format!("{}/{}", src, sub);
            book.imports.map.add_bind(sub, None, &src, diag);
          }
        }
      }
    }
  }
}

impl ParseBook {
  pub fn load_imports(
    self,
    mut loader: impl PackageLoader,
    diag_config: DiagnosticsConfig,
  ) -> Result<Book, Diagnostics> {
    let diag = &mut Diagnostics::new(diag_config);
    let pkgs = &mut Packages::new(self);
    let mut book = pkgs.load_imports(&mut loader, diag)?;

    book.apply_imports(None, diag, pkgs)?;
    eprint!("{}", diag);

    let mut book = book.to_fun()?;
    book.desugar_ctr_use();
    Ok(book)
  }

  fn apply_imports(
    &mut self,
    main_imports: Option<&ImportsMap>,
    diag: &mut Diagnostics,
    pkgs: &mut Packages,
  ) -> Result<(), Diagnostics> {
    self.load_packages(main_imports, diag, pkgs)?;
    self.apply_import_binds(main_imports, diag, pkgs);
    Ok(())
  }

  /// Consumes the book imported packages,
  /// applying the imports recursively of every nested book.
  fn load_packages(
    &mut self,
    main_imports: Option<&ImportsMap>,
    diag: &mut Diagnostics,
    pkgs: &mut Packages,
  ) -> Result<(), Diagnostics> {
    diag.start_pass();

    for src in self.imports.pkgs.clone() {
      let Some(mut package) = pkgs.books.swap_remove(&src) else { continue };

      // Can not be done outside the loop/function because of the borrow checker.
      // Just serves to pass only the import map of the first call to `apply_imports_go`.
      let main_imports = main_imports.unwrap_or(&self.imports.map);

      package.apply_imports(Some(main_imports), diag, pkgs)?;
      let new_adts = package.apply_adts(&src, main_imports);
      package.apply_defs(&src, main_imports);

      let book = package.to_fun()?;

      for (name, adt) in new_adts {
        let adts = pkgs.loaded_adts.entry(src.clone()).or_default();
        adts.insert(name.clone(), adt.ctrs.keys().cloned().collect_vec());
        self.add_imported_adt(name, adt, diag);
      }

      for def in book.defs.into_values() {
        self.add_imported_def(def, diag);
      }
    }

    diag.fatal(())
  }

  /// Applies a chain of `use bind = src` to every local definition.
  ///
  /// Must be used after `load_packages`
  fn apply_import_binds(
    &mut self,
    main_imports: Option<&ImportsMap>,
    diag: &mut Diagnostics,
    pkgs: &Packages,
  ) {
    // Can not be done outside the function because of the borrow checker.
    // Just serves to pass only the import map of the first call to `apply_imports_go`.
    let main_imports = main_imports.unwrap_or(&self.imports.map);

    let mut local_imports: IndexMap<Name, Name> = IndexMap::new();

    // Collect local imports binds, starting with `__` if not imported by the main book.
    'outer: for (bind, src) in self.imports.map.iter().rev() {
      if self.contains_def(bind) {
        let warn = format!("The local definition '{bind}' shadows the imported name '{src}'");
        diag.add_book_warning(warn, WarningType::ImportShadow);
        continue;
      }

      if self.ctrs.contains_key(bind) {
        let warn = format!("The local constructor '{bind}' shadows the imported name '{src}'");
        diag.add_book_warning(warn, WarningType::ImportShadow);
        continue;
      }

      if self.adts.contains_key(bind) {
        let warn = format!("The local type '{bind}' shadows the imported name '{src}'");
        diag.add_book_warning(warn, WarningType::ImportShadow);
        continue;
      }

      let nam =
        if main_imports.sources.contains(src) { src.clone() } else { Name::new(format!("__{}", src)) };

      if let Some(adt) = self.adts.get(&nam) {
        for (ctr, _) in adt.ctrs.iter().rev() {
          add_bind_to_map(&mut local_imports, bind, src, ctr);
        }
        continue;
      }

      for pkg in &self.imports.pkgs {
        if let Some(book) = pkgs.loaded_adts.get(pkg) {
          if let Some(ctrs) = book.get(&nam) {
            for ctr in ctrs.iter().rev() {
              add_bind_to_map(&mut local_imports, bind, src, ctr);
            }
            continue 'outer;
          }
        }
      }

      local_imports.insert(bind.clone(), nam);
    }

    for def in self.fun_defs.values_mut().filter(|d| matches!(d.source, Source::Local(..))) {
      for rule in &mut def.rules {
        rename_ctr_patterns(rule, &local_imports);
        rule.body = std::mem::take(&mut rule.body).fold_uses(local_imports.iter());
      }
    }

    for (def, _) in self.imp_defs.values_mut().filter(|(_, source)| matches!(source, Source::Local(..))) {
      def.body = std::mem::take(&mut def.body).fold_uses(local_imports.iter());
    }
  }

  /// Consumes the book adts, applying the necessary naming transformations
  /// adding `use ctr = ctr_src` chains to every local definition.
  fn apply_adts(&mut self, src: &Name, main_imp: &ImportsMap) -> IndexMap<Name, Adt> {
    let adts = std::mem::take(&mut self.adts);
    let mut new_adts = IndexMap::new();
    let mut ctrs_map = IndexMap::new();

    for (mut name, mut adt) in adts {
      match adt.source {
        Source::Local(..) => {
          adt.source = Source::Imported;
          name = Name::new(format!("{}/{}", src, name));

          let mangle_name = !main_imp.sources.contains(&name);
          let mut mangle_adt_name = mangle_name;

          for (ctr, f) in std::mem::take(&mut adt.ctrs) {
            let mut ctr_name = Name::new(format!("{}/{}", src, ctr));

            let mangle_ctr = mangle_name && !main_imp.sources.contains(&ctr_name);

            if mangle_ctr {
              mangle_adt_name = true;
              ctr_name = Name::new(format!("__{}", ctr_name));
            }

            ctrs_map.insert(ctr, ctr_name.clone());
            adt.ctrs.insert(ctr_name, f);
          }

          if mangle_adt_name {
            name = Name::new(format!("__{}", name));
          }
        }

        Source::Imported => {}

        Source::Builtin | Source::Generated => {
          unreachable!("No builtin or generated adt should be present at this step")
        }
      }

      new_adts.insert(name.clone(), adt);
    }

    for def in self.fun_defs.values_mut().filter(|d| matches!(d.source, Source::Local(..))) {
      for rule in &mut def.rules {
        rename_ctr_patterns(rule, &ctrs_map);
        rule.body = std::mem::take(&mut rule.body).fold_uses(ctrs_map.iter().rev());
      }
    }

    for (def, _) in self.imp_defs.values_mut().filter(|(_, source)| matches!(source, Source::Local(..))) {
      def.body = std::mem::take(&mut def.body).fold_uses(ctrs_map.iter().rev());
    }

    new_adts
  }

  /// Apply the necessary naming transformations to the book definitions,
  /// adding `use def = def_src` chains to every local definition.
  fn apply_defs(&mut self, src: &Name, main_imp: &ImportsMap) {
    let mut def_map: IndexMap<_, _> = IndexMap::new();

    // Rename the definitions to their source name
    // Starting with `__` if not imported by the main book.
    for def in self.fun_defs.values_mut() {
      update_name(&mut def.name, def.source.clone(), src, main_imp, &mut def_map);
    }

    for (def, source) in self.imp_defs.values_mut() {
      update_name(&mut def.name, source.clone(), src, main_imp, &mut def_map);
    }

    for (nam, def) in &mut self.fun_defs {
      if let Source::Local(..) = def.source {
        for rule in &mut def.rules {
          let bod = std::mem::take(&mut rule.body);
          rule.body = bod.fold_uses(def_map.iter().rev().filter(|(n, _)| n != &nam));
        }
        def.source = Source::Imported;
      }
    }

    for (nam, (def, source)) in &mut self.imp_defs {
      if let Source::Local(..) = source {
        let bod = std::mem::take(&mut def.body);
        def.body = bod.fold_uses(def_map.iter().rev().filter(|(n, _)| n != &nam));
        *source = Source::Imported;
      }
    }
  }

  fn add_imported_adt(&mut self, nam: Name, adt: Adt, diag: &mut Diagnostics) {
    if self.adts.get(&nam).is_some() {
      let err = format!("The imported datatype '{nam}' conflicts with the datatype '{nam}'.");
      diag.add_book_error(err);
    } else {
      for ctr in adt.ctrs.keys() {
        if self.contains_def(ctr) {
          let err = format!("The imported constructor '{ctr}' conflicts with the definition '{ctr}'.");
          diag.add_book_error(err);
        }
        match self.ctrs.entry(ctr.clone()) {
          Entry::Vacant(e) => _ = e.insert(nam.clone()),
          Entry::Occupied(e) => {
            let ctr = e.key();
            let err = format!("The imported constructor '{ctr}' conflicts with the constructor '{ctr}'.");
            diag.add_book_error(err);
          }
        }
      }
      self.adts.insert(nam, adt);
    }
  }

  fn add_imported_def(&mut self, def: Definition, diag: &mut Diagnostics) {
    let name = &def.name;
    if self.contains_def(name) {
      let err = format!("The imported definition '{name}' conflicts with the definition '{name}'.");
      diag.add_book_error(err);
    } else if self.ctrs.contains_key(name) {
      let err = format!("The imported definition '{name}' conflicts with the constructor '{name}'.");
      diag.add_book_error(err);
    }

    self.fun_defs.insert(name.clone(), def);
  }

  fn top_level_names(&self) -> impl Iterator<Item = &Name> {
    let imp_defs = self.imp_defs.keys();
    let fun_defs = self.fun_defs.keys();
    let adts = self.adts.keys();
    let ctrs = self.ctrs.keys();

    imp_defs.chain(fun_defs).chain(adts).chain(ctrs)
  }
}

fn rename_ctr_patterns(rule: &mut Rule, map: &IndexMap<Name, Name>) {
  for pat in &mut rule.pats {
    for bind in pat.binds_mut().flatten() {
      if let Some(alias) = map.get(bind) {
        *bind = alias.clone();
      }
    }
  }
}

fn add_bind_to_map(map: &mut IndexMap<Name, Name>, bind: &Name, src: &Name, ctr: &Name) {
  let full_ctr_name = ctr.split("__").nth(1).unwrap_or(ctr.as_ref());
  let ctr_name = full_ctr_name.strip_prefix(src.as_ref()).unwrap();
  let bind = Name::new(format!("{}{}", bind, ctr_name));
  map.insert(bind, ctr.clone());
}

fn update_name(
  def_name: &mut Name,
  def_source: Source,
  src: &Name,
  main_imp: &ImportsMap,
  def_map: &mut IndexMap<Name, Name>,
) {
  match def_source {
    Source::Local(..) => {
      let mut new_name = Name::new(format!("{}/{}", src, def_name));

      if !main_imp.sources.contains(&new_name) {
        new_name = Name::new(format!("__{}", new_name));
      }

      def_map.insert(def_name.clone(), new_name.clone());
      *def_name = new_name;
    }

    Source::Imported => {}

    Source::Builtin | Source::Generated => {
      unreachable!("No builtin or generated definition should be present at this step")
    }
  }
}

impl Term {
  fn fold_uses<'a>(self, map: impl Iterator<Item = (&'a Name, &'a Name)>) -> Self {
    map.fold(self, |acc, (bind, nam)| Self::Use {
      nam: Some(bind.clone()),
      val: Box::new(Self::Var { nam: nam.clone() }),
      nxt: Box::new(acc),
    })
  }
}

impl Stmt {
  fn fold_uses<'a>(self, map: impl Iterator<Item = (&'a Name, &'a Name)>) -> Self {
    map.fold(self, |acc, (bind, nam)| Self::Use {
      nam: bind.clone(),
      val: Box::new(Expr::Var { nam: nam.clone() }),
      nxt: Box::new(acc),
    })
  }
}

type Sources = IndexMap<Name, String>;

pub trait PackageLoader {
  fn load(&mut self, name: Name, import_type: &ImportType, pkgs: &mut Sources) -> Result<Vec<Name>, String>;
  fn is_loaded(&self, name: &Name) -> bool;
}

pub struct DefaultLoader {
  pub local_path: PathBuf,
  pub loaded: HashSet<Name>,
}

impl DefaultLoader {
  pub fn new(local_path: PathBuf) -> Self {
    Self { local_path, loaded: HashSet::new() }
  }
}

impl PackageLoader for DefaultLoader {
  fn load(&mut self, name: Name, _import_type: &ImportType, pkgs: &mut Sources) -> Result<Vec<Name>, String> {
    if !self.is_loaded(&name) {
      // TODO: Should the local filesystem be searched anyway for each sub_name?
      // TODO: If the name to load is a folder, and we have sub names, we should load those files instead?
      self.loaded.insert(name.clone());
      let path = self.local_path.parent().unwrap().join(name.as_ref()).with_extension("bend");
      let code = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
      pkgs.insert(name.clone(), code);
    }

    Ok(vec![name])
  }

  fn is_loaded(&self, name: &Name) -> bool {
    self.loaded.contains(name)
  }
}
