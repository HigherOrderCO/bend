use crate::{
  diagnostics::{Diagnostics, ToStringVerbose},
  maybe_grow,
  term::{Book, Name, Pattern, Tag, Term},
};
use std::{
  collections::{hash_map::Entry, HashMap},
  ops::{Index, IndexMut},
};

use hvmc::ast::{Net, Tree};
use loaned::LoanedMut;

use super::{num_to_name, FanKind};

#[derive(Debug, Clone)]
pub struct ViciousCycleErr;

pub fn encode_book(book: &Book, info: &mut Diagnostics) -> Result<(hvmc::ast::Book, Labels), Diagnostics> {
  info.start_pass();

  let mut hvmc = hvmc::ast::Book::default();
  let mut labels = Labels::default();

  let main = book.entrypoint.as_ref().unwrap();

  for def in book.defs.values() {
    for rule in def.rules.iter() {
      let net = encode_term(&rule.body, &mut labels);

      let name = if def.name == *main { book.hvmc_entrypoint().to_string() } else { def.name.0.to_string() };

      if let Some(net) = info.take_inet_err(net, name.clone()) {
        hvmc.insert(name, net);
      }
    }
  }

  labels.con.finish();
  labels.dup.finish();

  Ok((hvmc, labels))
}

/// Converts an LC term into an IC net.
pub fn encode_term(term: &Term, labels: &mut Labels) -> Result<Net, ViciousCycleErr> {
  let mut net = Net::default();

  let mut state = EncodeTermState {
    vars: Default::default(),
    wires: Default::default(),
    redexes: Default::default(),
    name_idx: 0,
    created_nodes: 0,
    labels,
  };

  state.encode_term(term, Place::Hole(&mut net.root));
  LoanedMut::from(std::mem::take(&mut state.redexes)).place(&mut net.redexes);

  let EncodeTermState { created_nodes, .. } = { state };

  let found_nodes = net.trees().map(count_nodes).sum::<usize>();

  if created_nodes != found_nodes {
    Err(ViciousCycleErr)?
  }

  Ok(net)
}

#[derive(Debug)]
struct EncodeTermState<'t, 'l> {
  vars: HashMap<(bool, Name), Place<'t>>,
  wires: Vec<Option<Place<'t>>>,
  redexes: Vec<LoanedMut<'t, (Tree, Tree)>>,
  name_idx: u64,
  created_nodes: usize,
  labels: &'l mut Labels,
}

fn count_nodes(tree: &Tree) -> usize {
  maybe_grow(|| {
    usize::from(tree.children().next().is_some()) + tree.children().map(count_nodes).sum::<usize>()
  })
}

#[derive(Debug)]
enum Place<'t> {
  Tree(LoanedMut<'t, Tree>),
  Hole(&'t mut Tree),
  Wire(usize),
}

impl<'t, 'l> EncodeTermState<'t, 'l> {
  /// Adds a subterm connected to `up` to the `inet`.
  /// `scope` has the current variable scope.
  /// `vars` has the information of which ports the variables are declared and used in.
  /// `global_vars` has the same information for global lambdas. Must be linked outside this function.
  /// Expects variables to be linear, refs to be stored as Refs and all names to be bound.
  fn encode_term(&mut self, term: &Term, up: Place<'t>) {
    maybe_grow(|| {
      match term {
        Term::Era => self.link(up, Place::Tree(LoanedMut::new(Tree::Era))),
        Term::Var { nam } => self.link_var(false, nam, up),
        Term::Lnk { nam } => self.link_var(true, nam, up),
        Term::Ref { nam } => self.link(up, Place::Tree(LoanedMut::new(Tree::Ref { nam: nam.to_string() }))),
        Term::Num { val } => self.link(up, Place::Tree(LoanedMut::new(Tree::Num { val: *val as i64 }))),
        // A lambda becomes to a con node. Ports:
        // - 0: points to where the lambda occurs.
        // - 1: points to the lambda variable.
        // - 2: points to the lambda body.
        // core: (var_use bod)
        Term::Lam { tag, pat, bod } => {
          let lab = self.labels.generate(&CtrKind::Con(tag.clone()));
          let node = self.new_ctr(lab);
          self.link(up, node.0);
          self.encode_pat(pat, node.1);
          self.encode_term(bod, node.2);
        }
        // An application becomes to a con node too. Ports:
        // - 0: points to the function being applied.
        // - 1: points to the function's argument.
        // - 2: points to where the application occurs.
        // core: & fun ~ (arg ret) (fun not necessarily main port)
        Term::App { tag, fun, arg } => {
          let lab = self.labels.generate(&CtrKind::Con(tag.clone()));
          let node = self.new_ctr(lab);
          self.encode_term(fun, node.0);
          self.encode_term(arg, node.1);
          self.link(up, node.2);
        }
        // core: & arg ~ ?<(zero succ) ret>
        Term::Swt { arg, bnd: _, with, pred: _, arms: rules } => {
          // At this point should be only num matches of 0 and succ.
          assert!(with.is_empty());
          assert!(rules.len() == 2);

          self.created_nodes += 1;
          let ((zero, succ, out), node) =
            LoanedMut::loan_with(Tree::Mat { zero: hole(), succ: hole(), out: hole() }, |t, l| {
              let Tree::Mat { zero, succ, out, .. } = t else { unreachable!() };
              (l.loan_mut(zero), l.loan_mut(succ), l.loan_mut(out))
            });

          self.encode_term(arg, Place::Tree(node));
          self.encode_term(&rules[0], Place::Hole(zero));
          self.encode_term(&rules[1], Place::Hole(succ));
          self.link(up, Place::Hole(out));
        }
        Term::Let { pat, val, nxt } => {
          let wire = self.new_wire();
          self.encode_term(val, Place::Wire(wire));
          self.encode_pat(pat, Place::Wire(wire));

          self.encode_term(nxt, up);
        }
        Term::Fan { fan, tag, els } => {
          let kind = CtrKind::Fan(*fan, tag.clone());
          self.make_node_list(kind, up, els.iter().map(|el| |slf: &mut Self, up| slf.encode_term(el, up)));
        }
        // core: & fst ~ <op snd ret>
        Term::Opx { opr, fst, snd } => {
          self.created_nodes += 1;
          let ((rhs, out), lhs) =
            LoanedMut::loan_with(Tree::Op { op: *opr, rhs: hole(), out: hole() }, |t, l| {
              let Tree::Op { rhs, out, .. } = t else { unreachable!() };
              (l.loan_mut(rhs), l.loan_mut(out))
            });

          self.encode_term(fst, Place::Tree(lhs));
          self.encode_term(snd, Place::Hole(rhs));
          self.link(up, Place::Hole(out));
        }
        Term::Use { .. }  // Removed in earlier pass
        | Term::Bnd { .. } // Removed in earlier pass
        | Term::Mat { .. } // Removed in earlier pass
        | Term::Nat { .. } // Removed in encode_nat
        | Term::Str { .. } // Removed in encode_str
        | Term::Lst { .. } // Removed in encode_list
        | Term::Err => unreachable!(),
      }
    })
  }

  fn encode_pat(&mut self, pat: &Pattern, up: Place<'t>) {
    maybe_grow(|| match pat {
      Pattern::Var(None) => self.link(up, Place::Tree(LoanedMut::new(Tree::Era))),
      Pattern::Var(Some(name)) => self.link_var(false, name, up),
      Pattern::Chn(name) => self.link_var(true, name, up),
      Pattern::Fan(fan, tag, els) => {
        let kind = CtrKind::Fan(*fan, tag.clone());
        self.make_node_list(kind, up, els.iter().map(|el| |slf: &mut Self, up| slf.encode_pat(el, up)));
      }
      Pattern::Ctr(_, _) | Pattern::Num(_) | Pattern::Lst(_) | Pattern::Str(_) | Pattern::Err => {
        unreachable!()
      }
    })
  }

  fn link(&mut self, a: Place<'t>, b: Place<'t>) {
    match (a, b) {
      (Place::Tree(a), Place::Tree(b)) => self.redexes.push((b, a).into()),
      (Place::Tree(t), Place::Hole(h)) | (Place::Hole(h), Place::Tree(t)) => {
        t.place(h);
      }
      (Place::Hole(a), Place::Hole(b)) => {
        let var = Tree::Var { nam: num_to_name(self.name_idx) };
        self.name_idx += 1;
        *a = var.clone();
        *b = var;
      }
      (Place::Wire(v), p) | (p, Place::Wire(v)) => {
        let v = &mut self.wires[v];
        match v.take() {
          Some(q) => self.link(p, q),
          None => *v = Some(p),
        }
      }
    }
  }

  fn new_ctr(&mut self, lab: u16) -> (Place<'t>, Place<'t>, Place<'t>) {
    self.created_nodes += 1;
    let (ports, node) = LoanedMut::loan_with(Tree::Ctr { lab, ports: vec![Tree::Era, Tree::Era] }, |t, l| {
      let Tree::Ctr { ports, .. } = t else { unreachable!() };
      l.loan_mut(ports)
    });
    let (a, b) = ports.split_at_mut(1);
    (Place::Tree(node), Place::Hole(&mut a[0]), Place::Hole(&mut b[0]))
  }

  /// Adds a list-like tree of nodes of the same kind to the inet.
  fn make_node_list(
    &mut self,
    kind: CtrKind,
    mut up: Place<'t>,
    mut els: impl DoubleEndedIterator<Item = impl FnOnce(&mut Self, Place<'t>)>,
  ) {
    let last = els.next_back().unwrap();
    let lab = self.labels.generate(&kind);
    for item in els {
      let node = self.new_ctr(lab);
      self.link(up, node.0);
      item(self, node.1);
      up = node.2;
    }
    last(self, up);
  }

  fn new_wire(&mut self) -> usize {
    let i = self.wires.len();
    self.wires.push(None);
    i
  }

  fn link_var(&mut self, global: bool, name: &Name, place: Place<'t>) {
    match self.vars.entry((global, name.clone())) {
      Entry::Occupied(e) => {
        let other = e.remove();
        self.link(place, other);
      }
      Entry::Vacant(e) => {
        e.insert(place);
      }
    }
  }
}

#[derive(Debug, Default, Clone)]
pub struct Labels {
  pub con: LabelGenerator,
  pub dup: LabelGenerator,
  pub tup: LabelGenerator,
}

impl Labels {
  pub fn generate(&mut self, kind: &CtrKind) -> u16 {
    match kind {
      CtrKind::Con(tag) => (self.con.generate(tag).map(|x| x + 1).unwrap_or(0)) << 2,
      CtrKind::Fan(FanKind::Tup, tag) => ((self.tup.generate(tag).map(|x| x + 1).unwrap_or(0)) << 2) | 0b01,
      CtrKind::Fan(FanKind::Dup, tag) => ((self.dup.generate(tag).unwrap()) << 2) | 0b10,
    }
  }

  pub fn to_ctr_kind(&self, lab: u16) -> CtrKind {
    match (lab >> 2, lab & 0b11) {
      (x, 0b00) => CtrKind::Con(self.con.to_tag(x.checked_sub(1))),
      (x, 0b01) => CtrKind::Fan(FanKind::Tup, self.tup.to_tag(x.checked_sub(1))),
      (x, 0b10) => CtrKind::Fan(FanKind::Dup, self.dup.to_tag(Some(x))),
      _ => unreachable!(),
    }
  }
}

#[derive(Debug, Default, Clone)]
pub struct LabelGenerator {
  pub next: u16,
  pub name_to_label: HashMap<Name, u16>,
  pub label_to_name: HashMap<u16, Name>,
}

impl Index<FanKind> for Labels {
  type Output = LabelGenerator;

  fn index(&self, fan: FanKind) -> &Self::Output {
    match fan {
      FanKind::Tup => &self.tup,
      FanKind::Dup => &self.dup,
    }
  }
}

impl IndexMut<FanKind> for Labels {
  fn index_mut(&mut self, fan: FanKind) -> &mut Self::Output {
    match fan {
      FanKind::Tup => &mut self.tup,
      FanKind::Dup => &mut self.dup,
    }
  }
}

impl LabelGenerator {
  // If some tag and new generate a new label, otherwise return the generated label.
  // If none use the implicit label counter.
  fn generate(&mut self, tag: &Tag) -> Option<u16> {
    let mut unique = || {
      let lab = self.next;
      self.next += 1;
      lab
    };
    match tag {
      Tag::Named(name) => match self.name_to_label.entry(name.clone()) {
        Entry::Occupied(e) => Some(*e.get()),
        Entry::Vacant(e) => {
          let lab = unique();
          self.label_to_name.insert(lab, name.clone());
          Some(*e.insert(lab))
        }
      },
      Tag::Numeric(lab) => Some(*lab),
      Tag::Auto => Some(unique()),
      Tag::Static => None,
    }
  }

  pub fn to_tag(&self, label: Option<u16>) -> Tag {
    match label {
      Some(label) => match self.label_to_name.get(&label) {
        Some(name) => Tag::Named(name.clone()),
        None => Tag::Numeric(label),
      },
      None => Tag::Static,
    }
  }

  fn finish(&mut self) {
    self.next = u16::MAX;
    self.name_to_label.clear();
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CtrKind {
  Con(Tag),
  Fan(FanKind, Tag),
}

fn hole<T: Default>() -> T {
  T::default()
}

impl ToStringVerbose for ViciousCycleErr {
  fn to_string_verbose(&self, _verbose: bool) -> String {
    "Found term that compiles into an inet with a vicious cycle".into()
  }
}
