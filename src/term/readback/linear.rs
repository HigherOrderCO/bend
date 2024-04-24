//! Linear readback, directly from interaction combinator trees.
//!
//! ## The Algorithm
//!
//! To explain the algorithm, let's look at a simple case, the interaction net
//! `((123 x) x)`:
//!
//! ```text
//!        |   ((123 x) x)
//!       / \
//!      /   \ <---- node 0
//!     /_____\
//!      |   |
//!     /    |
//!    |     |
//!   / \    |
//!  /   \ <-|------ node 1
//! /_____\  |
//!  |   |   |
//! 123  |   |
//!       \_/
//!        x
//! ```
//!
//! This corresponds to the term `@f (f 123)`.
//!
//! Note that:
//! - node 0 and node 1 are both CON nodes, but node 0 corresponds to a lambda
//!   term whilst node 1 corresponds to an application node
//! - in the net, node 1 is the left child of node 0, but in the term, it is the
//!   right child
//!
//! Also, because of how the nodes are stored, when traversing the net, we can
//! only enter nodes through their principal ports (from tree parent to child),
//! so the readback algorithm must be structured as a series of top-down
//! traversals of the trees in the net.
//!
//! To read the net back properly, we first need to be able to distinguish
//! between applications and lambdas.
//!
//! ### Distinguishing lambdas and applications
//!
//! To distinguishing lambdas and applications, we need to determine the
//! *polarity* of each port in the net. The polarity of a port is either `+` or
//! `-`, and `+` ports can only be connected to `-` ports (and vice versa).
//!
//! Semantically, positive ports represent the production of a value, and
//! negative ports represent the consumption of a value.
//!
//! For this interaction system, there are a few more rules for polarity:
//! - the polarity of a free port is always `-`
//! - the polarity of the ports of a CON node is either `+(+ -)` or `-(- +)`
//!   (that is, port 0 and port 1 have the same polarity, and port 2 has the
//!   opposite)
//!
//! Using this information, we can completely label the above diagram with a
//! simple top-down recursive pass:
//!
//! ```text
//!        -
//!        |
//!        +
//!       / \
//!      /   \ <---- node 0
//!     /_____\
//!      +   -
//!     /    |
//!    -     |
//!   / \    |
//!  /   \ <-|------ node 1
//! /_____\  |
//!  -   +   |
//!  |   |   |
//!  +   |   |
//! 123  |   |
//!       \_/
//!        x
//! ```
//!
//! In fact, in general, if you know the polarity of one port of a tree, you can
//! trivially determine the polarity of every other port.
//!
//! The difficulty, then, comes when reading interaction nets with active pairs:
//!
//! ```text
//!                    redex 0
//!              /--------'--------\
//!   tree 0      tree 1    tree 2
//! /----'----\   /--'--\   /--'--\
//!        |           _______      ((a b) b)
//!        |          /       \     & (c c) ~ (123 a)
//!        |         |         |
//!       / \       / \       / \
//!      /   \     /   \     /   \
//!     /_____\   /_____\   /_____\
//!      |   |     |   |     |   |
//!     /    |      \_/     123  |
//!    |     |       c           |
//!   / \    |                   |
//!  /   \   |                   |
//! /_____\  |                   |
//!  |   |   |                   |
//!  |    \_/                    |
//!  |     b                     |
//!   \_________________________/
//!                a
//! ```
//!
//! If we were to simply iterate over the trees here, we would run into a
//! problem at tree 1 -- we have no way of knowing the polarity of any of its
//! ports. We would have to first label tree 0, then tree 2, before being able
//! to label tree 1.
//!
//! Thus, when computing the polarities, we support propagating polarity
//! *variables*. Specifically, we create a polarity variable for each active
//! pair, and then traverse the tree, performing a process of polarity
//! inference/unification.
//!
//! For example:
//!
//! Start by labelling tree 0, since we know the polarity of the free port:
//! ```text
//!        -           _______      ((a b) b)
//!        |          /       \     & (c c) ~ (123 a)
//!        +         |         |
//!       / \       / \       / \
//!      /   \     /   \     /   \
//!     /_____\   /_____\   /_____\
//!      +   -     |   |     |   |
//!     /    |      \_/     123  |
//!    -     |       c           |
//!   / \    |                   |
//!  /   \   |                   |
//! /_____\  |                   |
//!  -   +   |                   |
//!  |    \_/                    |
//!  |     b                     |
//!   \_________________________/
//!                a
//! ```
//!
//! Introduce the polarity variable `p`, and label tree 1 (P is the opposite
//! polarity of p):
//! ```text
//!        -           _______      ((a b) b)
//!        |          /       \     & (c c) ~ (123 a)
//!        +         p         |
//!       / \       / \       / \
//!      /   \     /   \     /   \
//!     /_____\   /_____\   /_____\
//!      +   -     p   P     |   |
//!     /    |      \_/     123  |
//!    -     |       c           |
//!   / \    |                   |
//!  /   \   |                   |
//! /_____\  |                   |
//!  -   +   |                   |
//!  |    \_/                    |
//!  |     b                     |
//!   \_________________________/
//!                a
//!
//! Now label tree 2:
//! ```text
//!        -           _______      ((a b) b)
//!        |          /       \     & (c c) ~ (123 a)
//!        +         p         P
//!       / \       / \       / \
//!      /   \     /   \     /   \
//!     /_____\   /_____\   /_____\
//!      +   -     p   P     P   p
//!     /    |      \_/      |   |
//!    -     |       c       p   |
//!   / \    |              123  |
//!  /   \   |                   |
//! /_____\  |                   |
//!  -   +   |                   |
//!  |    \_/                    |
//!  |     b                     |
//!   \_________________________/
//!                a
//! ```
//!
//! Note that we now have a connection between a `-` port and a `p` port. This
//! means that `p = +`, so we can update our diagram:
//! ```text
//!        -           _______      ((a b) b)
//!        |          /       \     & (c c) ~ (123 a)
//!        +         +         -
//!       / \       / \       / \
//!      /   \     /   \     /   \
//!     /_____\   /_____\   /_____\
//!      +   -     +   -     -   +
//!     /    |      \_/      |   |
//!    -     |       c       +   |
//!   / \    |              123  |
//!  /   \   |                   |
//! /_____\  |                   |
//!  -   +   |                   |
//!  |    \_/                    |
//!  |     b                     |
//!   \_________________________/
//!                a
//! ```
//!
//! And now our graph is fully labeled.
//!
//! When the net is fully connected, this algorithm can successfully label all
//! of the ports of the net.
//!
//! When there are disconnected subnets, however, there may be multiple valid
//! solutions. If the polarity can not be unambiguously determined, the readback
//! algorithm will skip the problematic subnets and issue a warning.
//!
//! In many cases, though, the polarity of disconnected subnets *can* be
//! determined, since our interaction system has nodes that can only be read
//! back as one polarity (e.g. reference nodes are always positive).
//!
//! ### Labeling ports with AST nodes/holes
//!
//! Now that we know the polarities of every port, we need to label each port
//! with either an AST node (either a term or a pattern), or a hole that must be
//! filled with an AST node (either a term hole or a pattern hole).
//!
//! Ports with polarity `+` will either get a term or a pattern hole, and ports
//! with polarity `-` will either get a term hole or a pattern.
//!
//! The labels of connected ports will then be linked as follows:
//! - linking a term with a term hole or a pattern with a pattern hole simply
//!   fills the hole with the node
//! - linking a term with a pattern creates a `let pattern = term` statement
//! - linking a term hole with a pattern hole fills each with a new variable
//!
//! Going back to our simpler example (which, if you recall, corresponds to the
//! term `@f (f 123)`):
//!
//! ```text
//!        |   ((123 x) x)
//!       / \
//!      /   \ <---- node 0
//!     /_____\
//!      |   |
//!     /    |
//!    |     |
//!   / \    |
//!  /   \ <-|------ node 1
//! /_____\  |
//!  |   |   |
//! 123  |   |
//!       \_/
//!        x
//! ```
//!
//! We traverse the tree as follows:
//! - visit node 0
//!   - port 0 has `+` polarity, so this is a lambda
//!   - we create a term with two holes, `@?p ?t`, and label the ports thusly:
//!     - port 0: `@?p ?t` (`+`, term)
//!     - port 1: `?p` (`+`, pattern hole)
//!     - port 2: `?t` (`-`, term hole)
//!   - this is the root node, so `@?p ?t` is the root term
//!   - visit the left child, node 1 (via port 1, `?p`)
//!     - port 0 has `-` polarity, so this is an application:
//!       - port 0: `?u` (`-`, term hole)
//!       - port 1: `?v` (`-`, term hole)
//!       - port 2: `(?u ?v)` (`+`, term)
//!     - we link port 0, `?u`, to the uplink `?p` (pattern hole)
//!       - these are both holes so we create a new variable, `a`, and fill them
//!         both with it
//!     - visit the left child, `123` (via port 1, `?v`)
//!       - this is a number node:
//!         - port 0: `123` (`+`, term)
//!       - we link port 0, `123`, to the uplink `?v`
//!         - we simply fill the term hole `?v` with the term `123`
//!     - visit the right child, `x` (via port 2, which is now `(a 123)`)
//!       - this is `x`, an aux-aux wire; add ``("x", `(a 123)`)`` to a hashmap
//!   - visit the right child, `x` (via port 2, `?t`)
//!       - this is `x`; remove ``("x", `(a 123)`)`` from the hashmap
//!       - link `(a 123)` to `?t`
//!         - we simply fill the term hole `?t` with the term `(a 123)`
//! - we're finished; the root term is now `@a (a 123)`
//!
//! ## Implementation
//!
//! To implement this algorithm, we need to create a system where both AST nodes
//! and AST *holes* can be transferred. For this, we use the `loaned` crate,
//! which supplies a `LoanedMut<'t, T>` type that allows transferring values
//! with live interior mutable references.
//!
//! Thus, we will associate a `Target<'t>` with every port, where a target can
//! be one of:
//! - `LoanedMut<'t, Term>` (a term)
//! - `&'t mut Term` (a term hole)
//! - `LoanedMut<'t, Pattern>` (a pattern)
//! - `&'t mut Pattern` (a pattern hole)

use std::{
  collections::{hash_map::Entry, HashMap},
  ops::{BitXor, Not},
};

use hvmc::ast::{Net, Tree};
use loaned::{drop, LoanedMut};

use crate::{
  diagnostics::{DiagnosticOrigin, Diagnostics, Severity},
  term::{
    encoding::{CtrKind, Labels},
    num_to_name,
    readback::normalize_vars::normalize_vars,
    Book, Name, Pattern, Term,
  },
};

pub fn readback_linear(net: &Net, book: &Book, labels: &Labels, info: &mut Diagnostics) -> Term {
  let mut root = hole();

  let mut readback = Readback {
    polarity_vars: Default::default(),
    vars: Default::default(),
    lets: Some(&mut root),
    labels,
    name_idx: 0,
    root: None,
    garbage_terms: vec![],
    garbage_pats: vec![],
    errors: vec![],
  };

  readback.readback(net);

  readback.report_errors(info);

  let Readback { garbage_terms, garbage_pats, .. } = { readback };
  drop!(LoanedMut::<Vec<Term>>::from(garbage_terms));
  drop!(LoanedMut::<Vec<Pattern>>::from(garbage_pats));

  normalize_vars(book, &mut root);

  root
}

struct Readback<'c, 't, 'n> {
  /// The value of all our polarity variables, if known yet.
  polarity_vars: Vec<Option<Polarity>>,

  /// A map from hvm-core variable names to either `Target`s or `Polar`:
  /// - `Ok`: a definite `Target`
  /// - `Err`: only a `Polarity`
  ///
  /// After polarity inference, `Err`s are treated equivalently to missing entries.
  vars: HashMap<&'n str, Result<Target<'t>, Polarity>>,

  /// Where to insert let statements; optional only so it can be temporarily
  /// moved; it's generally safe to `.unwrap()`.
  lets: Option<&'t mut Term>,

  // Dropping `LoanedMut`s panic, so we push them to these vectors instead, so
  // that we can dispose of the data once `'t` expires.
  garbage_terms: Vec<LoanedMut<'t, Term>>,
  garbage_pats: Vec<LoanedMut<'t, Pattern>>,

  root: Option<LoanedMut<'t, Term>>,
  errors: Vec<ReadbackError>,
  labels: &'c Labels,
  name_idx: u64,
}

macro_rules! sym {
  ($a:pat, $b:pat) => {
    ($a, $b) | ($b, $a)
  };
}

impl<'c, 't, 'n> Readback<'c, 't, 'n> {
  fn readback(&mut self, net: &'n Net) {
    // Theoretically, first, we want to infer the polarities for the root tree.
    // However, since we definitely know the polarity of the free port, it' more
    // efficient to skip that step and go straight to reading it back.
    self.read_pos(&net.root, NegRoot);

    // Create one polarity variable for each redex
    self.polarity_vars.resize(net.redexes.len(), None);

    // Infer polarities for every redex
    for (i, (l, r)) in net.redexes.iter().enumerate() {
      let p = self.infer_polarity(l, Polarity::Var(false, i));
      self.infer_polarity(r, !p);
    }

    // Now that all of the polarities are known (or as known as possible), we
    // can now read the redexes back:
    for (i, (lft, rgt)) in net.redexes.iter().enumerate() {
      // If we still don't know the polarity of the redex, there must be a
      // disconnected subnet with too little information to readback. Give a
      // warning and move on.
      let Some(p) = self.polarity_vars[i] else {
        self.error(ReadbackError::DisconnectedSubnet);
        continue;
      };
      if !p.is_concrete() {
        self.error(ReadbackError::DisconnectedSubnet);
        continue;
      }
      let (pos, neg) = if p == Polarity::Pos { (lft, rgt) } else { (rgt, lft) };
      let t = self._read_pos(pos);
      self.read_neg(neg, t);
    }

    // We've finished creating lets, so we place the root at the bottom of the
    // let chain.
    self.root.take().unwrap_or_default().place(self.lets.take().unwrap());

    // If we have any entries remaining in the hashmap, issue warnings and push
    // their targets to the garbage vectors.
    for target in std::mem::take(&mut self.vars).into_iter().filter_map(|x| x.1.ok()) {
      self.error(ReadbackError::UnboundVar);
      self.trash(target);
    }
  }

  /// Infer polarity values for each port in this tree, based on a polarity
  /// value for the root (which may be a variable), unifying with the rest of
  /// the information we've recorded so far.
  ///
  /// Returns the polarity of the root of this tree, potentially further refined
  /// due to unification.
  ///
  /// We treat numbers and reference nodes as always positive, and match and
  /// operation nodes as always negative. It is possible to have
  /// opposite-polarity versions of these in certain programs, but we have no
  /// good way to read them back, and treating them as constant polarity
  /// increases the number of disconnected subnets we can read back.
  fn infer_polarity(&mut self, tree: &'n Tree, mut polarity: Polarity) -> Polarity {
    match tree {
      // Erasers can be positive or negative.
      Tree::Era => polarity,
      Tree::Num { .. } | Tree::Ref { .. } => self.unify_polarities(polarity, Polarity::Pos),
      Tree::Op { rhs, out, .. } => {
        polarity = self.unify_polarities(polarity, Polarity::Neg);
        self.infer_polarity(rhs, Polarity::Pos);
        self.infer_polarity(out, Polarity::Neg);
        polarity
      }
      Tree::Mat { zero, succ, out } => {
        polarity = self.unify_polarities(polarity, Polarity::Neg);
        self.infer_polarity(zero, Polarity::Pos);
        self.infer_polarity(succ, Polarity::Pos);
        self.infer_polarity(out, Polarity::Neg);
        polarity
      }
      Tree::Ctr { lab, ports } => {
        let invert_first = matches!(self.labels.to_ctr_kind(*lab), CtrKind::Con(_));
        for (i, port) in ports.iter().enumerate() {
          let invert = invert_first && (i != ports.len() - 1);
          polarity = self.infer_polarity(port, polarity ^ invert) ^ invert;
        }
        polarity
      }
      Tree::Var { nam } => match self.vars.entry(nam) {
        Entry::Occupied(e) => {
          let other_polarity = match e.get() {
            Ok(t) => !t.polarity(),
            Err(p) => *p,
          };
          self.unify_polarities(polarity, !other_polarity)
        }
        Entry::Vacant(e) => {
          e.insert(Err(polarity));
          polarity
        }
      },
      Tree::Adt { .. } => unimplemented!(),
    }
  }

  /// Update the values of our polarity variables based on an equation between
  /// two polarities.
  fn unify_polarities(&mut self, a: Polarity, b: Polarity) -> Polarity {
    match (a, b) {
      _ if a == b => a, // already equivalent, no-op
      sym!(Polarity::Var(inv, id), p) => {
        let val = &mut self.polarity_vars[id];
        if let Some(q) = *val {
          self.unify_polarities(p, q ^ inv)
        } else {
          *val = Some(p ^ inv);
          p
        }
      }
      _ => {
        self.error(ReadbackError::InvalidPolarity);
        a
      }
    }
  }

  /// Given a negative-polarity uplink, read back from a `Tree` with a positive
  /// root port.
  fn read_pos(&mut self, tree: &'n Tree, up: Target<'t>) {
    match tree {
      Tree::Var { nam } => self.link_var(nam, up),
      _ => {
        let down = self._read_pos(tree);
        self.link(up, down);
      }
    }
  }

  /// Read back from a `Tree` with a positive root port, returning a downlink.
  ///
  /// Panics if this is a variable (as a `Target` may not be available for it
  /// yet).
  fn _read_pos(&mut self, tree: &'n Tree) -> Target<'t> {
    match tree {
      Tree::Var { .. } => unreachable!(),
      Tree::Era => PosTerm(LoanedMut::new(Term::Era)),
      Tree::Num { val } => PosTerm(LoanedMut::new(Term::Num { val: *val as u64 & ((1 << 60) - 1) })),
      Tree::Ref { nam } => PosTerm(LoanedMut::new(Term::Ref { nam: Name::new(nam) })),
      Tree::Ctr { lab, ports } => match self.labels.to_ctr_kind(*lab) {
        // lambda
        CtrKind::Con(tag) => {
          let [fst, snd] = &ports[..] else { unimplemented!() };
          let ((pat, bod), ret) =
            LoanedMut::loan_with(Term::Lam { tag, pat: hole(), bod: hole() }, |node, l| {
              let Term::Lam { pat, bod, .. } = node else { unreachable!() };
              (l.loan_mut(pat), l.loan_mut(bod))
            });
          let bod = self.collect_lets(bod, |slf| {
            slf.read_neg(fst, PosPat(pat));
          });
          self.read_pos(snd, NegTerm(bod));
          PosTerm(ret)
        }
        // tuple / superposition
        CtrKind::Fan(fan, tag) => {
          let (els, ret) =
            LoanedMut::loan_with(Term::Fan { fan, tag, els: vec![hole(); ports.len()] }, |node, l| {
              let Term::Fan { els, .. } = node else { unreachable!() };
              l.loan_mut(els)
            });
          for (port, el) in ports.iter().zip(els.iter_mut()) {
            self.read_pos(port, NegTerm(el));
          }
          PosTerm(ret)
        }
      },
      Tree::Op { .. } | Tree::Mat { .. } => {
        self.error(ReadbackError::InvalidPolarity);
        PosTerm(hole())
      }
      Tree::Adt { .. } => unimplemented!(),
    }
  }

  /// Given a positive-polarity uplink, read back from a `Tree` with a negative
  /// root port.
  fn read_neg(&mut self, tree: &'n Tree, up: Target<'t>) {
    match tree {
      Tree::Var { nam } => self.link_var(nam, up),
      Tree::Era => self.link(up, NegPat(LoanedMut::new(Pattern::Var(None)))),
      Tree::Ctr { lab, ports } => match self.labels.to_ctr_kind(*lab) {
        // application
        CtrKind::Con(tag) => {
          let [fst, snd] = &ports[..] else { unimplemented!() };
          let ((fun, arg), ret) =
            LoanedMut::loan_with(Term::App { tag, fun: hole(), arg: hole() }, |node, l| {
              let Term::App { fun, arg, .. } = node else { unreachable!() };
              (l.loan_mut(fun), l.loan_mut(arg))
            });
          self.link(up, NegTerm(fun));
          self.read_pos(fst, NegTerm(arg));
          self.read_neg(snd, PosTerm(ret));
        }
        // tuple destructure / duplication
        CtrKind::Fan(fan, tag) => {
          let (els, ret) =
            LoanedMut::loan_with(Pattern::Fan(fan, tag, vec![hole(); ports.len()]), |node, l| {
              let Pattern::Fan(.., els) = node else { unreachable!() };
              l.loan_mut(els)
            });
          self.link(up, NegPat(ret));
          for (port, el) in ports.iter().zip(els.iter_mut()) {
            self.read_neg(port, PosPat(el));
          }
        }
      },
      Tree::Op { op, rhs, out } => {
        let ((fst, snd), ret) =
          LoanedMut::loan_with(Term::Opx { opr: *op, fst: hole(), snd: hole() }, |node, l| {
            let Term::Opx { fst, snd, .. } = node else { unreachable!() };
            (l.loan_mut(fst), l.loan_mut(snd))
          });
        self.link(up, NegTerm(fst));
        self.read_pos(rhs, NegTerm(snd));
        self.read_neg(out, PosTerm(ret));
      }
      Tree::Mat { zero, succ, out } => {
        let name = self.gen_name();
        let pred_var = Name::new(format!("{name}-1"));
        let ((arg, zer, suc), swt) = LoanedMut::loan_with(
          Term::Swt {
            arg: hole(),
            bnd: Some(name.clone()),
            with: vec![],
            pred: Some(pred_var.clone()),
            arms: vec![hole(), hole()],
          },
          |node, l| {
            let Term::Swt { arg, arms, .. } = node else { unreachable!() };
            let arg = l.loan_mut(arg);
            let arms = l.loan_mut(arms);
            let (a, b) = arms.split_at_mut(1);
            (arg, &mut a[0], &mut b[0])
          },
        );
        self.link(up, NegTerm(arg));
        self.read_pos(zero, NegTerm(zer));
        self.read_neg(out, PosTerm(swt));
        if let Tree::Ctr { lab: 0, ports } = &**succ
          && let [pred, succ] = &ports[..]
        {
          let suc = self.collect_lets(suc, |slf| {
            slf.read_neg(pred, PosTerm(LoanedMut::new(Term::Var { nam: pred_var })));
          });
          self.read_pos(succ, NegTerm(suc));
        } else {
          *suc = Term::app(hole(), Term::Var { nam: pred_var });
          let Term::App { fun, .. } = suc else { unreachable!() };
          self.read_pos(succ, NegTerm(fun));
        }
      }
      Tree::Num { .. } | Tree::Ref { .. } => {
        self.error(ReadbackError::InvalidPolarity);
        self.link(up, NegPat(hole()));
      }
      Tree::Adt { .. } => unimplemented!(),
    }
  }

  fn link_var(&mut self, var: &'n str, x: Target<'t>) {
    match self.vars.entry(var) {
      Entry::Occupied(mut e) => match *e.get_mut() {
        Ok(_) => {
          let y = e.remove().unwrap();
          self.link(x, y);
        }
        Err(p) => {
          let q = x.polarity();
          let _ = e.insert(Ok(x));
          self.unify_polarities(p, !q);
        }
      },
      Entry::Vacant(e) => {
        e.insert(Ok(x));
      }
    }
  }

  fn link(&mut self, a: Target<'t>, b: Target<'t>) {
    // report polarity errors
    self.unify_polarities(a.polarity(), !b.polarity());
    match (a, b) {
      sym!(PosTerm(a), NegTerm(b)) => a.place(b),
      sym!(PosPat(a), NegPat(b)) => b.place(a),
      sym!(PosPat(a), NegTerm(b)) => {
        let nam = self.gen_name();
        *a = Pattern::Var(Some(nam.clone()));
        *b = Term::Var { nam };
      }
      sym!(PosTerm(t), NegPat(p)) => self.insert_let(p, t),
      sym!(PosTerm(t), NegRoot) => self.root = Some(t),
      sym!(PosPat(p), NegRoot) => {
        let nam = self.gen_name();
        *p = Pattern::Var(Some(nam.clone()));
        self.root = Some(LoanedMut::new(Term::Var { nam }));
      }
      // push data from polarity errors to garbage:
      (a, b) => {
        self.trash(a);
        self.trash(b);
      }
    }
  }

  fn insert_let(&mut self, p: LoanedMut<'t, Pattern>, v: LoanedMut<'t, Term>) {
    let lets = self.lets.take().unwrap();
    *lets = Term::Let { pat: hole(), val: hole(), nxt: hole() };
    let Term::Let { pat, val, nxt } = lets else { unreachable!() };
    self.lets = Some(nxt);
    p.place(&mut **pat);
    v.place(&mut **val);
  }

  /// Insert all lets created in the callback into `into`, returning a hole at
  /// the base of the let chain.
  ///
  /// This is purely aesthetic; `f(); into` would be a valid implementation of
  /// this function.
  fn collect_lets(&mut self, into: &'t mut Term, f: impl FnOnce(&mut Self)) -> &'t mut Term {
    let old_lets = std::mem::replace(&mut self.lets, Some(into));
    f(self);
    std::mem::replace(&mut self.lets, old_lets).unwrap()
  }

  fn gen_name(&mut self) -> Name {
    let nam = Name::new(num_to_name(self.name_idx));
    self.name_idx += 1;
    nam
  }

  fn trash(&mut self, target: Target<'t>) {
    match target {
      PosTerm(t) => self.garbage_terms.push(t),
      NegPat(t) => self.garbage_pats.push(t),
      _ => {}
    }
  }

  pub fn error(&mut self, error: ReadbackError) {
    self.errors.push(error);
  }

  pub fn report_errors(&mut self, diagnostics: &mut Diagnostics) {
    let mut err_counts = std::collections::BTreeMap::new();
    for err in &self.errors {
      *err_counts.entry(*err).or_insert(0) += 1;
    }

    for (err, count) in err_counts {
      let count_msg = if count > 1 { format!(" ({count} occurrences)") } else { "".to_string() };
      let msg = format!("{}{}", err, count_msg);
      diagnostics.add_diagnostic(msg.as_str(), Severity::Warning, DiagnosticOrigin::Readback);
    }
  }
}

#[derive(Debug)]
enum Target<'t> {
  PosTerm(LoanedMut<'t, Term>),
  NegTerm(&'t mut Term),
  PosPat(&'t mut Pattern),
  NegPat(LoanedMut<'t, Pattern>),
  /// The root term is handled specially so that top-level lets are handled
  /// nicely.
  NegRoot,
}

use Target::*;

impl<'t> Target<'t> {
  fn polarity(&self) -> Polarity {
    match self {
      PosTerm(_) | PosPat(_) => Polarity::Pos,
      NegTerm(_) | NegPat(_) | NegRoot => Polarity::Neg,
    }
  }
}

/// A polarity value; positive represents the creation of a value and negative
/// represents the destruction.
///
/// All wires in the net must be between a positive and a negative port.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Polarity {
  Pos,
  Neg,
  /// The concrete value is not yet known, and is based on a polarity variable.
  ///
  /// The `bool` indicates if self polarity is negated from the variable.
  Var(bool, usize),
}

impl Polarity {
  fn is_concrete(&self) -> bool {
    !matches!(self, Polarity::Var(..))
  }
}

/// Negates a polarity.
impl Not for Polarity {
  type Output = Self;
  fn not(self) -> Self::Output {
    match self {
      Polarity::Pos => Polarity::Neg,
      Polarity::Neg => Polarity::Pos,
      Polarity::Var(inv, id) => Polarity::Var(!inv, id),
    }
  }
}

/// Negates a polarity if the bool is `true`.
impl BitXor<bool> for Polarity {
  type Output = Self;

  fn bitxor(self, rhs: bool) -> Self::Output {
    if rhs { !self } else { self }
  }
}

fn hole<T: Default>() -> T {
  T::default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReadbackError {
  UnboundVar,
  InvalidPolarity,
  DisconnectedSubnet,
}

impl std::fmt::Display for ReadbackError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ReadbackError::UnboundVar => write!(f, "Detected unbound variable"),
      ReadbackError::InvalidPolarity => write!(f, "Mismatched polarities"),
      ReadbackError::DisconnectedSubnet => write!(f, "Could not print disconnected subnet"),
    }
  }
}
