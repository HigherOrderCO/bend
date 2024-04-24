use crate::{
  diagnostics::{Diagnostics, DiagnosticsConfig},
  maybe_grow,
  term::builtins::*,
  ENTRY_POINT,
};
use indexmap::{IndexMap, IndexSet};
use interner::global::{GlobalPool, GlobalString};
use itertools::Itertools;
use std::{borrow::Cow, collections::HashMap, ops::Deref};

pub mod builtins;
pub mod check;
pub mod display;
pub mod encoding;
pub mod load_book;
pub mod parser;
pub mod readback;
pub mod transform;

pub use encoding::{encode_book, encode_term};
pub use hvmc::ops::{IntOp, Op, Ty as OpType};
pub use readback::readback;

pub static STRINGS: GlobalPool<String> = GlobalPool::new();
#[derive(Debug)]
pub struct Ctx<'book> {
  pub book: &'book mut Book,
  pub info: Diagnostics,
}

impl Ctx<'_> {
  pub fn new(book: &mut Book, diagnostics_cfg: DiagnosticsConfig) -> Ctx {
    Ctx { book, info: Diagnostics::new(diagnostics_cfg) }
  }
}

/// The representation of a program.
#[derive(Debug, Clone, Default)]
pub struct Book {
  /// The function definitions.
  pub defs: IndexMap<Name, Definition>,

  /// The algebraic datatypes defined by the program
  pub adts: Adts,

  /// To which type does each constructor belong to.
  pub ctrs: Constructors,

  /// A custom or default "main" entrypoint.
  pub entrypoint: Option<Name>,
}

pub type Adts = IndexMap<Name, Adt>;
pub type Constructors = IndexMap<Name, Name>;

/// A pattern matching function definition.
#[derive(Debug, Clone)]
pub struct Definition {
  pub name: Name,
  pub rules: Vec<Rule>,
  pub builtin: bool,
}

/// A pattern matching rule of a definition.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Rule {
  pub pats: Vec<Pattern>,
  pub body: Term,
}

#[derive(Debug, Default, PartialEq, Eq, Hash)]
pub enum Term {
  Lam {
    tag: Tag,
    pat: Box<Pattern>,
    bod: Box<Term>,
  },
  Var {
    nam: Name,
  },
  Lnk {
    nam: Name,
  },
  Let {
    pat: Box<Pattern>,
    val: Box<Term>,
    nxt: Box<Term>,
  },
  Bnd {
    fun: Name,
    ask: Box<Pattern>,
    val: Box<Term>,
    nxt: Box<Term>,
  },
  Use {
    nam: Option<Name>,
    val: Box<Term>,
    nxt: Box<Term>,
  },
  App {
    tag: Tag,
    fun: Box<Term>,
    arg: Box<Term>,
  },
  /// Either a tuple or a superposition
  Fan {
    fan: FanKind,
    tag: Tag,
    els: Vec<Term>,
  },
  Num {
    val: u64,
  },
  Nat {
    val: u64,
  },
  Str {
    val: GlobalString,
  },
  Lst {
    els: Vec<Term>,
  },
  /// A numeric operation between built-in numbers.
  Opx {
    opr: Op,
    fst: Box<Term>,
    snd: Box<Term>,
  },
  /// Pattern matching on an ADT.
  Mat {
    arg: Box<Term>,
    bnd: Option<Name>,
    with: Vec<Name>,
    arms: Vec<MatchRule>,
  },
  /// Native pattern matching on numbers
  Swt {
    arg: Box<Term>,
    bnd: Option<Name>,
    with: Vec<Name>,
    pred: Option<Name>,
    arms: Vec<Term>,
  },
  Ref {
    nam: Name,
  },
  Era,
  #[default]
  Err,
}

pub type MatchRule = (Option<Name>, Vec<Option<Name>>, Term);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FanKind {
  Tup,
  Dup,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Pattern {
  Var(Option<Name>),
  Chn(Name),
  Ctr(Name, Vec<Pattern>),
  Num(u64),
  /// Either a tuple or a duplication
  Fan(FanKind, Tag, Vec<Pattern>),
  Lst(Vec<Pattern>),
  Str(GlobalString),
  #[default]
  Err,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum Tag {
  Named(Name),
  Numeric(u16),
  Auto,
  #[default]
  Static,
}

/// A user defined datatype
#[derive(Debug, Clone, Default)]
pub struct Adt {
  pub ctrs: IndexMap<Name, Vec<Name>>,
  pub builtin: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AdtEncoding {
  Scott,

  #[default]
  TaggedScott,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Name(GlobalString);

/* Implementations */

/// A macro for creating iterators that can have statically known
/// different types. Useful for iterating over tree children, where
/// each tree node variant yields a different iterator type.
#[macro_export]
macro_rules! multi_iterator {
  ($Iter:ident { $($Variant:ident),* $(,)? }) => {
    #[derive(Debug, Clone)]
    enum $Iter<$($Variant),*> {
      $($Variant { iter: $Variant }),*
    }

    impl<$($Variant),*> $Iter<$($Variant),*> {
      $(
        #[allow(non_snake_case)]
        fn $Variant(iter: impl IntoIterator<IntoIter = $Variant>) -> Self {
          $Iter::$Variant { iter: iter.into_iter() }
        }
      )*
    }

    impl<T, $($Variant: Iterator<Item = T>),*> Iterator for $Iter<$($Variant),*> {
      type Item = T;
      fn next(&mut self) -> Option<T> {
        match self { $($Iter::$Variant { iter } => iter.next()),* }
      }

      fn size_hint(&self) -> (usize, Option<usize>) {
        match self { $($Iter::$Variant { iter } => iter.size_hint()),* }
      }
    }

    impl<T, $($Variant: DoubleEndedIterator<Item = T>),*> DoubleEndedIterator for $Iter<$($Variant),*> {
      fn next_back(&mut self) -> Option<T> {
        match self { $($Iter::$Variant { iter } => iter.next_back()),* }
      }
    }
  };
}

impl PartialEq<str> for Name {
  fn eq(&self, other: &str) -> bool {
    &**self == other
  }
}

impl PartialEq<&str> for Name {
  fn eq(&self, other: &&str) -> bool {
    self == other
  }
}

impl PartialEq<Option<Name>> for Name {
  fn eq(&self, other: &Option<Name>) -> bool {
    if let Some(other) = other.as_ref() { self == other } else { false }
  }
}

impl PartialEq<Name> for Option<Name> {
  fn eq(&self, other: &Name) -> bool {
    other.eq(self)
  }
}

impl PartialEq<Option<&Name>> for Name {
  fn eq(&self, other: &Option<&Name>) -> bool {
    if let Some(other) = other { &self == other } else { false }
  }
}

impl PartialEq<Name> for Option<&Name> {
  fn eq(&self, other: &Name) -> bool {
    other.eq(self)
  }
}

pub fn num_to_name(mut num: u64) -> String {
  let mut name = String::new();
  loop {
    let c = (num % 26) as u8 + b'a';
    name.push(c as char);
    num /= 26;
    if num == 0 {
      break;
    }
  }
  name
}

impl Tag {
  pub fn adt_name(name: &Name) -> Self {
    Self::Named(name.clone())
  }
}

impl Clone for Term {
  fn clone(&self) -> Self {
    maybe_grow(|| match self {
      Self::Lam { tag, pat, bod } => Self::Lam { tag: tag.clone(), pat: pat.clone(), bod: bod.clone() },
      Self::Var { nam } => Self::Var { nam: nam.clone() },
      Self::Lnk { nam } => Self::Lnk { nam: nam.clone() },
      Self::Let { pat, val, nxt } => Self::Let { pat: pat.clone(), val: val.clone(), nxt: nxt.clone() },
      Self::Bnd { fun, ask, val, nxt } => {
        Self::Bnd { fun: fun.clone(), ask: ask.clone(), val: val.clone(), nxt: nxt.clone() }
      }
      Self::Use { nam, val, nxt } => Self::Use { nam: nam.clone(), val: val.clone(), nxt: nxt.clone() },
      Self::App { tag, fun, arg } => Self::App { tag: tag.clone(), fun: fun.clone(), arg: arg.clone() },
      Self::Fan { fan, tag, els } => Self::Fan { fan: *fan, tag: tag.clone(), els: els.clone() },
      Self::Num { val } => Self::Num { val: *val },
      Self::Nat { val } => Self::Nat { val: *val },
      Self::Str { val } => Self::Str { val: val.clone() },
      Self::Lst { els } => Self::Lst { els: els.clone() },
      Self::Opx { opr, fst, snd } => Self::Opx { opr: *opr, fst: fst.clone(), snd: snd.clone() },
      Self::Mat { arg, bnd, with, arms } => {
        Self::Mat { arg: arg.clone(), bnd: bnd.clone(), with: with.clone(), arms: arms.clone() }
      }
      Self::Swt { arg, bnd, with, pred, arms } => Self::Swt {
        arg: arg.clone(),
        bnd: bnd.clone(),
        with: with.clone(),
        pred: pred.clone(),
        arms: arms.clone(),
      },
      Self::Ref { nam } => Self::Ref { nam: nam.clone() },
      Self::Era => Self::Era,
      Self::Err => Self::Err,
    })
  }
}

impl Drop for Term {
  fn drop(&mut self) {
    loop {
      // Each iteration moves a child with nested nodes to the last child.
      // When no nested on the left, we can just drop it and they'll be handled
      // by the special cases;
      let mut i = self.children_mut().filter(|x| x.children().next().is_some());

      // No nested children, just drop everything
      let Some(b) = i.next() else { break };

      // Only one child with nested nodes, move it up to be the new root.
      // Non-nested (height=0) children are dropped recursively.
      if { i }.next().is_none() {
        *self = std::mem::take(b);
        continue;
      }

      // Rotate the tree right:
      // ```text
      //     a            b
      //    / \          / \
      //   b   e   ->   c   a
      //  / \              / \
      // c   d            d   e
      // ```
      let tmp = Term::Err;
      let d = std::mem::replace(b.children_mut().next_back().unwrap(), tmp);
      let b = std::mem::replace(b, d);
      let a = std::mem::replace(self, b);
      let tmp = std::mem::replace(self.children_mut().next_back().unwrap(), a);
      std::mem::forget(tmp);
    }
  }
}

impl From<Option<Name>> for Pattern {
  fn from(value: Option<Name>) -> Self {
    Pattern::Var(value)
  }
}

impl Term {
  /* Common construction patterns */
  pub fn lam(pat: Pattern, bod: Term) -> Self {
    Self::tagged_lam(Tag::Static, pat, bod)
  }

  pub fn tagged_lam(tag: Tag, pat: Pattern, bod: Term) -> Self {
    Term::Lam { tag, pat: Box::new(pat), bod: Box::new(bod) }
  }

  pub fn var_or_era(nam: Option<Name>) -> Self {
    if let Some(nam) = nam { Term::Var { nam } } else { Term::Era }
  }

  pub fn app(fun: Term, arg: Term) -> Self {
    Self::tagged_app(Tag::Static, fun, arg)
  }

  pub fn tagged_app(tag: Tag, fun: Term, arg: Term) -> Self {
    Term::App { tag, fun: Box::new(fun), arg: Box::new(arg) }
  }

  /// Make a call term by folding args around a called function term with applications.
  pub fn call(called: Term, args: impl IntoIterator<Item = Term>) -> Self {
    args.into_iter().fold(called, Term::app)
  }

  pub fn tagged_call(tag: Tag, called: Term, args: impl IntoIterator<Item = Term>) -> Self {
    args.into_iter().fold(called, |acc, arg| Term::tagged_app(tag.clone(), acc, arg))
  }

  /// Apply a variable to a term by the var name.
  pub fn arg_call(fun: Term, arg: Name) -> Self {
    Term::app(fun, Term::Var { nam: arg })
  }

  pub fn r#ref(name: &str) -> Self {
    Term::Ref { nam: Name::new(name) }
  }

  pub fn str(str: &str) -> Self {
    Term::Str { val: STRINGS.get(str) }
  }

  pub fn sub_num(arg: Term, val: u64) -> Term {
    if val == 0 {
      arg
    } else {
      Term::Opx {
        opr: Op { ty: OpType::U60, op: IntOp::Sub },
        fst: Box::new(arg),
        snd: Box::new(Term::Num { val }),
      }
    }
  }

  pub fn add_num(arg: Term, val: u64) -> Term {
    if val == 0 {
      arg
    } else {
      Term::Opx {
        opr: Op { ty: OpType::U60, op: IntOp::Add },
        fst: Box::new(arg),
        snd: Box::new(Term::Num { val }),
      }
    }
  }

  pub fn pattern(&self) -> Option<&Pattern> {
    match self {
      Term::Lam { pat, .. } | Term::Let { pat, .. } => Some(pat),
      _ => None,
    }
  }

  pub fn pattern_mut(&mut self) -> Option<&mut Pattern> {
    match self {
      Term::Lam { pat, .. } | Term::Let { pat, .. } => Some(pat),
      _ => None,
    }
  }

  /* Iterators */
  pub fn children(&self) -> impl DoubleEndedIterator<Item = &Term> + Clone {
    multi_iterator!(ChildrenIter { Zero, One, Two, Vec, Mat, Swt });
    match self {
      Term::Mat { arg, bnd: _, with: _, arms } => {
        ChildrenIter::Mat([arg.as_ref()].into_iter().chain(arms.iter().map(|r| &r.2)))
      }
      Term::Swt { arg, bnd: _, with: _, pred: _, arms } => {
        ChildrenIter::Swt([arg.as_ref()].into_iter().chain(arms))
      }
      Term::Fan { els, .. } | Term::Lst { els } => ChildrenIter::Vec(els),
      Term::Let { val: fst, nxt: snd, .. }
      | Term::Bnd { val: fst, nxt: snd, .. }
      | Term::Use { val: fst, nxt: snd, .. }
      | Term::App { fun: fst, arg: snd, .. }
      | Term::Opx { fst, snd, .. } => ChildrenIter::Two([fst.as_ref(), snd.as_ref()]),
      Term::Lam { bod, .. } => ChildrenIter::One([bod.as_ref()]),
      Term::Var { .. }
      | Term::Lnk { .. }
      | Term::Num { .. }
      | Term::Nat { .. }
      | Term::Str { .. }
      | Term::Ref { .. }
      | Term::Era
      | Term::Err => ChildrenIter::Zero([]),
    }
  }

  pub fn children_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut Term> {
    multi_iterator!(ChildrenIter { Zero, One, Two, Vec, Mat, Swt });
    match self {
      Term::Mat { arg, bnd: _, with: _, arms: rules } => {
        ChildrenIter::Mat([arg.as_mut()].into_iter().chain(rules.iter_mut().map(|r| &mut r.2)))
      }
      Term::Swt { arg, bnd: _, with: _, pred: _, arms } => {
        ChildrenIter::Swt([arg.as_mut()].into_iter().chain(arms))
      }
      Term::Fan { els, .. } | Term::Lst { els } => ChildrenIter::Vec(els),
      Term::Let { val: fst, nxt: snd, .. }
      | Term::Bnd { val: fst, nxt: snd, .. }
      | Term::Use { val: fst, nxt: snd, .. }
      | Term::App { fun: fst, arg: snd, .. }
      | Term::Opx { fst, snd, .. } => ChildrenIter::Two([fst.as_mut(), snd.as_mut()]),
      Term::Lam { bod, .. } => ChildrenIter::One([bod.as_mut()]),
      Term::Var { .. }
      | Term::Lnk { .. }
      | Term::Num { .. }
      | Term::Nat { .. }
      | Term::Str { .. }
      | Term::Ref { .. }
      | Term::Era
      | Term::Err => ChildrenIter::Zero([]),
    }
  }

  /// An iterator over the subterms with an iterator over the binds
  /// introduced by the current term for each subterm.
  ///
  /// Must only be called after fix_matches.
  ///
  /// Example: A lambda introduces 1 bind for it's only subterm,
  /// while a let expression introduces 0 binds for the value and
  /// many binds for the next term.
  pub fn children_with_binds(
    &self,
  ) -> impl DoubleEndedIterator<Item = (&Term, impl DoubleEndedIterator<Item = &Option<Name>> + Clone)> + Clone
  {
    multi_iterator!(ChildrenIter { Zero, One, Two, Vec, Mat, Swt });
    multi_iterator!(BindsIter { Zero, One, Mat, Pat });
    match self {
      Term::Mat { arg, bnd: _, with: _, arms: rules } => ChildrenIter::Mat(
        [(arg.as_ref(), BindsIter::Zero([]))]
          .into_iter()
          .chain(rules.iter().map(move |r| (&r.2, BindsIter::Mat(r.1.iter())))),
      ),
      Term::Swt { arg, bnd: _, with: _, pred, arms: rules } => {
        let (succ, nums) = rules.split_last().unwrap();
        ChildrenIter::Swt(
          [(arg.as_ref(), BindsIter::Zero([]))]
            .into_iter()
            .chain(nums.iter().map(move |x| (x, BindsIter::Zero([]))))
            .chain([(succ, BindsIter::One([pred]))]),
        )
      }
      Term::Fan { els, .. } | Term::Lst { els } => {
        ChildrenIter::Vec(els.iter().map(|el| (el, BindsIter::Zero([]))))
      }
      Term::Let { pat, val, nxt, .. } => {
        ChildrenIter::Two([(val.as_ref(), BindsIter::Zero([])), (nxt.as_ref(), BindsIter::Pat(pat.binds()))])
      }
      Term::Bnd { ask, val, nxt, .. } => {
        ChildrenIter::Two([(val.as_ref(), BindsIter::Zero([])), (nxt.as_ref(), BindsIter::Pat(ask.binds()))])
      }
      Term::Use { nam, val, nxt, .. } => {
        ChildrenIter::Two([(val.as_ref(), BindsIter::Zero([])), (nxt.as_ref(), BindsIter::One([nam]))])
      }
      Term::App { fun: fst, arg: snd, .. } | Term::Opx { fst, snd, .. } => {
        ChildrenIter::Two([(fst.as_ref(), BindsIter::Zero([])), (snd.as_ref(), BindsIter::Zero([]))])
      }
      Term::Lam { pat, bod, .. } => ChildrenIter::One([(bod.as_ref(), BindsIter::Pat(pat.binds()))]),
      Term::Var { .. }
      | Term::Lnk { .. }
      | Term::Num { .. }
      | Term::Nat { .. }
      | Term::Str { .. }
      | Term::Ref { .. }
      | Term::Era
      | Term::Err => ChildrenIter::Zero([]),
    }
  }

  /// Must only be called after fix_matches.
  pub fn children_mut_with_binds(
    &mut self,
  ) -> impl DoubleEndedIterator<Item = (&mut Term, impl DoubleEndedIterator<Item = &Option<Name>> + Clone)>
  {
    multi_iterator!(ChildrenIter { Zero, One, Two, Vec, Mat, Swt });
    multi_iterator!(BindsIter { Zero, One, Mat, Pat });
    match self {
      Term::Mat { arg, bnd: _, with: _, arms: rules } => ChildrenIter::Mat(
        [(arg.as_mut(), BindsIter::Zero([]))]
          .into_iter()
          .chain(rules.iter_mut().map(move |r| (&mut r.2, BindsIter::Mat(r.1.iter())))),
      ),
      Term::Swt { arg, bnd: _, with: _, pred, arms: rules } => {
        let (succ, nums) = rules.split_last_mut().unwrap();
        ChildrenIter::Swt(
          [(arg.as_mut(), BindsIter::Zero([]))]
            .into_iter()
            .chain(nums.iter_mut().map(move |x| (x, BindsIter::Zero([]))))
            .chain([(succ, BindsIter::One([&*pred]))]),
        )
      }
      Term::Fan { els, .. } | Term::Lst { els } => {
        ChildrenIter::Vec(els.iter_mut().map(|el| (el, BindsIter::Zero([]))))
      }
      Term::Let { pat, val, nxt, .. } => {
        ChildrenIter::Two([(val.as_mut(), BindsIter::Zero([])), (nxt.as_mut(), BindsIter::Pat(pat.binds()))])
      }
      Term::Bnd { val, nxt, .. } => {
        ChildrenIter::Two([(val.as_mut(), BindsIter::Zero([])), (nxt.as_mut(), BindsIter::Zero([]))])
      }
      Term::Use { nam, val, nxt } => {
        ChildrenIter::Two([(val.as_mut(), BindsIter::Zero([])), (nxt.as_mut(), BindsIter::One([&*nam]))])
      }
      Term::App { fun: fst, arg: snd, .. } | Term::Opx { fst, snd, .. } => {
        ChildrenIter::Two([(fst.as_mut(), BindsIter::Zero([])), (snd.as_mut(), BindsIter::Zero([]))])
      }
      Term::Lam { pat, bod, .. } => ChildrenIter::One([(bod.as_mut(), BindsIter::Pat(pat.binds()))]),
      Term::Var { .. }
      | Term::Lnk { .. }
      | Term::Num { .. }
      | Term::Nat { .. }
      | Term::Str { .. }
      | Term::Ref { .. }
      | Term::Era
      | Term::Err => ChildrenIter::Zero([]),
    }
  }

  /// Must only be called after fix_matches.
  pub fn children_mut_with_binds_mut(
    &mut self,
  ) -> impl DoubleEndedIterator<Item = (&mut Term, impl DoubleEndedIterator<Item = &mut Option<Name>>)> {
    multi_iterator!(ChildrenIter { Zero, One, Two, Vec, Mat, Swt });
    multi_iterator!(BindsIter { Zero, One, Mat, Pat });
    match self {
      Term::Mat { arg, bnd: _, with: _, arms: rules } => ChildrenIter::Mat(
        [(arg.as_mut(), BindsIter::Zero([]))]
          .into_iter()
          .chain(rules.iter_mut().map(move |r| (&mut r.2, BindsIter::Mat(r.1.iter_mut())))),
      ),
      Term::Swt { arg, bnd: _, with: _, pred, arms: rules } => {
        let (succ, nums) = rules.split_last_mut().unwrap();
        ChildrenIter::Swt(
          [(arg.as_mut(), BindsIter::Zero([]))]
            .into_iter()
            .chain(nums.iter_mut().map(move |x| (x, BindsIter::Zero([]))))
            .chain([(succ, BindsIter::One([pred]))]),
        )
      }
      Term::Fan { els, .. } | Term::Lst { els } => {
        ChildrenIter::Vec(els.iter_mut().map(|el| (el, BindsIter::Zero([]))))
      }
      Term::Use { nam, val, nxt } => {
        ChildrenIter::Two([(val.as_mut(), BindsIter::Zero([])), (nxt.as_mut(), BindsIter::One([nam]))])
      }
      Term::Let { pat, val, nxt, .. } => ChildrenIter::Two([
        (val.as_mut(), BindsIter::Zero([])),
        (nxt.as_mut(), BindsIter::Pat(pat.binds_mut())),
      ]),
      Term::Bnd { ask, val, nxt, .. } => ChildrenIter::Two([
        (val.as_mut(), BindsIter::Zero([])),
        (nxt.as_mut(), BindsIter::Pat(ask.binds_mut())),
      ]),
      Term::App { fun: fst, arg: snd, .. } | Term::Opx { fst, snd, .. } => {
        ChildrenIter::Two([(fst.as_mut(), BindsIter::Zero([])), (snd.as_mut(), BindsIter::Zero([]))])
      }
      Term::Lam { pat, bod, .. } => ChildrenIter::One([(bod.as_mut(), BindsIter::Pat(pat.binds_mut()))]),
      Term::Var { .. }
      | Term::Lnk { .. }
      | Term::Num { .. }
      | Term::Nat { .. }
      | Term::Str { .. }
      | Term::Ref { .. }
      | Term::Era
      | Term::Err => ChildrenIter::Zero([]),
    }
  }

  /* Common checks and transformations */

  /// Substitute the occurrences of a variable in a term with the given term.
  ///
  /// Caution: can cause invalid shadowing of variables if used incorrectly.
  /// Ex: Using subst to beta-reduce `(@a @b a b)` converting it into `@b b`.
  ///
  /// NOTE: Expects var bind information to be properly stored in match expressions,
  /// so it must run AFTER `fix_match_terms`.
  ///
  /// NOTE: Since it doesn't (can't) handle `with` clauses in match terms,
  /// it must be run only AFTER `with` linearization.
  pub fn subst(&mut self, from: &Name, to: &Term) {
    maybe_grow(|| {
      for (child, binds) in self.children_mut_with_binds() {
        if !binds.flat_map(|b| b.as_ref()).contains(from) {
          child.subst(from, to);
        }
      }
    });

    if let Term::Var { nam } = self
      && nam == from
    {
      *self = to.clone();
    }
  }

  /// Substitute the occurrence of an unscoped variable with the given term.
  pub fn subst_unscoped(&mut self, from: &Name, to: &Term) {
    maybe_grow(|| {
      // We don't check the unscoped binds because there can be only one bind of an unscoped var.
      // TODO: potentially there could be some situation where this causes an incorrect program to compile?
      for child in self.children_mut() {
        child.subst_unscoped(from, to);
      }
    });

    if let Term::Lnk { nam } = self
      && nam == from
    {
      *self = to.clone();
    }
  }

  /// Collects all the free variables that a term has
  /// and the number of times each var is used
  pub fn free_vars(&self) -> HashMap<Name, u64> {
    fn go_term(term: &Term, free_vars: &mut HashMap<Name, u64>) {
      maybe_grow(|| {
        if let Term::Var { nam } = term {
          *free_vars.entry(nam.clone()).or_default() += 1;
        }

        for (child, binds) in term.children_with_binds() {
          let mut new_scope = Default::default();
          go_term(child, &mut new_scope);

          for nam in binds.flatten() {
            new_scope.remove(nam);
          }

          free_vars.extend(new_scope);
        }
      })
    }

    let mut free_vars = Default::default();
    go_term(self, &mut free_vars);
    free_vars
  }

  /// Returns the set of declared and the set of used unscoped variables
  pub fn unscoped_vars(&self) -> (IndexSet<Name>, IndexSet<Name>) {
    fn go_pat(pat: &Pattern, decls: &mut IndexSet<Name>) {
      maybe_grow(|| {
        if let Pattern::Chn(name) = pat {
          decls.insert(name.clone());
        }

        for child in pat.children() {
          go_pat(child, decls);
        }
      })
    }
    fn go_term(term: &Term, decls: &mut IndexSet<Name>, uses: &mut IndexSet<Name>) {
      maybe_grow(|| {
        if let Term::Lnk { nam } = term {
          uses.insert(nam.clone());
        }

        if let Some(pat) = term.pattern() {
          go_pat(pat, decls)
        }

        for child in term.children() {
          go_term(child, decls, uses);
        }
      })
    }
    let mut decls = Default::default();
    let mut uses = Default::default();
    go_term(self, &mut decls, &mut uses);
    (decls, uses)
  }

  pub fn has_unscoped(&self) -> bool {
    maybe_grow(|| {
      let mut has_unscoped = match self {
        Term::Let { pat, .. } if pat.has_unscoped() => true,
        Term::Lnk { .. } => true,
        _ => false,
      };
      for child in self.children() {
        if has_unscoped {
          return true;
        }
        has_unscoped |= child.has_unscoped()
      }
      has_unscoped
    })
  }
}

impl Pattern {
  pub fn binds(&self) -> impl DoubleEndedIterator<Item = &Option<Name>> + Clone {
    self.iter().filter_map(|pat| match pat {
      Pattern::Var(nam) => Some(nam),
      _ => None,
    })
  }

  pub fn binds_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut Option<Name>> {
    // Can't have a Pattern::iter_mut() since it has a tree-like structure.
    let mut binds = vec![];
    let mut to_visit = vec![self];
    while let Some(pat) = to_visit.pop() {
      match pat {
        Pattern::Var(nam) => binds.push(nam),
        _ => to_visit.extend(pat.children_mut().rev()),
      }
    }
    binds.into_iter()
  }

  /// Returns an iterator over each immediate child sub-pattern of `self`.
  /// Considers Lists as its own pattern and not a sequence of Cons.
  pub fn children(&self) -> impl DoubleEndedIterator<Item = &Pattern> + Clone {
    multi_iterator!(ChildrenIter { Zero, Vec });
    match self {
      Pattern::Ctr(_, els) | Pattern::Fan(.., els) | Pattern::Lst(els) => ChildrenIter::Vec(els.iter()),
      Pattern::Var(_) | Pattern::Chn(_) | Pattern::Num(_) | Pattern::Str(_) | Pattern::Err => {
        ChildrenIter::Zero([])
      }
    }
  }

  pub fn children_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut Pattern> {
    multi_iterator!(ChildrenIter { Zero, Vec });
    match self {
      Pattern::Ctr(_, els) | Pattern::Fan(.., els) | Pattern::Lst(els) => ChildrenIter::Vec(els.iter_mut()),
      Pattern::Var(_) | Pattern::Chn(_) | Pattern::Num(_) | Pattern::Str(_) | Pattern::Err => {
        ChildrenIter::Zero([])
      }
    }
  }

  /// Returns an iterator over each subpattern in depth-first, left to right order.
  // TODO: Not lazy.
  pub fn iter(&self) -> impl DoubleEndedIterator<Item = &Pattern> + Clone {
    let mut to_visit = vec![self];
    let mut els = vec![];
    while let Some(pat) = to_visit.pop() {
      els.push(pat);
      to_visit.extend(pat.children().rev());
    }
    els.into_iter()
  }

  pub fn is_wildcard(&self) -> bool {
    matches!(self, Pattern::Var(_) | Pattern::Chn(_))
  }

  pub fn to_term(&self) -> Term {
    match self {
      Pattern::Var(nam) => Term::var_or_era(nam.clone()),
      Pattern::Chn(nam) => Term::Lnk { nam: nam.clone() },
      Pattern::Ctr(ctr, args) => {
        Term::call(Term::Ref { nam: ctr.clone() }, args.iter().map(|arg| arg.to_term()))
      }
      Pattern::Num(val) => Term::Num { val: *val },
      Pattern::Fan(fan, tag, args) => {
        Term::Fan { fan: *fan, tag: tag.clone(), els: args.iter().map(|p| p.to_term()).collect() }
      }
      Pattern::Lst(_) | Pattern::Str(_) => todo!(),
      Pattern::Err => Term::Err,
    }
  }

  pub fn has_unscoped(&self) -> bool {
    match self {
      Pattern::Chn(_) => true,
      Pattern::Var(_) | Pattern::Str(_) | Pattern::Num(_) | Pattern::Err => false,
      Pattern::Ctr(_, x) | Pattern::Fan(_, _, x) | Pattern::Lst(x) => x.iter().any(|x| x.has_unscoped()),
    }
  }
}

impl Rule {
  pub fn arity(&self) -> usize {
    self.pats.len()
  }
}

impl Definition {
  pub fn arity(&self) -> usize {
    self.rules[0].arity()
  }

  #[track_caller]
  pub fn assert_no_pattern_matching_rules(&self) {
    assert!(self.rules.len() == 1, "Definition rules should have been removed in earlier pass");
    assert!(self.rules[0].pats.is_empty(), "Definition args should have been removed in an earlier pass");
  }

  #[track_caller]
  pub fn rule(&self) -> &Rule {
    self.assert_no_pattern_matching_rules();
    &self.rules[0]
  }

  #[track_caller]
  pub fn rule_mut(&mut self) -> &mut Rule {
    self.assert_no_pattern_matching_rules();
    &mut self.rules[0]
  }
}

impl Name {
  pub fn new<'a, V: Into<Cow<'a, str>>>(value: V) -> Name {
    Name(STRINGS.get(value))
  }

  pub fn is_generated(&self) -> bool {
    // Generated def names use $ while var names use %
    self.contains('$') || self.contains('%')
  }
}

impl Default for Name {
  fn default() -> Self {
    Self::new("")
  }
}

impl From<u64> for Name {
  fn from(value: u64) -> Self {
    Name::new(num_to_name(value).as_str())
  }
}

impl From<u32> for Name {
  fn from(value: u32) -> Self {
    Name::new(num_to_name(value as u64).as_str())
  }
}

impl Deref for Name {
  type Target = str;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl AsRef<str> for Name {
  fn as_ref(&self) -> &str {
    &self.0
  }
}

impl Book {
  pub fn hvmc_entrypoint(&self) -> &str {
    match self.entrypoint.as_ref().map(|e| e.as_ref()) {
      Some("main" | "Main") | None => ENTRY_POINT,
      Some(nam) => nam,
    }
  }
}
