use crate::{
  diagnostics::{Diagnostics, DiagnosticsConfig, WarningType},
  fun::{load_book::do_parse_book, parser::ParseBook, Adt, Book, Definition, Name, Rule, Source, Term},
  imp::{Expr, Stmt},
};
use indexmap::{map::Entry, IndexMap};
use itertools::Itertools;
use std::{
  collections::{HashSet, VecDeque},
  fmt::Display,
  path::{Path, PathBuf},
};

#[derive(Debug, Clone, Default)]
pub struct ImportCtx {
  /// Imports declared in the program source.
  imports: Vec<Import>,

  /// Map from bound names to source package.
  map: ImportsMap,
}

impl ImportCtx {
  pub fn add_import(&mut self, import: Import) {
    self.imports.push(import);
  }

  pub fn to_imports(self) -> Vec<Import> {
    self.imports
  }

  pub fn sources(&self) -> Vec<&Name> {
    let mut names = Vec::new();
    for imps in &self.imports {
      match &imps.src {
        BoundSource::None => todo!(),
        BoundSource::File(f) => names.push(f),
        BoundSource::Folder(v) => names.extend(v),
      }
    }
    names
  }
}

#[derive(Debug, Clone)]
pub struct Import {
  path: Name,
  imp_type: ImportType,
  relative: bool,
  src: BoundSource,
}

#[derive(Debug, Clone)]
pub enum BoundSource {
  None,
  File(Name),
  Folder(Vec<Name>),
}

impl Import {
  pub fn new(path: Name, imp_type: ImportType, relative: bool) -> Self {
    Self { path, imp_type, relative, src: BoundSource::None }
  }
}

#[derive(Debug, Clone)]
pub enum ImportType {
  Simple(Name, Option<Name>),
  List(Vec<(Name, Option<Name>)>),
  Glob,
}

impl Display for ImportType {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ImportType::Simple(n, _) => write!(f, "{n}"),
      ImportType::List(l) => write!(f, "({})", l.iter().map(|(n, _)| n).join(", ")),
      ImportType::Glob => write!(f, "*"),
    }
  }
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

  fn add_bind(&mut self, bind: Name, src: &str, diag: &mut Diagnostics) {
    if let Some(old) = self.binds.get(&bind) {
      let old = &self.sources[*old];
      let warn = format!("The import '{src}' shadows the imported name '{old}'");
      diag.add_book_warning(warn, WarningType::ImportShadow);
    }

    self.binds.insert(bind, self.sources.len());
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
    let names = &mut self.get_book_mut(idx).import_ctx.imports;
    let mut sources = IndexMap::new();

    for import in names {
      if import.relative {
        if let Some(ref dir) = dir {
          import.path = Name::new(format!("{}/{}", dir, import.path));
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

    let sources = self.import_ctx.sources().into_iter().cloned().collect_vec();

    for src in sources {
      let Some(mut package) = pkgs.books.swap_remove(&src) else { continue };

      // Can not be done outside the loop/function because of the borrow checker.
      // Just serves to pass only the import map of the first call to `apply_imports_go`.
      let main_imports = main_imports.unwrap_or(&self.import_ctx.map);

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
    _diag: &mut Diagnostics,
    pkgs: &Packages,
  ) {
    // Can not be done outside the function because of the borrow checker.
    // Just serves to pass only the import map of the first call to `apply_imports_go`.
    let main_imports = main_imports.unwrap_or(&self.import_ctx.map);

    let mut local_imports: IndexMap<Name, Name> = IndexMap::new();

    // Collect local imports binds, starting with `__` if not imported by the main book.
    'outer: for (bind, src) in self.import_ctx.map.iter().rev() {
      if self.contains_def(bind) | self.ctrs.contains_key(bind) | self.adts.contains_key(bind) {
        // TODO: Here we should show warnings for shadowing of imported names by local def/ctr/adt
        // It can be done, but when importing with `ImportType::Simple` files in the same folder,
        // it gives a false positive warning
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

      for pkg in self.import_ctx.sources() {
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
  fn load(&mut self, import: &mut Import) -> Result<Sources, String>;
  fn is_loaded(&self, name: &Name) -> bool;
}

pub struct DefaultLoader {
  local_path: PathBuf,
  loaded: HashSet<Name>,
}

impl DefaultLoader {
  pub fn new(local_path: &Path) -> Self {
    let local_path = local_path.parent().unwrap().to_path_buf();
    Self { local_path, loaded: HashSet::new() }
  }

  fn read_file(&mut self, path: &Path, file: Name, src: &mut Sources) -> Option<Name> {
    if !self.is_loaded(&file) {
      self.loaded.insert(file.clone());

      let path = path.with_extension("bend");
      let code = std::fs::read_to_string(path).ok()?;
      src.insert(file.clone(), code);
    }
    Some(file)
  }

  fn read_file_in_folder(
    &mut self,
    full_path: &Path,
    folder: &str,
    file_name: &str,
    src: &mut Sources,
  ) -> Option<Name> {
    let file_path = Name::new(format!("{}/{}", folder, file_name));
    let full_path = full_path.join(file_name);

    self.read_file(&full_path, file_path, src)
  }

  fn read_path(
    &mut self,
    base_path: &Path,
    path: &Name,
    imp_type: &ImportType,
  ) -> Option<(BoundSource, Sources)> {
    let full_path = base_path.join(path.as_ref());
    let mut src = IndexMap::new();

    if full_path.with_extension("bend").is_file() {
      if let Some(nam) = self.read_file(&full_path, path.clone(), &mut src) {
        return Some((BoundSource::File(nam), src));
      }
    }

    if full_path.is_dir() || path.is_empty() {
      let mut names = Vec::new();

      match imp_type {
        ImportType::Simple(file, _) => {
          let name = self.read_file_in_folder(&full_path, path, file, &mut src)?;
          names.push(name);
        }
        ImportType::List(list) => {
          for (file, _) in list {
            let name = self.read_file_in_folder(&full_path, path, file, &mut src)?;
            names.push(name);
          }
        }
        ImportType::Glob => {
          for entry in full_path.read_dir().unwrap().flatten() {
            let file = PathBuf::from(&entry.file_name());

            if let Some("bend") = file.extension().and_then(|f| f.to_str()) {
              let file = file.file_stem().unwrap().to_string_lossy();
              let name = self.read_file_in_folder(&full_path, path, &file, &mut src)?;
              names.push(name);
            }
          }
        }
      }

      return Some((BoundSource::Folder(names), src));
    }

    None
  }
}

pub const PATH: &[&str] = &[".bend"];

impl PackageLoader for DefaultLoader {
  fn load(&mut self, import: &mut Import) -> Result<Sources, String> {
    let mut sources = Sources::new();

    let Import { path, imp_type, relative, src } = import;

    let folders = if *relative {
      vec![self.local_path.clone()]
    } else {
      PATH.iter().map(|p| self.local_path.join(p)).collect()
    };

    for base in folders {
      let Some((names, new_pkgs)) = self.read_path(&base, path, imp_type) else { continue };

      *src = names;
      sources.extend(new_pkgs);
      break;
    }

    if let BoundSource::None = src {
      return Err(format!("Failed to import '{}' from '{}'", imp_type, path).to_string());
    }

    Ok(sources)
  }

  fn is_loaded(&self, name: &Name) -> bool {
    self.loaded.contains(name)
  }
}
