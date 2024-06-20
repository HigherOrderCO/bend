use crate::{
  fun::{Book, Term},
  maybe_grow,
};

impl Book {
  /// Inline copies of the declared bind in the `use` expression.
  ///
  /// Example:
  /// ```bend
  /// use id = λx x
  /// (id id id)
  ///
  /// // Transforms to:
  /// (λx x λx x λx x)
  /// ```
  pub fn desugar_use(&mut self) {
    for def in self.defs.values_mut() {
      for rule in def.rules.iter_mut() {
        rule.body.desugar_use();
      }
    }
  }

  /// Inline copies of the declared bind in `Fold`, `Mat` and `Open` inside `use` expressions.
  pub fn desugar_ctr_use(&mut self) {
    for def in self.defs.values_mut() {
      for rule in def.rules.iter_mut() {
        rule.body.desugar_ctr_use();
      }
    }
  }
}

impl Term {
  pub fn desugar_use(&mut self) {
    maybe_grow(|| {
      for children in self.children_mut() {
        children.desugar_use();
      }
    });

    if let Term::Use { nam: Some(nam), val, nxt } = self {
      nxt.subst(nam, val);
      *self = std::mem::take(nxt);
    }
  }

  pub fn desugar_ctr_use(&mut self) {
    maybe_grow(|| {
      for children in self.children_mut() {
        children.desugar_ctr_use();
      }
    });

    if let Term::Use { nam: Some(nam), val, nxt } = self {
      if let Term::Var { nam: val } = val.as_ref() {
        nxt.subst_ctrs(nam, val);
      }
    }
  }
}
