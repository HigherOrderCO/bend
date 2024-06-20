use super::{BindMap, ImportsMap, PackageLoader};
use crate::{
  diagnostics::{Diagnostics, DiagnosticsConfig},
  fun::{parser::ParseBook, Adt, Book, Definition, HvmDefinition, Name, Rule, Source, Term},
  imp::{self, Expr, Stmt},
  imports::packages::Packages,
};
use indexmap::{map::Entry, IndexMap};
use itertools::Itertools;

impl ParseBook {
  /// Loads and applies imports recursively to a ParseBook,
  /// transforming definitions and ADTs to a canonical name,
  /// and adding `use` binds so that names are accessible by their alias.
  ///
  /// # Details
  ///
  /// The process involves:
  ///
  /// 1. Loading imports recursively using the provided `loader`.
  /// 2. Transforming definitions and ADTs with naming transformations.
  /// 3. Adding binds for aliases and old names in their respective definitions.
  /// 4. Converting the ParseBook into its functional form.
  /// 5. Perform any necessary post-processing.
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

    // Process terms that contains constructors names and can't be updated by `desugar_use`.
    book.desugar_ctr_use();

    Ok(book)
  }

  /// Loads the imported books recursively into the importing book,
  /// then apply imported names or aliases binds to its definitions.
  fn apply_imports(
    &mut self,
    main_imports: Option<&ImportsMap>,
    diag: &mut Diagnostics,
    pkgs: &mut Packages,
  ) -> Result<(), Diagnostics> {
    self.load_packages(main_imports, diag, pkgs)?;
    self.apply_import_binds(main_imports, pkgs);
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

      // Rename ADTs and defs, applying binds from old names to new names
      package.apply_adts(&src, main_imports);
      package.apply_defs(&src, main_imports);

      package.apply_imports(Some(main_imports), diag, pkgs)?; // TODO: Should this be after the apply_adts/defs functions?

      let Book { defs, hvm_defs, adts, .. } = package.to_fun()?;

      // Add the ADTs to the importing book,
      // saving the constructors names to be used when applying ADTs binds.
      for (name, adt) in adts {
        let adts = pkgs.loaded_adts.entry(src.clone()).or_default();
        adts.insert(name.clone(), adt.ctrs.keys().cloned().collect_vec());
        self.add_imported_adt(name, adt, diag);
      }

      // The names on the indexmap are the original ones, so we ignore them
      for def in defs.into_values() {
        self.add_imported_def(def, diag);
      }

      // The names on the indexmap are the original ones, so we ignore them
      for def in hvm_defs.into_values() {
        self.add_imported_hvm_def(def, diag);
      }
    }

    diag.fatal(())
  }

  /// Applies a chain of `use bind = src` to every local definition.
  ///
  /// Must be used after `load_packages`
  fn apply_import_binds(&mut self, main_imports: Option<&ImportsMap>, pkgs: &Packages) {
    // Can not be done outside the function because of the borrow checker.
    // Just serves to pass only the import map of the first call to `apply_imports_go`.
    let main_imports = main_imports.unwrap_or(&self.import_ctx.map);

    let mut local_imports = BindMap::new();

    // Collect local imports binds, starting with `__` if not imported by the main book.
    'outer: for (bind, src) in self.import_ctx.map.binds.iter().rev() {
      if self.contains_def(bind) | self.ctrs.contains_key(bind) | self.adts.contains_key(bind) {
        // TODO: Here we should show warnings for shadowing of imported names by local def/ctr/adt
        // It can be done, but when importing with `ImportType::Simple` files in the same folder,
        // it gives a false positive warning
        continue;
      }

      let nam = if main_imports.contains_source(src) { src.clone() } else { Name::new(format!("__{}", src)) };

      // Checks if the bind is an loaded ADT name,
      // If so, add the constructors binds as `bind/ctr` instead.
      // As ADTs names are not used in the syntax, we don't bind their names.
      for pkg in self.import_ctx.sources() {
        if let Some(book) = pkgs.loaded_adts.get(pkg) {
          if let Some(ctrs) = book.get(&nam) {
            for ctr in ctrs.iter().rev() {
              let full_ctr_name = ctr.split("__").nth(1).unwrap_or(ctr.as_ref());
              let ctr_name = full_ctr_name.strip_prefix(src.as_ref()).unwrap();
              let bind = Name::new(format!("{}{}", bind, ctr_name));
              local_imports.insert(bind, ctr.clone());
            }
            continue 'outer;
          }
        }
      }

      // Not a constructor, so just insert the bind.
      local_imports.insert(bind.clone(), nam);
    }

    for (_, def) in self.local_defs_mut() {
      def.apply_binds(true, &local_imports);
    }
  }

  /// Applying the necessary naming transformations to the book ADTs,
  /// adding `use ctr = ctr_src` chains to every local definition.
  fn apply_adts(&mut self, src: &Name, main_imports: &ImportsMap) {
    let adts = std::mem::take(&mut self.adts);
    let mut new_adts = IndexMap::new();
    let mut ctrs_map = IndexMap::new();

    // Rename the ADTs and constructors to their canonical name,
    // starting with `__` if not imported by the main book.
    for (mut name, mut adt) in adts {
      if adt.source.is_local() {
        adt.source = Source::Imported;
        name = Name::new(format!("{}/{}", src, name));

        let mangle_name = !main_imports.contains_source(&name);
        let mut mangle_adt_name = mangle_name;

        for (ctr, f) in std::mem::take(&mut adt.ctrs) {
          let mut ctr_name = Name::new(format!("{}/{}", src, ctr));

          let mangle_ctr = mangle_name && !main_imports.contains_source(&ctr_name);

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

      new_adts.insert(name.clone(), adt);
    }

    // Applies the binds for the new constructor names for every definition.
    // As ADTs names are not used in the syntax, we don't bind their new names.
    for (_, def) in self.local_defs_mut() {
      def.apply_binds(true, &ctrs_map);
    }

    self.adts = new_adts;
  }

  /// Apply the necessary naming transformations to the book definitions,
  /// adding `use def = def_src` chains to every local definition.
  fn apply_defs(&mut self, src: &Name, main_imports: &ImportsMap) {
    let mut canonical_map: IndexMap<_, _> = IndexMap::new();

    // Rename the definitions to their canonical name
    // Starting with `__` if not imported by the main book.
    for (_, def) in self.local_defs_mut() {
      def.canonicalize_name(src, main_imports, &mut canonical_map);
    }

    // Applies the binds for the new names for every definition
    for (_, def) in self.local_defs_mut() {
      def.apply_binds(false, &canonical_map);
      *def.source_mut() = Source::Imported;
    }
  }
}

/// Helper functions
impl ParseBook {
  pub fn top_level_names(&self) -> impl Iterator<Item = &Name> {
    let imp_defs = self.imp_defs.keys();
    let fun_defs = self.fun_defs.keys();
    let hvm_defs = self.hvm_defs.keys();
    let adts = self.adts.keys();
    let ctrs = self.ctrs.keys();

    imp_defs.chain(fun_defs).chain(hvm_defs).chain(adts).chain(ctrs)
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
    if !self.has_def_conflict(&def.name, diag) {
      self.fun_defs.insert(def.name.clone(), def);
    }
  }

  fn add_imported_hvm_def(&mut self, def: HvmDefinition, diag: &mut Diagnostics) {
    if !self.has_def_conflict(&def.name, diag) {
      self.hvm_defs.insert(def.name.clone(), def);
    }
  }

  fn has_def_conflict(&mut self, name: &Name, diag: &mut Diagnostics) -> bool {
    if self.contains_def(name) {
      let err = format!("The imported definition '{name}' conflicts with the definition '{name}'.");
      diag.add_book_error(err);
      true
    } else if self.ctrs.contains_key(name) {
      let err = format!("The imported definition '{name}' conflicts with the constructor '{name}'.");
      diag.add_book_error(err);
      true
    } else {
      false
    }
  }

  fn local_defs_mut(&mut self) -> impl Iterator<Item = (&Name, &mut dyn Def)> {
    let fun = self.fun_defs.iter_mut().map(|(nam, def)| (nam, def as &mut dyn Def));
    let imp = self.imp_defs.iter_mut().map(|(nam, def)| (nam, def as &mut dyn Def));
    let hvm = self.hvm_defs.iter_mut().map(|(nam, def)| (nam, def as &mut dyn Def));
    fun.chain(imp).chain(hvm).filter(|(_, def)| def.source().is_local())
  }
}

/// Common functions for the different definition types
trait Def {
  fn canonicalize_name(&mut self, src: &Name, main_imports: &ImportsMap, binds: &mut BindMap) {
    let def_name = self.name_mut();
    let mut new_name = Name::new(format!("{}/{}", src, def_name));

    if !main_imports.contains_source(&new_name) {
      new_name = Name::new(format!("__{}", new_name));
    }

    binds.insert(def_name.clone(), new_name.clone());
    *def_name = new_name;
  }

  /// Should apply the binds to the definition,
  /// and if there are possible constructor names on it, rename rule patterns.
  fn apply_binds(&mut self, maybe_constructor: bool, binds: &BindMap);

  fn source(&self) -> &Source;
  fn source_mut(&mut self) -> &mut Source;
  fn name_mut(&mut self) -> &mut Name;
}

impl Def for Definition {
  fn apply_binds(&mut self, maybe_constructor: bool, binds: &BindMap) {
    fn rename_ctr_patterns(rule: &mut Rule, binds: &BindMap) {
      for pat in &mut rule.pats {
        for bind in pat.binds_mut().flatten() {
          if let Some(alias) = binds.get(bind) {
            *bind = alias.clone();
          }
        }
      }
    }

    for rule in &mut self.rules {
      if maybe_constructor {
        rename_ctr_patterns(rule, binds);
      }
      let bod = std::mem::take(&mut rule.body);
      rule.body = bod.fold_uses(binds.iter().rev());
    }
  }

  fn source(&self) -> &Source {
    &self.source
  }

  fn source_mut(&mut self) -> &mut Source {
    &mut self.source
  }

  fn name_mut(&mut self) -> &mut Name {
    &mut self.name
  }
}

impl Def for imp::Definition {
  fn apply_binds(&mut self, _maybe_constructor: bool, binds: &BindMap) {
    let bod = std::mem::take(&mut self.body);
    self.body = bod.fold_uses(binds.iter().rev());
  }

  fn source(&self) -> &Source {
    &self.source
  }

  fn source_mut(&mut self) -> &mut Source {
    &mut self.source
  }

  fn name_mut(&mut self) -> &mut Name {
    &mut self.name
  }
}

impl Def for HvmDefinition {
  /// Do nothing, can not apply binds to a HvmDefinition.
  fn apply_binds(&mut self, _maybe_constructor: bool, _binds: &BindMap) {}

  fn source(&self) -> &Source {
    &self.source
  }

  fn source_mut(&mut self) -> &mut Source {
    &mut self.source
  }

  fn name_mut(&mut self) -> &mut Name {
    &mut self.name
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
