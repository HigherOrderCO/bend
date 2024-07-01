pub mod gen_map_get;
mod order_kwargs;
pub mod parser;
pub mod to_fun;

use crate::fun::{CtrField, Name, Num, Op};
use interner::global::GlobalString;

#[derive(Clone, Debug)]
pub enum Expr {
  // "*"
  Era,
  // [a-zA-Z_]+
  Var { nam: Name },
  // "$" [a-zA-Z_]+
  Chn { nam: Name },
  // [0-9_]+
  Num { val: Num },
  // {fun}({args},{kwargs},)
  Call { fun: Box<Expr>, args: Vec<Expr>, kwargs: Vec<(Name, Expr)> },
  // "lambda" {names}* ":" {bod}
  Lam { names: Vec<(Name, bool)>, bod: Box<Expr> },
  // {lhs} {op} {rhs}
  Opr { op: Op, lhs: Box<Expr>, rhs: Box<Expr> },
  // "\"" ... "\""
  Str { val: GlobalString },
  // "[" ... "]"
  Lst { els: Vec<Expr> },
  // "(" ... ")"
  Tup { els: Vec<Expr> },
  // "{" {els} "}"
  Sup { els: Vec<Expr> },
  // {name} "{" {kwargs} "}"
  Ctr { name: Name, args: Vec<Expr>, kwargs: Vec<(Name, Expr)> },
  // "[" {term} "for" {bind} "in" {iter} ("if" {cond})? "]"
  LstMap { term: Box<Expr>, bind: Name, iter: Box<Expr>, cond: Option<Box<Expr>> },
  // "{" {entries} "}"
  Map { entries: Vec<(Expr, Expr)> },
  // {map} "[" {key} "]"
  MapGet { nam: Name, key: Box<Expr> },
  // "![" {left} "," {right} "]"
  TreeNode { left: Box<Expr>, right: Box<Expr> },
  // "!" {val}
  TreeLeaf { val: Box<Expr> },
}

#[derive(Clone, Debug)]
pub struct MatchArm {
  pub lft: Option<Name>,
  pub rgt: Stmt,
}

#[derive(Clone, Debug, Default)]
pub enum AssignPattern {
  // "*"
  #[default]
  Eraser,
  // [a-zA-Z_]+
  Var(Name),
  // "$" [a-zA-Z_]+
  Chn(Name),
  // "(" ... ")"
  Tup(Vec<AssignPattern>),
  // "{" ... "}"
  Sup(Vec<AssignPattern>),
  // {name} "[" {expr} "]"
  MapSet(Name, Expr),
}

#[derive(Clone, Debug)]
pub enum InPlaceOp {
  Add,
  Sub,
  Mul,
  Div,
  And,
  Or,
  Xor,
  Map,
}

#[derive(Clone, Debug, Default)]
pub enum Stmt {
  // {pat} = {val} ";"? {nxt}
  Assign {
    pat: AssignPattern,
    val: Box<Expr>,
    nxt: Option<Box<Stmt>>,
  },
  // {var} += {val} ";"? {nxt}
  InPlace {
    op: InPlaceOp,
    pat: Box<AssignPattern>,
    val: Box<Expr>,
    nxt: Box<Stmt>,
  },
  // "if" {cond} ":"
  //  {then}
  // "else" ":"
  //  {otherwise}
  // <nxt>?
  If {
    cond: Box<Expr>,
    then: Box<Stmt>,
    otherwise: Box<Stmt>,
    nxt: Option<Box<Stmt>>,
  },
  // "match" ({bind} "=")? {arg} ({with_clause})? ":"
  //   case {lft} ":" {rgt}
  //   ...
  // <nxt>?
  Match {
    arg: Box<Expr>,
    bnd: Option<Name>,
    with_bnd: Vec<Option<Name>>,
    with_arg: Vec<Expr>,
    arms: Vec<MatchArm>,
    nxt: Option<Box<Stmt>>,
  },
  // "switch" ({bind} "=")? {arg} ({with_clause})? ":"
  //   case 0..wildcard ":" {rgt}
  //   ...
  // <nxt>?
  Switch {
    arg: Box<Expr>,
    bnd: Option<Name>,
    with_bnd: Vec<Option<Name>>,
    with_arg: Vec<Expr>,
    arms: Vec<Stmt>,
    nxt: Option<Box<Stmt>>,
  },
  // "bend" ({bind} ("=" {init})?)* "while" {cond} ":"
  //   {step}
  // "then" ":"
  //   {base}
  // <nxt>?
  Bend {
    bnd: Vec<Option<Name>>,
    arg: Vec<Expr>,
    cond: Box<Expr>,
    step: Box<Stmt>,
    base: Box<Stmt>,
    nxt: Option<Box<Stmt>>,
  },
  // "fold" ({bind} "=")? {arg} ({with_clause})? ":" {arms}
  //   case {lft} ":" {rgt}
  //   ...
  // <nxt>?
  Fold {
    arg: Box<Expr>,
    bnd: Option<Name>,
    with_bnd: Vec<Option<Name>>,
    with_arg: Vec<Expr>,
    arms: Vec<MatchArm>,
    nxt: Option<Box<Stmt>>,
  },
  // "with" {fun} ":"
  //   {block}
  // <nxt>?
  With {
    typ: Name,
    bod: Box<Stmt>,
    nxt: Option<Box<Stmt>>,
  },
  // {pat} <- {val} ";"? {nxt}
  Ask {
    pat: AssignPattern,
    val: Box<Expr>,
    nxt: Box<Stmt>,
  },
  // "return" {expr} ";"?
  Return {
    term: Box<Expr>,
  },
  // "open" {typ} ":" {var} ";"? {nxt}
  Open {
    typ: Name,
    var: Name,
    nxt: Box<Stmt>,
  },
  // "use" {name} "=" {expr} ";"? {nxt}
  Use {
    nam: Name,
    val: Box<Expr>,
    nxt: Box<Stmt>,
  },
  // {def} {nxt}
  LocalDef {
    def: Box<Definition>,
    nxt: Box<Stmt>,
  },
  #[default]
  Err,
}

// Name "{" {fields}* "}"
#[derive(Clone, Debug)]
pub struct Variant {
  pub name: Name,
  pub fields: Vec<CtrField>,
}

// "def" {name} "(" {params} ")" ":" {body}
#[derive(Clone, Debug)]
pub struct Definition {
  pub name: Name,
  pub params: Vec<Name>,
  pub body: Stmt,
}

// "enum" ":" {variants}*
#[derive(Clone, Debug)]
pub struct Enum {
  pub name: Name,
  pub variants: Vec<Variant>,
}

impl InPlaceOp {
  pub fn to_lang_op(self) -> Op {
    match self {
      InPlaceOp::Add => Op::ADD,
      InPlaceOp::Sub => Op::SUB,
      InPlaceOp::Mul => Op::MUL,
      InPlaceOp::Div => Op::DIV,
      InPlaceOp::And => Op::AND,
      InPlaceOp::Or => Op::OR,
      InPlaceOp::Xor => Op::XOR,
      InPlaceOp::Map => unreachable!(),
    }
  }
}
