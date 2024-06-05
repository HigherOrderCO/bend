use crate::{
  diagnostics::WarningType,
  fun::{Book, Ctx, Name, Term},
  maybe_grow,
};
use hvm::ast::{Net, Tree};
use std::collections::{hash_map::Entry, HashMap};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Used {
  /// Definition is accessible from the main entry point, should never be pruned.
  Main,
  /// Definition is not accessible from main, but is accessible from non-builtin definitions.
  NonBuiltin,
  /// Definition is not accessible from main, but is a user-defined constructor.
  Ctr,
}

type Definitions = HashMap<Name, Used>;

impl Ctx<'_> {
  /// If `prune_all`, removes all unused definitions and adts starting from Main.
  /// Otherwise, prunes only the builtins not accessible from any non-built-in definition
  pub fn prune(&mut self, prune_all: bool) {
    let mut used = Definitions::new();

    // Get the functions that are accessible from the main entry point.
    if let Some(main) = &self.book.entrypoint {
      let def = self.book.defs.get(main).unwrap();
      used.insert(main.clone(), Used::Main);
      self.book.find_used_definitions_from_term(&def.rule().body, Used::Main, &mut used);
    }

    // Get the functions that are accessible from non-builtins.
    for def in self.book.defs.values() {
      if !def.is_builtin() && !(used.get(&def.name) == Some(&Used::Main)) {
        if self.book.ctrs.contains_key(&def.name) {
          used.insert(def.name.clone(), Used::Ctr);
        } else {
          used.insert(def.name.clone(), Used::NonBuiltin);
        }
        self.book.find_used_definitions_from_term(&def.rule().body, Used::NonBuiltin, &mut used);
      }
    }
    for def in self.book.hvm_defs.values() {
      if !def.builtin && !(used.get(&def.name) == Some(&Used::Main)) {
        used.insert(def.name.clone(), Used::NonBuiltin);
        self.book.find_used_definitions_from_hvm_net(&def.body, Used::NonBuiltin, &mut used);
      }
    }

    fn rm_def(book: &mut Book, def_name: &Name) {
      if book.defs.contains_key(def_name) {
        book.defs.shift_remove(def_name);
      } else if book.hvm_defs.contains_key(def_name) {
        book.hvm_defs.shift_remove(def_name);
      } else {
        unreachable!()
      }
    }

    // Remove unused definitions.
    let names = self.book.defs.keys().cloned().chain(self.book.hvm_defs.keys().cloned()).collect::<Vec<_>>();
    for def in names {
      if let Some(use_) = used.get(&def) {
        match use_ {
          Used::Main => {
            // Used by the main entry point, never pruned;
          }
          Used::NonBuiltin => {
            // Used by a non-builtin definition.
            // Prune if `prune_all`, otherwise show a warning.
            if prune_all {
              rm_def(self.book, &def);
            } else if !def.is_generated() {
              self.info.add_rule_warning("Definition is unused.", WarningType::UnusedDefinition, def);
            }
          }
          Used::Ctr => {
            // Unused, but a user-defined constructor.
            // Prune if `prune_all`, otherwise nothing.
            if prune_all {
              rm_def(self.book, &def);
            } else {
              // Don't show warning if it's a user-defined constructor.
            }
          }
        }
      } else {
        // Unused builtin, can always be pruned.
        rm_def(self.book, &def);
      }
    }
  }
}

impl Book {
  /// Finds all used definitions on the book, starting from the given term.
  fn find_used_definitions_from_term(&self, term: &Term, used: Used, uses: &mut Definitions) {
    maybe_grow(|| {
      let mut to_find = vec![term];

      while let Some(term) = to_find.pop() {
        match term {
          Term::Ref { nam: def_name } => self.insert_used(def_name, used, uses),
          Term::List { .. } => {
            self.insert_used(&Name::new(crate::fun::builtins::LCONS), used, uses);
            self.insert_used(&Name::new(crate::fun::builtins::LNIL), used, uses);
          }
          Term::Str { .. } => {
            self.insert_used(&Name::new(crate::fun::builtins::SCONS), used, uses);
            self.insert_used(&Name::new(crate::fun::builtins::SNIL), used, uses);
          }
          _ => {}
        }

        for child in term.children() {
          to_find.push(child);
        }
      }
    })
  }

  fn find_used_definitions_from_hvm_net(&self, net: &Net, used: Used, uses: &mut Definitions) {
    maybe_grow(|| {
      let mut to_find = [&net.root]
        .into_iter()
        .chain(net.rbag.iter().flat_map(|(_, lft, rgt)| [lft, rgt]))
        .collect::<Vec<_>>();

      while let Some(term) = to_find.pop() {
        match term {
          Tree::Ref { nam } => self.insert_used(&Name::new(nam), used, uses),
          Tree::Con { fst, snd }
          | Tree::Dup { fst, snd }
          | Tree::Opr { fst, snd }
          | Tree::Swi { fst, snd } => {
            to_find.push(fst);
            to_find.push(snd);
          }
          Tree::Era | Tree::Var { .. } | Tree::Num { .. } => {}
        }
      }
    })
  }

  fn insert_used(&self, def_name: &Name, used: Used, uses: &mut Definitions) {
    if let Entry::Vacant(e) = uses.entry(def_name.clone()) {
      e.insert(used);

      // This needs to be done for each rule in case the pass it's
      // ran from has not encoded the pattern match.
      // E.g.: the `flatten_rules` golden test
      if let Some(def) = self.defs.get(def_name) {
        for rule in &def.rules {
          self.find_used_definitions_from_term(&rule.body, used, uses);
        }
      } else if let Some(def) = self.hvm_defs.get(def_name) {
        self.find_used_definitions_from_hvm_net(&def.body, used, uses);
      } else {
        unreachable!()
      }
    }
  }
}
