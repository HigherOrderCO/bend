// Pass to give all variables in a definition unique names.

use crate::{
  fun::{Book, Name, Term},
  maybe_grow,
};
use std::collections::HashMap;

impl Book {
  /// Makes all variables in each definition have a new unique name.
  /// Skips unbound variables.
  /// Precondition: Definition references have been resolved.
  pub fn make_var_names_unique(&mut self) {
    for def in self.defs.values_mut() {
      def.rule_mut().body.make_var_names_unique();
    }
  }
}

impl Term {
  pub fn make_var_names_unique(&mut self) {
    UniqueNameGenerator::default().unique_names_in_term(self);
  }
}

type VarId = u64;

#[derive(Default)]
pub struct UniqueNameGenerator {
  name_map: HashMap<Name, Vec<VarId>>,
  name_count: VarId,
}

impl UniqueNameGenerator {
  // Recursively assign an id to each variable in the term, then convert each id into a unique name.
  pub fn unique_names_in_term(&mut self, term: &mut Term) {
    // Note: we can't use the children iterators here because we mutate the binds,
    // which are shared across multiple children.
    maybe_grow(|| match term {
      Term::Var { nam } => *nam = self.use_var(nam),

      Term::Mat { bnd, arg, with_bnd, with_arg, arms }
      | Term::Fold { bnd, arg, with_bnd, with_arg, arms } => {
        // Process args
        self.unique_names_in_term(arg);
        for arg in with_arg {
          self.unique_names_in_term(arg);
        }

        // Add binds shared by all arms
        self.push(bnd.as_ref());
        for bnd in with_bnd.iter() {
          self.push(bnd.as_ref());
        }

        // Process arms
        for arm in arms {
          // Add binds unique to each arm
          for bnd in arm.1.iter() {
            self.push(bnd.as_ref());
          }

          // Process arm body
          self.unique_names_in_term(&mut arm.2);

          // Remove binds unique to each arm
          for bnd in arm.1.iter_mut() {
            *bnd = self.pop(bnd.as_ref());
          }
        }

        // Remove binds shared by all arms
        for bnd in with_bnd {
          *bnd = self.pop(bnd.as_ref());
        }
        *bnd = self.pop(bnd.as_ref());
      }

      Term::Swt { bnd, arg, with_bnd, with_arg, pred, arms } => {
        self.unique_names_in_term(arg);
        for arg in with_arg {
          self.unique_names_in_term(arg);
        }

        self.push(bnd.as_ref());
        for bnd in with_bnd.iter() {
          self.push(bnd.as_ref());
        }

        let (succ, nums) = arms.split_last_mut().unwrap();
        for arm in nums.iter_mut() {
          self.unique_names_in_term(arm);
        }

        self.push(pred.as_ref());
        self.unique_names_in_term(succ);
        *pred = self.pop(pred.as_ref());

        for bnd in with_bnd {
          *bnd = self.pop(bnd.as_ref());
        }
        *bnd = self.pop(bnd.as_ref());
      }

      Term::Bend { bnd, arg, cond, step, base } => {
        for arg in arg {
          self.unique_names_in_term(arg);
        }
        for bnd in bnd.iter() {
          self.push(bnd.as_ref());
        }
        self.unique_names_in_term(cond);
        self.unique_names_in_term(step);
        self.unique_names_in_term(base);
        for bnd in bnd {
          *bnd = self.pop(bnd.as_ref());
        }
      }

      Term::Let { pat, val, nxt } | Term::Ask { pat, val, nxt } => {
        self.unique_names_in_term(val);
        for bnd in pat.binds() {
          self.push(bnd.as_ref());
        }
        self.unique_names_in_term(nxt);
        for bind in pat.binds_mut() {
          *bind = self.pop(bind.as_ref());
        }
      }
      Term::Use { nam, val, nxt } => {
        self.unique_names_in_term(val);
        self.push(nam.as_ref());
        self.unique_names_in_term(nxt);
        *nam = self.pop(nam.as_ref());
      }
      Term::Lam { tag: _, pat, bod } => {
        for bind in pat.binds() {
          self.push(bind.as_ref());
        }
        self.unique_names_in_term(bod);
        for bind in pat.binds_mut() {
          *bind = self.pop(bind.as_ref());
        }
      }
      Term::Fan { fan: _, tag: _, els } | Term::List { els } => {
        for el in els {
          self.unique_names_in_term(el);
        }
      }
      Term::App { tag: _, fun: fst, arg: snd } | Term::Oper { opr: _, fst, snd } => {
        self.unique_names_in_term(fst);
        self.unique_names_in_term(snd);
      }
      Term::With { typ: _, bod } => {
        self.unique_names_in_term(bod);
      }
      Term::Link { .. }
      | Term::Num { .. }
      | Term::Nat { .. }
      | Term::Str { .. }
      | Term::Ref { .. }
      | Term::Era
      | Term::Err => {}
      Term::Open { .. } => unreachable!("'open' should be removed in earlier pass"),
      Term::Def { .. } => unreachable!("'def' should be removed in earlier pass"),
    })
  }

  fn push(&mut self, nam: Option<&Name>) {
    if let Some(name) = nam {
      if let Some(ids) = self.name_map.get_mut(name) {
        ids.push(self.name_count);
      } else {
        self.name_map.insert(name.clone(), vec![self.name_count]);
      }
      self.name_count += 1;
    }
  }

  fn pop(&mut self, nam: Option<&Name>) -> Option<Name> {
    if let Some(name) = nam {
      let var_id = self.name_map.get_mut(name).unwrap().pop().unwrap();
      if self.name_map[name].is_empty() {
        self.name_map.remove(name);
      }
      Some(Name::from(var_id))
    } else {
      None
    }
  }

  fn use_var(&self, nam: &Name) -> Name {
    if let Some(vars) = self.name_map.get(nam) {
      let var_id = *vars.last().unwrap();
      Name::from(var_id)
    } else {
      // Skip unbound variables.
      // With this, we can use this function before checking for unbound vars.
      nam.clone()
    }
  }
}
