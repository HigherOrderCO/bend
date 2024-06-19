use super::{BoundSource, Import, ImportType};
use crate::fun::Name;
use indexmap::IndexMap;
use std::{
  collections::HashSet,
  path::{Path, PathBuf},
};

pub const BEND_PATH: &[&str] = &[""];

pub type Sources = IndexMap<Name, String>;

pub trait PackageLoader {
  fn load(&mut self, import: &mut Import) -> Result<Sources, String>;
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

  fn is_loaded(&self, name: &Name) -> bool {
    self.loaded.contains(name)
  }
}

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
