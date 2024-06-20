use crate::{
  diagnostics::{Diagnostics, WarningType},
  fun::Name,
};
use indexmap::IndexMap;
use itertools::Itertools;
use std::fmt::Display;

pub mod book;
pub mod loader;
pub mod packages;

pub use loader::*;

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
        BoundSource::None => {}
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

#[derive(Debug, Clone)]
pub enum BoundSource {
  None,
  File(Name),
  Folder(Vec<Name>),
}

#[derive(Debug, Clone, Default)]
struct ImportsMap {
  binds: IndexMap<Name, Name>,
}

impl ImportsMap {
  pub fn contains_source(&self, s: &Name) -> bool {
    self.binds.values().contains(s)
  }

  fn add_bind(&mut self, bind: Name, src: &str, diag: &mut Diagnostics) {
    if let Some(old) = self.binds.get(&bind) {
      let warn = format!("The import '{src}' shadows the imported name '{old}'");
      diag.add_book_warning(warn, WarningType::ImportShadow);
    }

    self.binds.insert(bind, Name::new(src));
  }
}
