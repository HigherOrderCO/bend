use crate::{
  diagnostics::Diagnostics,
  fun::{Ctx, Definition, Name, Rule, Source, Term},
  maybe_grow,
};

pub const RECURSIVE_KW: &str = "fork";
const NEW_FN_SEP: &str = "__bend";

impl Ctx<'_> {
  pub fn desugar_bend(&mut self) -> Result<(), Diagnostics> {
    self.info.start_pass();
    let mut new_defs = vec![];
    for def in self.book.defs.values_mut() {
      let mut fresh = 0;
      for rule in def.rules.iter_mut() {
        if let Err(err) = rule.body.desugar_bend(&def.name, &mut fresh, &mut new_defs, &def.source) {
          self.info.add_rule_error(err, def.name.clone());
          break;
        }
      }
    }

    for def in new_defs {
      self.book.defs.insert(def.name.clone(), def);
    }

    self.info.fatal(())
  }
}

impl Term {
  fn desugar_bend(
    &mut self,
    def_name: &Name,
    fresh: &mut usize,
    new_defs: &mut Vec<Definition>,
    source: &Source,
  ) -> Result<(), String> {
    maybe_grow(|| {
      // Recursively encode bends in the children
      for child in self.children_mut() {
        child.desugar_bend(def_name, fresh, new_defs, source)?;
      }

      // Convert a bend into a new recursive function and call it.
      if let Term::Bend { .. } = self {
        // Can't have unmatched unscoped because this'll be extracted
        if self.has_unscoped_diff() {
          return Err("Can't have non self-contained unscoped variables in a 'bend'".into());
        }
        let Term::Bend { bnd, arg, cond, step, base } = self else { unreachable!() };

        let new_nam = Name::new(format!("{}{}{}", def_name, NEW_FN_SEP, fresh));
        *fresh += 1;

        // Gather the free variables
        // They will be implicitly captured by the new function
        let mut free_vars = step.free_vars();
        free_vars.remove(&Name::new(RECURSIVE_KW));
        free_vars.extend(base.free_vars());
        free_vars.extend(cond.free_vars());
        for bnd in bnd.iter().flatten() {
          free_vars.remove(bnd);
        }
        let free_vars = free_vars.into_keys().collect::<Vec<_>>();

        // Add a substitution of `fork`, a use term with a partially applied recursive call
        let step = Term::Use {
          nam: Some(Name::new(RECURSIVE_KW)),
          val: Box::new(Term::call(
            Term::Ref { nam: new_nam.clone() },
            free_vars.iter().cloned().map(|nam| Term::Var { nam }),
          )),
          nxt: Box::new(std::mem::take(step.as_mut())),
        };

        // Create the function body for the bend.
        let body = Term::Swt {
          arg: Box::new(std::mem::take(cond)),
          bnd: Some(Name::new("_")),
          with_bnd: vec![],
          with_arg: vec![],
          pred: Some(Name::new("_-1")),
          arms: vec![std::mem::take(base.as_mut()), step],
        };
        let body = Term::rfold_lams(body, std::mem::take(bnd).into_iter());
        let body = Term::rfold_lams(body, free_vars.iter().cloned().map(Some));

        // Make a definition from the new function
        let def = Definition::new(new_nam.clone(), vec![Rule { pats: vec![], body }], source.clone());
        new_defs.push(def);

        // Call the new function in the original term.
        let call =
          Term::call(Term::Ref { nam: new_nam }, free_vars.iter().map(|v| Term::Var { nam: v.clone() }));
        *self = Term::call(call, arg.drain(..));
      }

      Ok(())
    })
  }
}
