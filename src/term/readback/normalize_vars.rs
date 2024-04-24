use std::collections::{hash_map::Entry, HashMap};

use crate::term::{num_to_name, Book, Name, Pattern, Term};

pub fn normalize_vars(book: &Book, term: &mut Term) {
  NormalizeVarsState { vars: Default::default(), name_idx: 0, book }.visit_term(term)
}

struct NormalizeVarsState<'t, 'b> {
  vars: HashMap<Name, Var<'t>>,
  name_idx: u64,
  book: &'b Book,
}

enum Var<'t> {
  Bound(Term),
  Later(&'t mut Term),
}

impl<'t> NormalizeVarsState<'t, '_> {
  fn gen_name(&mut self) -> Name {
    let nam = Name::new(num_to_name(self.name_idx));
    self.name_idx += 1;
    nam
  }

  fn visit_term(&mut self, term: &'t mut Term) {
    match term {
      Term::Var { nam } => match self.vars.entry(nam.clone()) {
        Entry::Occupied(mut e) => {
          let Var::Bound(bnd) = e.get() else { unreachable!() };
          *term = bnd.clone();
          *e.get_mut() = Var::Later(term);
        }
        Entry::Vacant(e) => {
          e.insert(Var::Later(term));
        }
      },
      Term::Lam { pat, bod, .. } => {
        self.enter_pattern(pat);
        self.visit_term(bod);
        self.exit_pattern(pat);
      }
      Term::Let { pat, val, nxt } => {
        self.visit_term(val);
        self.enter_pattern(pat);
        self.visit_term(nxt);
        self.exit_pattern(pat);
      }
      Term::Swt { arg, bnd, pred, arms, .. } => {
        let new_bnd = if let Term::Var { nam } = &**arg
          && let Some(Var::Bound(var)) = self.vars.get(nam)
          && let Term::Var { nam } = var
        {
          Some(nam.clone())
        } else {
          None
        };
        self.visit_term(arg);
        let new_bnd = new_bnd.unwrap_or_else(|| self.gen_name());
        let [zero, succ] = &mut arms[..] else { unreachable!() };
        self.visit_term(zero);
        let new_pred = Name::new(format!("{new_bnd}-1"));
        let mut pat = self.enter_var_binding(pred.clone().unwrap(), new_pred);
        self.visit_term(succ);
        self.exit_pattern(&mut pat);
        *pred = None;
        *bnd = Some(new_bnd);
      }
      Term::Ref { nam } => {
        if nam.is_generated() {
          *term = self.book.defs[&*nam].rules[0].body.clone();
          self.visit_term(term);
        }
      }
      _ => {
        for child in term.children_mut() {
          self.visit_term(child);
        }
      }
    }
  }

  fn enter_pattern(&mut self, pat: &mut Pattern) {
    match pat {
      Pattern::Var(Some(old_name)) => {
        let new_name = self.gen_name();
        *pat = self.enter_var_binding(old_name.clone(), new_name);
      }
      _ => {
        for child in pat.children_mut() {
          self.enter_pattern(child)
        }
      }
    }
  }

  fn enter_var_binding(&mut self, old_name: Name, new_name: Name) -> Pattern {
    match self.vars.entry(old_name.clone()) {
      Entry::Occupied(e) => {
        let Var::Later(hole) = e.remove() else { unreachable!() };
        *hole = Term::Lnk { nam: new_name.clone() };
        Pattern::Chn(new_name)
      }
      Entry::Vacant(e) => {
        e.insert(Var::Bound(Term::Var { nam: new_name.clone() }));
        Pattern::Var(Some(old_name))
      }
    }
  }

  fn exit_pattern(&mut self, pat: &mut Pattern) {
    match pat {
      Pattern::Var(Some(name)) => {
        let Entry::Occupied(mut e) = self.vars.entry(name.clone()) else { unreachable!() };
        let (Var::Bound(Term::Var { nam }) | Var::Later(Term::Var { nam })) = e.get() else { unreachable!() };
        let new_name = nam.clone();
        match e.get_mut() {
          Var::Bound(t) => {
            *t = Term::Lnk { nam: new_name.clone() };
            *pat = Pattern::Chn(new_name);
          }
          Var::Later(_) => {
            *pat = Pattern::Var(Some(new_name));
          }
        }
      }
      _ => {
        for child in pat.children_mut() {
          self.exit_pattern(child)
        }
      }
    }
  }
}
