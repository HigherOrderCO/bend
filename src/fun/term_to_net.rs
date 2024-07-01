use crate::{
  diagnostics::Diagnostics,
  fun::{num_to_name, Book, FanKind, Name, Op, Pattern, Term},
  hvm::{net_trees, tree_children},
  maybe_grow,
  net::CtrKind::{self, *},
};
use hvm::ast::{Net, Tree};
use loaned::LoanedMut;
use std::{
  collections::{hash_map::Entry, HashMap},
  ops::{Index, IndexMut},
};

#[derive(Debug, Clone)]
pub struct ViciousCycleErr;

pub fn book_to_hvm(book: &Book, diags: &mut Diagnostics) -> Result<(hvm::ast::Book, Labels), Diagnostics> {
  diags.start_pass();

  let mut hvm_book = hvm::ast::Book { defs: Default::default() };
  let mut labels = Labels::default();

  let main = book.entrypoint.as_ref().unwrap();

  for def in book.defs.values() {
    for rule in def.rules.iter() {
      let net = term_to_hvm(&rule.body, &mut labels);

      let name = if def.name == *main { book.hvm_entrypoint().to_string() } else { def.name.0.to_string() };

      match net {
        Ok(net) => {
          hvm_book.defs.insert(name, net);
        }
        Err(err) => diags.add_inet_error(err, name),
      }
    }
  }

  // TODO: native hvm nets ignore labels
  for def in book.hvm_defs.values() {
    hvm_book.defs.insert(def.name.to_string(), def.body.clone());
  }

  labels.con.finish();
  labels.dup.finish();

  diags.fatal((hvm_book, labels))
}

/// Converts an LC term into an IC net.
pub fn term_to_hvm(term: &Term, labels: &mut Labels) -> Result<Net, String> {
  let mut net = Net { root: Tree::Era, rbag: Default::default() };

  let mut state = EncodeTermState {
    lets: Default::default(),
    vars: Default::default(),
    wires: Default::default(),
    redexes: Default::default(),
    name_idx: 0,
    created_nodes: 0,
    labels,
  };

  state.encode_term(term, Place::Hole(&mut net.root));
  LoanedMut::from(std::mem::take(&mut state.redexes)).place(&mut net.rbag);

  let EncodeTermState { created_nodes, .. } = { state };

  let found_nodes = net_trees(&net).map(count_nodes).sum::<usize>();
  if created_nodes != found_nodes {
    return Err("Found term that compiles into an inet with a vicious cycle".into());
  }

  Ok(net)
}

#[derive(Debug)]
struct EncodeTermState<'t, 'l> {
  lets: Vec<(&'t Pattern, &'t Term)>,
  vars: HashMap<(bool, Name), Place<'t>>,
  wires: Vec<Option<Place<'t>>>,
  redexes: Vec<LoanedMut<'t, (bool, Tree, Tree)>>,
  name_idx: u64,
  created_nodes: usize,
  labels: &'l mut Labels,
}

fn count_nodes(tree: &Tree) -> usize {
  maybe_grow(|| {
    usize::from(tree_children(tree).next().is_some()) + tree_children(tree).map(count_nodes).sum::<usize>()
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
  fn encode_term(&mut self, term: &'t Term, up: Place<'t>) {
    maybe_grow(|| {
      match term {
        Term::Era => self.link(up, Place::Tree(LoanedMut::new(Tree::Era))),
        Term::Var { nam } => self.link_var(false, nam, up),
        Term::Link { nam } => self.link_var(true, nam, up),
        Term::Ref { nam } => self.link(up, Place::Tree(LoanedMut::new(Tree::Ref { nam: nam.to_string() }))),
        Term::Num { val } => {
          let val = hvm::ast::Numb(val.to_bits());
          self.link(up, Place::Tree(LoanedMut::new(Tree::Num { val })))
        }
        // A lambda becomes to a con node. Ports:
        // - 0: points to where the lambda occurs.
        // - 1: points to the lambda variable.
        // - 2: points to the lambda body.
        // core: (var_use bod)
        Term::Lam { tag, pat, bod } => {
          let kind = Con(self.labels.con.generate(tag));
          let node = self.new_ctr(kind);
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
          let kind = Con(self.labels.con.generate(tag));
          let node = self.new_ctr(kind);
          self.encode_term(fun, node.0);
          self.encode_term(arg, node.1);
          self.link(up, node.2);
        }
        // core: & arg ~ ?<(zero succ) ret>
        Term::Swt { arg, bnd, with_bnd, with_arg, pred, arms,  } => {
          // At this point should be only num matches of 0 and succ.
          assert!(bnd.is_none());
          assert!(with_bnd.is_empty());
          assert!(with_arg.is_empty());
          assert!(pred.is_none());
          assert!(arms.len() == 2);

          self.created_nodes += 2;
          let loaned = Tree::Swi { fst: Box::new(Tree::Con{fst: Box::new(Tree::Era), snd: Box::new(Tree::Era)}), snd: Box::new(Tree::Era)};
          let ((zero, succ, out), node) =
            LoanedMut::loan_with(loaned, |t, l| {
              let Tree::Swi { fst, snd: out } = t else { unreachable!() };
              let Tree::Con { fst:zero, snd: succ } = fst.as_mut() else { unreachable!() };
              (l.loan_mut(zero), l.loan_mut(succ), l.loan_mut(out))
            });

          self.encode_term(arg, Place::Tree(node));
          self.encode_term(&arms[0], Place::Hole(zero));
          self.encode_term(&arms[1], Place::Hole(succ));
          self.link(up, Place::Hole(out));
        }
        Term::Let { pat, val, nxt } => {
          // Dups/tup eliminators are not actually scoped like other terms.
          // They are depended on
          self.lets.push((pat, val));
          self.encode_term(nxt, up);
        }
        Term::Fan { fan, tag, els } => {
          let kind = self.fan_kind(fan, tag);
          self.make_node_list(kind, up, els.iter().map(|el| |slf: &mut Self, up| slf.encode_term(el, up)));
        }
        // core: & [opr] ~ $(fst $(snd ret))
        Term::Oper { opr, fst, snd } => {
          match (fst.as_ref(), snd.as_ref()) {
            // Partially apply with fst
            (Term::Num { val }, snd) => {
              let val = val.to_bits();
              let val = hvm::ast::Numb((val & !0x1F) | opr.to_native_tag() as u32);
              let fst = Place::Tree(LoanedMut::new(Tree::Num { val }));
              let node = self.new_opr();
              self.link(fst, node.0);
              self.encode_term(snd, node.1);
              self.encode_le_ge_opers(opr, up, node.2);
            }
            // Partially apply with snd, flip
            (fst, Term::Num { val }) => {
              if [Op::POW, Op::ATN, Op::LOG].contains(opr) {
                // POW, ATN and LOG share tags with AND, OR and XOR, so don't flip or results will be wrong
                let opr_val = hvm::ast::Numb(hvm::hvm::Numb::new_sym(opr.to_native_tag()).0);
                let oper = Place::Tree(LoanedMut::new(Tree::Num { val: opr_val }));
                let node1 = self.new_opr();
                self.encode_term(fst, node1.0);
                self.link(oper, node1.1);
                let node2 = self.new_opr();
                self.link(node1.2, node2.0);
                self.encode_term(snd, node2.1);
                self.encode_le_ge_opers(opr, up, node2.2);
              } else {
                // flip
                let val = val.to_bits();
                let val = hvm::ast::Numb((val & !0x1F) | flip_sym(opr.to_native_tag()) as u32);
                let snd = Place::Tree(LoanedMut::new(Tree::Num { val }));
                let node = self.new_opr();
                self.encode_term(fst, node.0);
                self.link(snd, node.1);
                self.encode_le_ge_opers(opr, up, node.2);
              }
            }
            // Don't partially apply
            (fst, snd) => {
              let opr_val = hvm::ast::Numb(hvm::hvm::Numb::new_sym(opr.to_native_tag()).0);
              let oper = Place::Tree(LoanedMut::new(Tree::Num { val: opr_val }));
              let node1 = self.new_opr();
              self.encode_term(fst, node1.0);
              self.link(oper, node1.1);
              let node2 = self.new_opr();
              self.link(node1.2, node2.0);
              self.encode_term(snd, node2.1);
              self.encode_le_ge_opers(opr, up, node2.2);
            }
          }
        }
        Term::Use { .. }  // Removed in earlier pass
        | Term::With { .. } // Removed in earlier pass
        | Term::Ask { .. } // Removed in earlier pass
        | Term::Mat { .. } // Removed in earlier pass
        | Term::Bend { .. } // Removed in desugar_bend
        | Term::Fold { .. } // Removed in desugar_fold
        | Term::Open { .. } // Removed in desugar_open
        | Term::Nat { .. } // Removed in encode_nat
        | Term::Str { .. } // Removed in encode_str
        | Term::List { .. } // Removed in encode_list
        | Term::Def { .. } // Removed in earlier pass
        | Term::Err => unreachable!(),
      }
      while let Some((pat, val)) = self.lets.pop() {
        let wire = self.new_wire();
        self.encode_term(val, Place::Wire(wire));
        self.encode_pat(pat, Place::Wire(wire));
      }
    })
  }

  fn encode_le_ge_opers(&mut self, opr: &Op, up: Place<'t>, node: Place<'t>) {
    match opr {
      Op::LE | Op::GE => {
        let node_eq = self.new_opr();
        let eq_val =
          Place::Tree(LoanedMut::new(Tree::Num { val: hvm::ast::Numb(Op::EQ.to_native_tag() as u32) }));
        self.link(eq_val, node_eq.0);
        self.link(node_eq.1, node);
        self.link(up, node_eq.2);
      }
      _ => self.link(up, node),
    }
  }

  fn encode_pat(&mut self, pat: &Pattern, up: Place<'t>) {
    maybe_grow(|| match pat {
      Pattern::Var(None) => self.link(up, Place::Tree(LoanedMut::new(Tree::Era))),
      Pattern::Var(Some(name)) => self.link_var(false, name, up),
      Pattern::Chn(name) => self.link_var(true, name, up),
      Pattern::Fan(fan, tag, els) => {
        let kind = self.fan_kind(fan, tag);
        self.make_node_list(kind, up, els.iter().map(|el| |slf: &mut Self, up| slf.encode_pat(el, up)));
      }
      Pattern::Ctr(_, _) | Pattern::Num(_) | Pattern::Lst(_) | Pattern::Str(_) => unreachable!(),
    })
  }

  fn link(&mut self, a: Place<'t>, b: Place<'t>) {
    match (a, b) {
      (Place::Tree(a), Place::Tree(b)) => {
        self.redexes.push(LoanedMut::merge((false, Tree::Era, Tree::Era), |r, m| {
          m.place(b, &mut r.1);
          m.place(a, &mut r.2);
        }))
      }
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

  fn new_ctr(&mut self, kind: CtrKind) -> (Place<'t>, Place<'t>, Place<'t>) {
    self.created_nodes += 1;
    let node = match kind {
      CtrKind::Con(None) => Tree::Con { fst: Box::new(Tree::Era), snd: Box::new(Tree::Era) },
      CtrKind::Dup(0) => Tree::Dup { fst: Box::new(Tree::Era), snd: Box::new(Tree::Era) },
      CtrKind::Tup(None) => Tree::Con { fst: Box::new(Tree::Era), snd: Box::new(Tree::Era) },
      _ => unreachable!(),
    };
    let ((a, b), node) = LoanedMut::loan_with(node, |t, l| match t {
      Tree::Con { fst, snd } => (l.loan_mut(fst), l.loan_mut(snd)),
      Tree::Dup { fst, snd } => (l.loan_mut(fst), l.loan_mut(snd)),
      _ => unreachable!(),
    });
    (Place::Tree(node), Place::Hole(a), Place::Hole(b))
  }

  fn new_opr(&mut self) -> (Place<'t>, Place<'t>, Place<'t>) {
    self.created_nodes += 1;
    let ((fst, snd), node) =
      LoanedMut::loan_with(Tree::Opr { fst: Box::new(Tree::Era), snd: Box::new(Tree::Era) }, |t, l| {
        let Tree::Opr { fst, snd } = t else { unreachable!() };
        (l.loan_mut(fst), l.loan_mut(snd))
      });
    (Place::Tree(node), Place::Hole(fst), Place::Hole(snd))
  }

  /// Adds a list-like tree of nodes of the same kind to the inet.
  fn make_node_list(
    &mut self,
    kind: CtrKind,
    mut up: Place<'t>,
    mut els: impl DoubleEndedIterator<Item = impl FnOnce(&mut Self, Place<'t>)>,
  ) {
    let last = els.next_back().unwrap();
    for item in els {
      let node = self.new_ctr(kind);
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

  fn fan_kind(&mut self, fan: &FanKind, tag: &crate::fun::Tag) -> CtrKind {
    let lab = self.labels[*fan].generate(tag);
    if *fan == FanKind::Tup {
      Tup(lab)
    } else {
      Dup(lab.unwrap())
    }
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
  fn generate(&mut self, tag: &crate::fun::Tag) -> Option<u16> {
    use crate::fun::Tag;
    match tag {
      Tag::Named(_name) => {
        todo!("Named tags not implemented for hvm32");
        /* match self.name_to_label.entry(name.clone()) {
          Entry::Occupied(e) => Some(*e.get()),
          Entry::Vacant(e) => {
            let lab = unique();
            self.label_to_name.insert(lab, name.clone());
            Some(*e.insert(lab))
          }
        } */
      }
      Tag::Numeric(lab) => Some(*lab),
      Tag::Auto => Some(0),
      Tag::Static => None,
    }
  }

  pub fn to_tag(&self, label: Option<u16>) -> crate::fun::Tag {
    use crate::fun::Tag;
    match label {
      Some(label) => match self.label_to_name.get(&label) {
        Some(name) => Tag::Named(name.clone()),
        None => {
          if label == 0 {
            Tag::Auto
          } else {
            Tag::Numeric(label)
          }
        }
      },
      None => Tag::Static,
    }
  }

  fn finish(&mut self) {
    self.next = u16::MAX;
    self.name_to_label.clear();
  }
}

impl Op {
  fn to_native_tag(self) -> hvm::hvm::Tag {
    match self {
      Op::ADD => hvm::hvm::OP_ADD,
      Op::SUB => hvm::hvm::OP_SUB,
      Op::MUL => hvm::hvm::OP_MUL,
      Op::DIV => hvm::hvm::OP_DIV,
      Op::REM => hvm::hvm::OP_REM,
      Op::EQ => hvm::hvm::OP_EQ,
      Op::NEQ => hvm::hvm::OP_NEQ,
      Op::LT => hvm::hvm::OP_LT,
      Op::GT => hvm::hvm::OP_GT,
      Op::AND => hvm::hvm::OP_AND,
      Op::OR => hvm::hvm::OP_OR,
      Op::XOR => hvm::hvm::OP_XOR,
      Op::SHL => hvm::hvm::OP_SHL,
      Op::SHR => hvm::hvm::OP_SHR,

      Op::ATN => hvm::hvm::OP_AND,
      Op::LOG => hvm::hvm::OP_OR,
      Op::POW => hvm::hvm::OP_XOR,

      Op::LE => hvm::hvm::OP_GT,
      Op::GE => hvm::hvm::OP_LT,
    }
  }
}

fn flip_sym(tag: hvm::hvm::Tag) -> hvm::hvm::Tag {
  match tag {
    hvm::hvm::OP_SUB => hvm::hvm::FP_SUB,
    hvm::hvm::FP_SUB => hvm::hvm::OP_SUB,
    hvm::hvm::OP_DIV => hvm::hvm::FP_DIV,
    hvm::hvm::FP_DIV => hvm::hvm::OP_DIV,
    hvm::hvm::OP_REM => hvm::hvm::FP_REM,
    hvm::hvm::FP_REM => hvm::hvm::OP_REM,
    hvm::hvm::OP_LT => hvm::hvm::OP_GT,
    hvm::hvm::OP_GT => hvm::hvm::OP_LT,
    hvm::hvm::OP_SHL => hvm::hvm::FP_SHL,
    hvm::hvm::FP_SHL => hvm::hvm::OP_SHL,
    hvm::hvm::OP_SHR => hvm::hvm::FP_SHR,
    hvm::hvm::FP_SHR => hvm::hvm::OP_SHR,
    _ => tag,
  }
}
