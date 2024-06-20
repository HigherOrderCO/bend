use super::{ImportsMap, PackageLoader};
use crate::{
  diagnostics::{Diagnostics, DiagnosticsConfig},
  fun::{parser::ParseBook, Adt, Book, Definition, HvmDefinition, Name, Rule, Source, Term},
  imp::{Expr, Stmt},
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

      for def in defs.into_values() {
        self.add_imported_def(def, diag);
      }

      // TODO: Need to add hvm_defs too
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

    let mut local_imports: IndexMap<Name, Name> = IndexMap::new();

    // Collect local imports binds, starting with `__` if not imported by the main book.
    'outer: for (bind, src) in self.import_ctx.map.binds.iter().rev() {
      if self.contains_def(bind) | self.ctrs.contains_key(bind) | self.adts.contains_key(bind) {
        // TODO: Here we should show warnings for shadowing of imported names by local def/ctr/adt
        // It can be done, but when importing with `ImportType::Simple` files in the same folder,
        // it gives a false positive warning
        continue;
      }

      let nam = if main_imports.contains_source(src) { src.clone() } else { Name::new(format!("__{}", src)) };

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

  /// Applying the necessary naming transformations to the book ADTs,
  /// adding `use ctr = ctr_src` chains to every local definition.
  fn apply_adts(&mut self, src: &Name, main_imp: &ImportsMap) {
    let adts = std::mem::take(&mut self.adts);
    let mut new_adts = IndexMap::new();
    let mut ctrs_map = IndexMap::new();

    for (mut name, mut adt) in adts {
      match adt.source {
        Source::Local(..) => {
          adt.source = Source::Imported;
          name = Name::new(format!("{}/{}", src, name));

          let mangle_name = !main_imp.contains_source(&name);
          let mut mangle_adt_name = mangle_name;

          for (ctr, f) in std::mem::take(&mut adt.ctrs) {
            let mut ctr_name = Name::new(format!("{}/{}", src, ctr));

            let mangle_ctr = mangle_name && !main_imp.contains_source(&ctr_name);

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

    self.adts = new_adts;
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
}

/// Helper functions
impl ParseBook {
  pub fn top_level_names(&self) -> impl Iterator<Item = &Name> {
    let imp_defs = self.imp_defs.keys();
    let fun_defs = self.fun_defs.keys();
    let adts = self.adts.keys();
    let ctrs = self.ctrs.keys();

    imp_defs.chain(fun_defs).chain(adts).chain(ctrs)
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

      if !main_imp.contains_source(&new_name) {
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
