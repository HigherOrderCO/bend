use crate::{
  fun::{Book, Definition, Name, Pattern, Rule, Term},
  maybe_grow, multi_iterator,
};
use std::collections::{BTreeMap, HashSet};

impl Book {
  /// Extracts combinator terms into new definitions.
  ///
  /// Precondition: Variables must have been sanitized.
  ///
  /// The floating algorithm follows these rules:
  /// For each child of the term:
  /// - Recursively float every grandchild term.
  /// - If the child is a combinator:
  ///   * If the child is not "safe", extract it.
  ///   * If the term is a combinator and it's "safe":
  ///     - If the term is currently larger than `max_size`, extract the child.
  ///   * Otherwise, always extract the child to a new definition.
  /// - If the child is not a combinator, we can't extract it since
  ///   it would generate an invalid term.
  ///
  /// Terms are considered combinators if they have no free vars,
  /// no unmatched unscoped binds/vars and are not references (to
  /// avoid infinite recursion).
  ///
  /// See [`Term::is_safe`] for what is considered safe here.
  ///
  /// See [`Term::size`] for the measurement of size.
  /// It should more or less correspond to the compiled inet size.
  pub fn float_combinators(&mut self, max_size: usize) {
    let book = self.clone();
    let mut ctx = FloatCombinatorsCtx::new(&book, max_size);

    for (def_name, def) in self.defs.iter_mut() {
      // Don't float combinators in the main entrypoint.
      // This avoids making programs unexpectedly too lazy,
      // returning just a reference without executing anything.
      if let Some(main) = self.entrypoint.as_ref() {
        if def_name == main {
          continue;
        }
      }

      let builtin = def.builtin;
      let body = &mut def.rule_mut().body;
      ctx.reset();
      ctx.def_size = body.size();
      body.float_combinators(&mut ctx, def_name, builtin);
    }

    self.defs.extend(ctx.combinators.into_iter().map(|(nam, (_, def))| (nam, def)));
  }
}

struct FloatCombinatorsCtx<'b> {
  pub combinators: BTreeMap<Name, (bool, Definition)>,
  pub name_gen: usize,
  pub seen: HashSet<Name>,
  pub book: &'b Book,
  pub max_size: usize,
  pub def_size: usize,
}

impl<'b> FloatCombinatorsCtx<'b> {
  fn new(book: &'b Book, max_size: usize) -> Self {
    Self {
      combinators: Default::default(),
      name_gen: 0,
      seen: Default::default(),
      book,
      max_size,
      def_size: 0,
    }
  }

  fn reset(&mut self) {
    self.def_size = 0;
    self.name_gen = 0;
    self.seen = Default::default();
  }
}

impl Term {
  fn float_combinators(&mut self, ctx: &mut FloatCombinatorsCtx, def_name: &Name, builtin: bool) {
    maybe_grow(|| {
      // Recursively float the grandchildren terms.
      for child in self.float_children_mut() {
        child.float_combinators(ctx, def_name, builtin);
      }

      let mut size = self.size();
      let is_combinator = self.is_combinator();

      // Float unsafe children and children that make the term too big.
      for child in self.float_children_mut() {
        let child_is_safe = child.is_safe(ctx);
        let child_size = child.size();

        let extract_for_size = if is_combinator { size > ctx.max_size } else { ctx.def_size > ctx.max_size };

        if child.is_combinator() && child_size > 0 && (!child_is_safe || extract_for_size) {
          ctx.def_size -= child_size;
          size -= child_size;
          child.float(ctx, def_name, builtin, child_is_safe);
        }
      }
    })
  }

  /// Inserts a new definition for the given term in the combinators map.
  fn float(&mut self, ctx: &mut FloatCombinatorsCtx, def_name: &Name, builtin: bool, is_safe: bool) {
    let comb_name = Name::new(format!("{}__C{}", def_name, ctx.name_gen));
    ctx.name_gen += 1;

    let comb_ref = Term::Ref { nam: comb_name.clone() };
    let extracted_term = std::mem::replace(self, comb_ref);

    let rules = vec![Rule { body: extracted_term, pats: Vec::new() }];
    let rule = Definition { name: comb_name.clone(), rules, builtin };
    ctx.combinators.insert(comb_name, (is_safe, rule));
  }
}

impl Term {
  /// A term can be considered safe if it is:
  /// - A Number or an Eraser.
  /// - A Tuple or Superposition where all elements are safe.
  /// - An application or numeric operation where all arguments are safe.
  /// - A safe Lambda, e.g. a nullary constructor or a lambda with safe body.
  /// - A Reference with a safe body.
  /// A reference to a recursive definition (or mutually recursive) is not safe.
  fn is_safe(&self, ctx: &mut FloatCombinatorsCtx) -> bool {
    maybe_grow(|| match self {
      Term::Num { .. }
      | Term::Era
      | Term::Err
      | Term::Fan { .. }
      | Term::App { .. }
      | Term::Oper { .. }
      | Term::Swt { .. } => self.children().all(|c| c.is_safe(ctx)),
      Term::Lam { .. } => self.is_safe_lambda(ctx),
      Term::Ref { nam } => {
        // Constructors are safe
        if ctx.book.ctrs.contains_key(nam) {
          return true;
        }
        // If recursive, not safe
        if ctx.seen.contains(nam) {
          return false;
        }
        ctx.seen.insert(nam.clone());

        // Check if the function it's referring to is safe
        let safe = if let Some(def) = ctx.book.defs.get(nam) {
          let ref_safe = def.rule().body.is_safe(ctx);
          ref_safe
        } else if let Some((safe, _)) = ctx.combinators.get(nam) {
          *safe
        } else {
          false
        };

        ctx.seen.remove(nam);
        safe
      }
      // TODO: Variables can be safe depending on how they're used
      // For example, in a well-typed numop they're safe.
      _ => false,
    })
  }

  /// Checks if the term is a lambda sequence with the body being a variable in the scope or a reference.
  fn is_safe_lambda(&self, ctx: &mut FloatCombinatorsCtx) -> bool {
    let mut current = self;
    let mut scope = Vec::new();

    while let Term::Lam { bod, .. } = current {
      scope.extend(current.pattern().unwrap().binds().filter_map(|x| x.as_ref()));
      current = bod;
    }

    match current {
      Term::Var { nam } if scope.contains(&nam) => true,
      Term::Ref { .. } => true,
      term => term.is_safe(ctx),
    }
  }

  pub fn has_unscoped_diff(&self) -> bool {
    let (declared, used) = self.unscoped_vars();
    declared.difference(&used).count() != 0 || used.difference(&declared).count() != 0
  }

  fn is_combinator(&self) -> bool {
    self.free_vars().is_empty() && !self.has_unscoped_diff() && !matches!(self, Term::Ref { .. })
  }

  fn base_size(&self) -> usize {
    match self {
      Term::Let { pat, .. } => pat.size(),
      Term::Fan { els, .. } => els.len() - 1,
      Term::Mat { arms, .. } => arms.len(),
      Term::Swt { arms, .. } => 2 * (arms.len() - 1),
      Term::Lam { .. } => 1,
      Term::App { .. } => 1,
      Term::Oper { .. } => 1,
      Term::Var { .. } => 0,
      Term::Link { .. } => 0,
      Term::Use { .. } => 0,
      Term::Num { .. } => 0,
      Term::Ref { .. } => 0,
      Term::Era => 0,
      Term::Bend { .. }
      | Term::Fold { .. }
      | Term::Nat { .. }
      | Term::Str { .. }
      | Term::List { .. }
      | Term::With { .. }
      | Term::Ask { .. }
      | Term::Open { .. }
      | Term::Def { .. }
      | Term::Err => unreachable!(),
    }
  }

  fn size(&self) -> usize {
    maybe_grow(|| {
      let children_size: usize = self.children().map(|c| c.size()).sum();
      self.base_size() + children_size
    })
  }

  pub fn float_children_mut(&mut self) -> impl Iterator<Item = &mut Term> {
    multi_iterator!(FloatIter { Zero, Two, Vec, Mat, App, Swt });
    match self {
      Term::App { .. } => {
        let mut next = Some(self);
        FloatIter::App(std::iter::from_fn(move || {
          let cur = next.take();
          if let Some(Term::App { fun, arg, .. }) = cur {
            next = Some(&mut *fun);
            Some(&mut **arg)
          } else {
            cur
          }
        }))
      }
      Term::Mat { arg, bnd: _, with_bnd: _, with_arg, arms } => FloatIter::Mat(
        [arg.as_mut()].into_iter().chain(with_arg.iter_mut()).chain(arms.iter_mut().map(|r| &mut r.2)),
      ),
      Term::Swt { arg, bnd: _, with_bnd: _, with_arg, pred: _, arms } => {
        FloatIter::Swt([arg.as_mut()].into_iter().chain(with_arg.iter_mut()).chain(arms.iter_mut()))
      }
      Term::Fan { els, .. } | Term::List { els } => FloatIter::Vec(els),
      Term::Let { val: fst, nxt: snd, .. }
      | Term::Use { val: fst, nxt: snd, .. }
      | Term::Oper { fst, snd, .. } => FloatIter::Two([fst.as_mut(), snd.as_mut()]),
      Term::Lam { bod, .. } => bod.float_children_mut(),
      Term::Var { .. }
      | Term::Link { .. }
      | Term::Num { .. }
      | Term::Nat { .. }
      | Term::Str { .. }
      | Term::Ref { .. }
      | Term::Era
      | Term::Err => FloatIter::Zero([]),
      Term::With { .. }
      | Term::Ask { .. }
      | Term::Bend { .. }
      | Term::Fold { .. }
      | Term::Open { .. }
      | Term::Def { .. } => {
        unreachable!()
      }
    }
  }
}

impl Pattern {
  fn size(&self) -> usize {
    match self {
      Pattern::Var(_) => 0,
      Pattern::Chn(_) => 0,
      Pattern::Fan(_, _, pats) => pats.len() - 1 + pats.iter().map(|p| p.size()).sum::<usize>(),

      Pattern::Num(_) | Pattern::Lst(_) | Pattern::Str(_) | Pattern::Ctr(_, _) => unreachable!(),
    }
  }
}
