use std::collections::BTreeSet;

use indexmap::IndexMap;

use crate::fun::{Book, Definition, Name, Pattern, Rule, Term};

impl Book {
  pub fn lift_defs(&mut self) {
    let mut defs = IndexMap::new();
    for (name, def) in self.defs.iter_mut() {
      let mut gen = 0;
      for rule in def.rules.iter_mut() {
        rule.body.lift_defs(name, &mut defs, &mut gen);
      }
    }
    self.defs.extend(defs);
  }
}

impl Rule {
  pub fn binds(&self) -> impl DoubleEndedIterator<Item = &Option<Name>> + Clone {
    self.pats.iter().flat_map(Pattern::binds)
  }
}

impl Term {
  pub fn lift_defs(&mut self, parent: &Name, defs: &mut IndexMap<Name, Definition>, gen: &mut usize) {
    match self {
      Term::Def { nam, rules, nxt } => {
        let local_name = Name::new(format!("{}__local_{}_{}", parent, gen, nam));
        for rule in rules.iter_mut() {
          rule.body.lift_defs(&local_name, defs, gen);
        }
        nxt.lift_defs(parent, defs, gen);
        *gen += 1;

        let inner_defs =
          defs.keys().filter(|name| name.starts_with(local_name.as_ref())).cloned().collect::<BTreeSet<_>>();
        let (r#use, fvs, mut rules) = gen_use(inner_defs, &local_name, nam, nxt, std::mem::take(rules));
        *self = r#use;

        apply_closure(&mut rules, &fvs);

        let new_def = Definition { name: local_name.clone(), rules, builtin: false };
        defs.insert(local_name.clone(), new_def);
      }
      _ => {
        for child in self.children_mut() {
          child.lift_defs(parent, defs, gen);
        }
      }
    }
  }
}

fn gen_use(
  inner_defs: BTreeSet<Name>,
  local_name: &Name,
  nam: &Name,
  nxt: &mut Box<Term>,
  mut rules: Vec<Rule>,
) -> (Term, BTreeSet<Name>, Vec<Rule>) {
  let mut fvs = BTreeSet::<Name>::new();
  for rule in rules.iter() {
    fvs.extend(rule.body.free_vars().into_keys().collect::<BTreeSet<_>>());
  }
  fvs.retain(|fv| !inner_defs.contains(fv));
  for rule in rules.iter() {
    for bind in rule.binds().flatten() {
      fvs.remove(bind);
    }
  }
  fvs.remove(nam);

  let call = Term::call(
    Term::Ref { nam: local_name.clone() },
    fvs.iter().cloned().map(|nam| Term::Var { nam }).collect::<Vec<_>>(),
  );

  let r#use = Term::Use { nam: Some(nam.clone()), val: Box::new(call.clone()), nxt: std::mem::take(nxt) };

  for rule in rules.iter_mut() {
    let slf = std::mem::take(&mut rule.body);
    rule.body = Term::Use { nam: Some(nam.clone()), val: Box::new(call.clone()), nxt: Box::new(slf) };
  }

  (r#use, fvs, rules)
}

fn apply_closure(rules: &mut [Rule], fvs: &BTreeSet<Name>) {
  for rule in rules.iter_mut() {
    let pats = std::mem::take(&mut rule.pats);
    let mut captured = fvs.iter().cloned().map(|nam| Pattern::Var(Some(nam))).collect::<Vec<_>>();
    captured.extend(pats);
    rule.pats = captured;
  }
}
