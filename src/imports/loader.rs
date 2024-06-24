use super::{BoundSource, Import, ImportType};
use crate::fun::Name;
use indexmap::IndexMap;
use std::{
  collections::HashSet,
  path::{Path, PathBuf},
};

pub type Sources = IndexMap<Name, String>;

/// Trait to load packages from various sources.
pub trait PackageLoader {
  /// Load a package specified by the `import` parameter.
  ///
  /// # Parameters
  ///
  /// - `import`: A mutable reference to an `Import` structure, which contains:
  ///   - `path`: The path to the package or directory to be imported.
  ///   - `imp_type`: The type of import, which can specify a single name, a list of names, or all names in a path.
  ///   - `relative`: A boolean indicating if the path is relative to the current directory.
  ///   - `src`: A `BoundSource` to be updated with the names of the located files.
  ///
  /// # Behavior
  ///
  /// The `load` method is responsible for locating and loading the requested package(s).
  /// The loaded packages are returned as a `Sources` map, where the key is the package name and the value is its content.
  /// Implementers must:
  ///
  /// - Track already loaded sources to avoid loading and returning them again.
  /// - Update `import.src` with the names of the found packages, even if they are not included in the `Sources` map.
  ///
  /// The implementation should handle the following import types:
  /// - **Single**: Load a specific file by its name.
  /// - **List**: Load a list of specified files or names from a specific file.
  /// - **Glob**: Load all files in a directory or all names from a specific file.
  fn load(&mut self, import: &mut Import) -> Result<Sources, String>;
}

/// Default implementation of `PackageLoader` that loads packages from the local directory.
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

    let file = if full_path.with_extension("bend").is_file() {
      self.read_file(&full_path, path.clone(), &mut src)
    } else {
      None
    };

    let dir = if full_path.is_dir() || path.is_empty() {
      let mut names = IndexMap::new();

      match imp_type {
        ImportType::Single(file, _) => {
          if let Some(name) = self.read_file_in_folder(&full_path, path, file, &mut src) {
            names.insert(file.clone(), name);
          }
        }
        ImportType::List(list) => {
          for (file, _) in list {
            if let Some(name) = self.read_file_in_folder(&full_path, path, file, &mut src) {
              names.insert(file.clone(), name);
            }
          }
        }
        ImportType::Glob => {
          for entry in full_path.read_dir().unwrap().flatten() {
            let file = PathBuf::from(&entry.file_name());

            if let Some("bend") = file.extension().and_then(|f| f.to_str()) {
              let file = file.file_stem().unwrap().to_string_lossy();
              if let Some(name) = self.read_file_in_folder(&full_path, path, &file, &mut src) {
                names.insert(Name::new(file), name);
              }
            }
          }
        }
      }

      if names.is_empty() {
        None
      } else {
        Some(names)
      }
    } else {
      None
    };

    match (file, dir) {
      (Some(f), None) => Some((BoundSource::File(f), src)),
      (None, Some(d)) => Some((BoundSource::Dir(d), src)),
      (Some(f), Some(d)) => Some((BoundSource::Either(f, d), src)),
      (None, None) => None,
    }
  }

  fn is_loaded(&self, name: &Name) -> bool {
    self.loaded.contains(name)
  }
}

pub const BEND_PATH: &[&str] = &[""];

impl PackageLoader for DefaultLoader {
  fn load(&mut self, import: &mut Import) -> Result<Sources, String> {
    let mut sources = Sources::new();

    let Import { path, imp_type, relative, src } = import;

    let folders = if *relative {
      vec![self.local_path.clone()]
    } else {
      BEND_PATH.iter().map(|p| self.local_path.join(p)).collect()
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
}
