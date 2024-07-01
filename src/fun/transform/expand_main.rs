use crate::{
  fun::{Book, Name, Pattern, Term},
  maybe_grow,
};
use std::collections::HashMap;

impl Book {
  /// Expands the main function so that it is not just a reference.
  /// While technically correct, directly returning a reference is never what users want.
  pub fn expand_main(&mut self) {
    if self.entrypoint.is_none() {
      return;
    }

    let main = self.defs.get_mut(self.entrypoint.as_ref().unwrap()).unwrap();
    let mut main_bod = std::mem::take(&mut main.rule_mut().body);

    let mut seen = vec![self.entrypoint.as_ref().unwrap().clone()];
    main_bod.expand_ref_return(self, &mut seen, &mut 0);

    let main = self.defs.get_mut(self.entrypoint.as_ref().unwrap()).unwrap();
    main.rule_mut().body = main_bod;
  }
}

impl Term {
  /// Expands references in the main function that are in "return" position.
  ///
  /// This applies to:
  /// - When main returns a reference.
  /// - When main returns a lambda whose body is a reference.
  /// - When main returns a pair or superposition and one of its elements is a reference.
  ///
  /// Only expand recursive functions once.
  pub fn expand_ref_return(&mut self, book: &Book, seen: &mut Vec<Name>, globals_count: &mut usize) {
    maybe_grow(|| match self {
      Term::Ref { nam } => {
        if seen.contains(nam) {
          // Don't expand recursive references
        } else {
          seen.push(nam.clone());
          let mut body = book.defs.get(nam).unwrap().rule().body.clone();
          body.rename_unscoped(globals_count, &mut HashMap::new());
          *self = body;
          self.expand_ref_return(book, seen, globals_count);
          seen.pop().unwrap();
        }
      }
      Term::Fan { els, .. } | Term::List { els } => {
        for el in els {
          el.expand_ref_return(book, seen, globals_count);
        }
      }
      Term::Lam { bod: nxt, .. }
      | Term::With { bod: nxt, .. }
      | Term::Open { bod: nxt, .. }
      | Term::Let { nxt, .. }
      | Term::Ask { nxt, .. }
      | Term::Use { nxt, .. } => nxt.expand_ref_return(book, seen, globals_count),
      Term::Var { .. }
      | Term::Link { .. }
      | Term::App { .. }
      | Term::Num { .. }
      | Term::Nat { .. }
      | Term::Str { .. }
      | Term::Oper { .. }
      | Term::Mat { .. }
      | Term::Swt { .. }
      | Term::Fold { .. }
      | Term::Bend { .. }
      | Term::Def { .. }
      | Term::Era
      | Term::Err => {}
    })
  }
}

impl Term {
  /// Since expanded functions can contain unscoped variables, and
  /// unscoped variable names must be unique, we need to rename them
  /// to avoid conflicts.
  fn rename_unscoped(&mut self, unscoped_count: &mut usize, unscoped_map: &mut HashMap<Name, Name>) {
    match self {
      Term::Let { pat, .. } | Term::Lam { pat, .. } => pat.rename_unscoped(unscoped_count, unscoped_map),
      Term::Link { nam } => rename_unscoped(nam, unscoped_count, unscoped_map),
      _ => {
        // Isn't an unscoped bind or use, do nothing, just recurse.
      }
    }
    for child in self.children_mut() {
      child.rename_unscoped(unscoped_count, unscoped_map);
    }
  }
}

impl Pattern {
  fn rename_unscoped(&mut self, unscoped_count: &mut usize, unscoped_map: &mut HashMap<Name, Name>) {
    maybe_grow(|| {
      match self {
        Pattern::Chn(nam) => rename_unscoped(nam, unscoped_count, unscoped_map),
        _ => {
          // Pattern isn't an unscoped bind, just recurse.
        }
      }
      for child in self.children_mut() {
        child.rename_unscoped(unscoped_count, unscoped_map);
      }
    })
  }
}

/// Generates a new name for an unscoped variable.
fn rename_unscoped(nam: &mut Name, unscoped_count: &mut usize, unscoped_map: &mut HashMap<Name, Name>) {
  if let Some(new_nam) = unscoped_map.get(nam) {
    *nam = new_nam.clone();
  } else {
    let new_nam = Name::new(format!("{nam}%{}", unscoped_count));
    unscoped_map.insert(nam.clone(), new_nam.clone());
    *unscoped_count += 1;
    *nam = new_nam;
  }
}
