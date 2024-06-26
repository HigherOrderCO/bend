use crate::{
  diagnostics::{Diagnostics, WarningType},
  fun::Name,
};
use indexmap::{IndexMap, IndexSet};
use itertools::Itertools;
use std::fmt::Display;

pub mod book;
pub mod loader;
pub mod packages;

pub use loader::*;

pub type BindMap = IndexMap<Name, Name>;

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
        BoundSource::Dir(v) => names.extend(v.values()),
        BoundSource::Either(_, _) => unreachable!("This should be resolved into `File` or `Dir` by now"),
      }
    }
    names
  }
}

#[derive(Debug, Clone)]
pub struct Import {
  pub path: Name,
  pub imp_type: ImportType,
  pub relative: bool,
  pub src: BoundSource,
}

impl Import {
  pub fn new(path: Name, imp_type: ImportType, relative: bool) -> Self {
    Self { path, imp_type, relative, src: BoundSource::None }
  }
}

#[derive(Debug, Clone)]
pub enum ImportType {
  Single(Name, Option<Name>),
  List(Vec<(Name, Option<Name>)>),
  Glob,
}

impl Display for ImportType {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ImportType::Single(n, _) => write!(f, "{n}"),
      ImportType::List(l) => write!(f, "({})", l.iter().map(|(n, _)| n).join(", ")),
      ImportType::Glob => write!(f, "*"),
    }
  }
}

#[derive(Debug, Clone)]
pub enum BoundSource {
  None,
  File(Name),
  Dir(IndexMap<Name, Name>),
  /// If the bound source is ambiguous between a file or a directory
  Either(Name, IndexMap<Name, Name>),
}

#[derive(Debug, Clone, Default)]
struct ImportsMap {
  binds: BindMap,
}

impl ImportsMap {
  pub fn contains_source(&self, s: &Name) -> bool {
    self.binds.values().contains(s)
  }

  fn add_bind(&mut self, src: &str, bind: Name, diag: &mut Diagnostics) {
    if let Some(old) = self.binds.get(&bind) {
      let warn = format!("The import '{src}' shadows the imported name '{old}'");
      diag.add_book_warning(warn, WarningType::ImportShadow);
    }

    self.binds.insert(bind, Name::new(src));
  }

  fn add_aliased_bind(&mut self, src: &Name, sub: &Name, alias: Option<&Name>, diag: &mut Diagnostics) {
    let src = format!("{}/{}", src, sub);
    let aliased = alias.unwrap_or(sub);
    self.add_bind(&src, aliased.clone(), diag);
  }

  fn add_binds(&mut self, names: &IndexSet<Name>, src: &Name, diag: &mut Diagnostics) {
    for sub in names {
      self.add_aliased_bind(src, sub, None, diag);
    }
  }

  fn add_aliased_binds(&mut self, names: &[(Name, Option<Name>)], src: &Name, diag: &mut Diagnostics) {
    for (sub, alias) in names {
      self.add_aliased_bind(src, sub, alias.as_ref(), diag);
    }
  }

  /// Adds all names to the ImportMap in the form `alias/name`.
  /// If one of the names is equal to the non-aliased name, adds as `alias` instead.
  fn add_nested_binds(
    &mut self,
    src: &Name,
    nam: &Name,
    alias: Option<&Name>,
    names: IndexSet<Name>,
    diag: &mut Diagnostics,
  ) {
    let aliased = alias.unwrap_or(nam);

    for name in &names {
      if name != nam {
        let src = format!("{}/{}", src, name);
        let bind = Name::new(format!("{aliased}/{name}"));
        self.add_bind(&src, bind, diag);
      }
    }

    if names.contains(nam) {
      let src = format!("{}/{}", src, nam);
      self.add_bind(&src, aliased.clone(), diag);
    }
  }
}
