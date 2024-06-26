use crate::{
  fun::{Book, Definition, Name, Rule, Term},
  maybe_grow,
};
use indexmap::{IndexMap, IndexSet};
use itertools::Itertools;
use std::collections::BTreeMap;

pub const MERGE_SEPARATOR: &str = "__M_";

impl Book {
  /// Merges definitions that have the same structure into one definition.
  /// Expects variables to be linear.
  ///
  /// Ignores origin of the rules when merging,
  /// Should not be preceded by passes that cares about the origins.
  pub fn merge_definitions(&mut self) {
    let defs: Vec<_> = self.defs.keys().cloned().collect();
    self.merge(defs.into_iter());
  }

  /// Checks and merges identical definitions given by `defs`.
  /// We never merge the entrypoint function with something else.
  fn merge(&mut self, defs: impl Iterator<Item = Name>) {
    let name = self.entrypoint.clone();
    // Sets of definitions that are identical, indexed by the body term.
    let equal_terms =
      self.collect_terms(defs.filter(|def_name| !name.as_ref().is_some_and(|m| m == def_name)));

    // Map of old name to new merged name
    let mut name_map = BTreeMap::new();

    for (term, equal_defs) in equal_terms {
      // def1_$_def2_$_def3
      let new_name = Name::new(equal_defs.iter().join(MERGE_SEPARATOR));

      // Builtin origin takes precedence
      let builtin = equal_defs.iter().any(|nam| self.defs[nam].is_builtin());

      if equal_defs.len() > 1 {
        // Merging some defs
        // Add the merged def
        let new_def = Definition::new_gen(new_name.clone(), vec![Rule { pats: vec![], body: term }], builtin);
        self.defs.insert(new_name.clone(), new_def);
        // Remove the old ones and write the map of old names to new ones.
        for name in equal_defs {
          self.defs.swap_remove(&name);
          name_map.insert(name, new_name.clone());
        }
      } else {
        // Not merging, just put the body back
        let def_name = equal_defs.into_iter().next().unwrap();
        let def = self.defs.get_mut(&def_name).unwrap();
        def.rule_mut().body = term;
      }
    }
    self.update_refs(&name_map);
  }

  fn collect_terms(&mut self, def_entries: impl Iterator<Item = Name>) -> IndexMap<Term, IndexSet<Name>> {
    let mut equal_terms: IndexMap<Term, IndexSet<Name>> = IndexMap::new();

    for def_name in def_entries {
      let def = self.defs.get_mut(&def_name).unwrap();
      let term = std::mem::take(&mut def.rule_mut().body);
      equal_terms.entry(term).or_default().insert(def_name);
    }

    equal_terms
  }

  fn update_refs(&mut self, name_map: &BTreeMap<Name, Name>) {
    let mut updated_defs = Vec::new();

    for def in self.defs.values_mut() {
      if Term::subst_ref_to_ref(&mut def.rule_mut().body, name_map) {
        updated_defs.push(def.name.clone());
      }
    }

    if !updated_defs.is_empty() {
      self.merge(updated_defs.into_iter());
    }
  }
}

impl Term {
  /// Performs reference substitution within a term replacing any references found in
  /// `ref_map` with their corresponding targets.
  pub fn subst_ref_to_ref(term: &mut Term, ref_map: &BTreeMap<Name, Name>) -> bool {
    maybe_grow(|| match term {
      Term::Ref { nam: def_name } => {
        if let Some(target_name) = ref_map.get(def_name) {
          *def_name = target_name.clone();
          true
        } else {
          false
        }
      }

      _ => {
        let mut subst = false;
        for child in term.children_mut() {
          subst |= Term::subst_ref_to_ref(child, ref_map);
        }
        subst
      }
    })
  }
}
