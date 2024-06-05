use super::{
  parser::{ParseBook, TermParser},
  Book, Name, Num, Pattern, Term,
};
use crate::maybe_grow;

const BUILTINS: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/fun/builtins.bend"));

pub const LIST: &str = "List";
pub const LCONS: &str = "List/Cons";
pub const LNIL: &str = "List/Nil";
pub const LCONS_TAG: u32 = 1;
pub const LNIL_TAG_REF: &str = "List/Nil/tag";
pub const LCONS_TAG_REF: &str = "List/Cons/tag";

pub const HEAD: &str = "head";
pub const TAIL: &str = "tail";

pub const STRING: &str = "String";
pub const SCONS: &str = "String/Cons";
pub const SNIL: &str = "String/Nil";
pub const SCONS_TAG: u32 = 1;
pub const SNIL_TAG_REF: &str = "String/Nil/tag";
pub const SCONS_TAG_REF: &str = "String/Cons/tag";

pub const NAT: &str = "Nat";
pub const NAT_SUCC: &str = "Nat/Succ";
pub const NAT_ZERO: &str = "Nat/Zero";
pub const NAT_SUCC_TAG: u32 = 0;

pub const TREE: &str = "Tree";
pub const TREE_NODE: &str = "Tree/Node";
pub const TREE_LEAF: &str = "Tree/Leaf";

pub const MAP: &str = "Map";
pub const MAP_NODE: &str = "Map/Node";
pub const MAP_LEAF: &str = "Map/Leaf";

pub const IO: &str = "IO";
pub const IO_DONE: &str = "IO/Done";
pub const IO_CALL: &str = "IO/Call";

pub const BUILTIN_CTRS: &[&str] =
  &[LCONS, LNIL, SCONS, SNIL, NAT_SUCC, NAT_ZERO, TREE_NODE, TREE_LEAF, MAP_NODE, MAP_LEAF, IO_DONE, IO_CALL];

pub const BUILTIN_TYPES: &[&str] = &[LIST, STRING, NAT, TREE, MAP, IO];

impl ParseBook {
  pub fn builtins() -> Self {
    TermParser::new(BUILTINS)
      .parse_book(Self::default(), true)
      .expect("Error parsing builtin file, this should not happen")
  }
}

impl Book {
  pub fn encode_builtins(&mut self) {
    for def in self.defs.values_mut() {
      for rule in def.rules.iter_mut() {
        rule.pats.iter_mut().for_each(Pattern::encode_builtins);
        rule.body.encode_builtins();
      }
    }
  }
}

impl Term {
  fn encode_builtins(&mut self) {
    maybe_grow(|| match self {
      Term::List { els } => *self = Term::encode_list(std::mem::take(els)),
      Term::Str { val } => *self = Term::encode_str(val),
      Term::Nat { val } => *self = Term::encode_nat(*val),
      _ => {
        for child in self.children_mut() {
          child.encode_builtins();
        }
      }
    })
  }

  fn encode_list(elements: Vec<Term>) -> Term {
    elements.into_iter().rfold(Term::r#ref(LNIL), |acc, mut nxt| {
      nxt.encode_builtins();
      Term::call(Term::r#ref(LCONS), [nxt, acc])
    })
  }

  pub fn encode_str(val: &str) -> Term {
    val.chars().rfold(Term::r#ref(SNIL), |acc, char| {
      Term::call(Term::r#ref(SCONS), [Term::Num { val: Num::U24(char as u32 & 0x00ff_ffff) }, acc])
    })
  }

  pub fn encode_nat(val: u32) -> Term {
    (0..val).fold(Term::r#ref(NAT_ZERO), |acc, _| Term::app(Term::r#ref(NAT_SUCC), acc))
  }
}

impl Pattern {
  pub fn encode_builtins(&mut self) {
    match self {
      Pattern::Lst(pats) => *self = Self::encode_list(std::mem::take(pats)),
      Pattern::Str(str) => *self = Self::encode_str(str),
      _ => {
        for pat in self.children_mut() {
          pat.encode_builtins();
        }
      }
    }
  }

  fn encode_list(elements: Vec<Pattern>) -> Pattern {
    let lnil = Pattern::Ctr(Name::new(LNIL), vec![]);

    elements.into_iter().rfold(lnil, |acc, mut nxt| {
      nxt.encode_builtins();
      Pattern::Ctr(Name::new(LCONS), vec![nxt, acc])
    })
  }

  fn encode_str(str: &str) -> Pattern {
    let lnil = Pattern::Ctr(Name::new(SNIL), vec![]);

    str.chars().rfold(lnil, |tail, head| {
      let head = Pattern::Num(head as u32);
      Pattern::Ctr(Name::new(SCONS), vec![head, tail])
    })
  }
}
