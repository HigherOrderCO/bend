use hvmc::ast::{Net, Tree};
use indexmap::IndexSet;

use crate::{
  diagnostics::{DiagnosticOrigin, Diagnostics, Severity},
  maybe_grow,
  term::{
    encoding::{
      CtrKind::{self, *},
      Labels,
    },
    num_to_name, Book, FanKind, Name, Pattern, Tag, Term,
  },
};
use std::{
  collections::{hash_map::Entry, HashMap, HashSet},
  hash::Hash,
  ops::Deref,
};

pub fn readback_non_linear(net: &Net, book: &Book, labels: &Labels, diagnostics: &mut Diagnostics) -> Term {
  let wires = &Wires::new(net);
  let mut reader = Reader {
    wires,
    labels,
    book,
    dup_paths: Default::default(),
    scope: Default::default(),
    seen_fans: Default::default(),
    namegen: Default::default(),
    errors: Default::default(),
  };

  let mut term = reader.read_term(wires.enter_port(ROOT));

  while let Some(node) = reader.scope.pop() {
    let val = reader.read_term(reader.wires.enter_port(Port(node, 0)));
    let fst = reader.namegen.decl_name(wires, Port(node, 1));
    let snd = reader.namegen.decl_name(wires, Port(node, 2));

    let Tree::Ctr { lab, .. } = *node else { unreachable!() };
    let CtrKind::Fan(fan, tag) = labels.to_ctr_kind(*lab) else { unreachable!() };

    let split = &mut Split { fan, tag, fst, snd, val };

    let uses = term.insert_split(split, usize::MAX).unwrap();
    let result = term.insert_split(split, uses);
    debug_assert_eq!(result, None);
  }

  reader.report_errors(diagnostics);

  let mut unscoped = HashSet::new();
  let mut scope = Vec::new();
  term.collect_unscoped(&mut unscoped, &mut scope);
  term.apply_unscoped(&unscoped);

  term
}

type Scope<'n> = IndexSet<NodeId<'n>>;

pub struct Reader<'a, 'n> {
  book: &'a Book,
  namegen: NameGen<'n>,
  wires: &'a Wires<'n>,
  labels: &'a Labels,
  dup_paths: HashMap<u16, Vec<SlotId>>,
  /// Store for floating/unscoped terms, like dups and let tups.
  scope: Scope<'n>,
  seen_fans: Scope<'n>,
  errors: Vec<ReadbackError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Port<'n>(NodeId<'n>, SlotId);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NodeId<'n>(&'n Tree);

impl<'n> Hash for NodeId<'n> {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    (self.0 as *const Tree).hash(state);
  }
}

impl<'n> Deref for NodeId<'n> {
  type Target = &'n Tree;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

type SlotId = usize;

impl<'n> Port<'n> {
  fn node(&self) -> NodeId<'n> {
    self.0
  }
  fn slot(&self) -> SlotId {
    self.1
  }
}

#[derive(Default)]
struct Wires<'n> {
  wires: HashMap<Port<'n>, Port<'n>>,
  vars: HashMap<&'n str, Port<'n>>,
}

static ROOT_NODE: Tree = Tree::Ref { nam: String::new() };
static ROOT: Port = Port(NodeId(&ROOT_NODE), 0);

impl<'n> Wires<'n> {
  fn new(net: &'n Net) -> Self {
    let mut inet = Wires::default();
    inet.visit_tree(&net.root, ROOT);
    for (a, b) in &net.redexes {
      inet.visit_pair(a, b);
    }
    inet
  }

  fn visit_pair(&mut self, a: &'n Tree, b: &'n Tree) {
    self.visit_tree(a, Port(NodeId(b), 0));
    self.visit_tree(b, Port(NodeId(a), 0));
  }

  fn visit_tree(&mut self, tree: &'n Tree, up: Port<'n>) {
    if let Tree::Var { nam } = tree {
      match self.vars.entry(nam) {
        Entry::Occupied(e) => {
          let down = e.remove();
          self.wires.insert(up, down);
          self.wires.insert(down, up);
        }
        Entry::Vacant(e) => {
          e.insert(up);
        }
      }
    } else {
      self.wires.insert(Port(NodeId(tree), 0), up);
      if up == ROOT {
        self.wires.insert(up, Port(NodeId(tree), 0));
      }
      for (i, child) in tree.children().enumerate() {
        self.visit_tree(child, Port(NodeId(tree), i + 1));
      }
    }
  }

  fn enter_port(&self, port: Port<'n>) -> Option<Port<'n>> {
    if port.1 != 0 {
      let child = port.0.children().nth(port.1 - 1).unwrap();
      if !matches!(child, Tree::Var { .. }) {
        return Some(Port(NodeId(child), 0));
      }
    }
    self.wires.get(&port).copied()
  }
}

impl<'n> Reader<'_, 'n> {
  fn read_term(&mut self, next: Option<Port<'n>>) -> Term {
    if next == Some(ROOT) {
      self.error(ReadbackError::ReachedRoot);
      return Term::Err;
    }
    let Some(next) = next else {
      self.error(ReadbackError::UnboundVar);
      return Term::Err;
    };
    maybe_grow(|| {
      let node = next.node();
      let term = match *node {
        Tree::Era => Term::Era,
        // If we're visiting a con node...
        Tree::Ctr { lab, .. } => match self.labels.to_ctr_kind(*lab) {
          Con(tag) => match next.slot() {
            // If we're visiting a port 0, then it is a lambda.
            0 => {
              let nam = self.namegen.decl_name(self.wires, Port(node, 1));
              let bod = self.read_term(self.wires.enter_port(Port(node, 2)));
              Term::Lam { tag, pat: Box::new(Pattern::Var(nam)), bod: Box::new(bod) }
            }
            // If we're visiting a port 1, then it is a variable.
            1 => Term::Var { nam: self.namegen.var_name(next) },
            // If we're visiting a port 2, then it is an application.
            2 => {
              let fun = self.read_term(self.wires.enter_port(Port(node, 0)));
              let arg = self.read_term(self.wires.enter_port(Port(node, 1)));
              Term::App { tag, fun: Box::new(fun), arg: Box::new(arg) }
            }
            _ => unreachable!(),
          },
          // If we're visiting a fan node...
          Fan(fan, tag) => match next.slot() {
            // If we're visiting a port 0, then it is a pair.
            0 => {
              if fan == FanKind::Dup
                && let Some(slot) = self.dup_paths.entry(*lab).or_default().pop()
              {
                // Since we had a paired Dup in the path to this Sup,
                // we "decay" the superposition according to the original direction we came from the Dup.
                let term = self.read_term(self.wires.enter_port(Port(node, slot)));
                self.dup_paths.get_mut(lab).unwrap().push(slot);
                term
              } else {
                // If no Dup with same label in the path, we can't resolve the Sup, so keep it as a term.
                self
                  .decay_or_get_ports(node)
                  .map_or_else(|(fst, snd)| Term::Fan { fan, tag, els: vec![fst, snd] }, |term| term)
              }
            }
            // If we're visiting a port 1 or 2, then it is a variable.
            // Also, that means we found a dup, so we store it to read later.
            1 | 2 => {
              if fan == FanKind::Dup {
                self.dup_paths.entry(*lab).or_default().push(next.slot());
                let term = self.read_term(self.wires.enter_port(Port(node, 0)));
                self.dup_paths.entry(*lab).or_default().pop().unwrap();
                term
              } else {
                if self.seen_fans.insert(node) {
                  self.scope.insert(node);
                }
                Term::Var { nam: self.namegen.var_name(next) }
              }
            }
            _ => unreachable!(),
          },
        },
        Tree::Mat { .. } => match next.slot() {
          3 => {
            // Read the matched expression
            let arg = self.read_term(self.wires.enter_port(Port(node, 0)));
            let bnd = if let Term::Var { nam } = &arg { nam.clone() } else { self.namegen.unique() };

            let (zero, succ) = {
              let zero_term = self.read_term(self.wires.enter_port(Port(node, 1)));
              let mut succ_term = self.read_term(self.wires.enter_port(Port(node, 2)));

              match &mut succ_term {
                Term::Lam { pat: box Pattern::Var(nam), bod, .. } => {
                  let mut bod = std::mem::take(bod.as_mut());
                  if let Some(nam) = &nam {
                    bod.subst(nam, &Term::Var { nam: Name::new(format!("{bnd}-1")) });
                  }
                  (zero_term, bod)
                }
                _ => {
                  self.error(ReadbackError::InvalidNumericMatch);
                  (zero_term, succ_term)
                }
              }
            };
            Term::Swt { arg: Box::new(arg), bnd: Some(bnd), with: vec![], pred: None, arms: vec![zero, succ] }
          }
          _ => {
            self.error(ReadbackError::InvalidNumericMatch);
            Term::Err
          }
        },
        Tree::Ref { nam } => {
          let nam = Name::new(nam);
          if nam.is_generated() {
            // Dereference generated names since the user is not aware of them
            let def = &self.book.defs[&nam];
            let mut term = def.rule().body.clone();
            term.fix_names(&mut self.namegen.id_counter, self.book);

            term
          } else {
            Term::Ref { nam: nam.clone() }
          }
        }
        Tree::Num { val } => Term::Num { val: (*val as u64) & ((1 << 60) - 1) },
        Tree::Op { op, .. } => match next.slot() {
          2 => {
            let fst = self.read_term(self.wires.enter_port(Port(node, 0)));
            let snd = self.read_term(self.wires.enter_port(Port(node, 1)));
            let (opr, fst, snd) = if is_op_swapped(*op) { (op.swap(), snd, fst) } else { (*op, fst, snd) };
            Term::Opx { opr, fst: Box::new(fst), snd: Box::new(snd) }
          }
          _ => {
            self.error(ReadbackError::InvalidNumericOp);
            Term::Err
          }
        },
        Tree::Adt { .. } => unimplemented!(),
        Tree::Var { .. } => unreachable!(),
      };

      term
    })
  }

  /// Enters both ports 1 and 2 of a node,
  /// Returning a Term if is possible to simplify the net, or the Terms on the two ports of the node.
  /// The two possible outcomes are always equivalent.
  ///
  /// If:
  ///  - The node Kind is CON/TUP/DUP
  ///  - Both ports 1 and 2 are connected to the same node on slots 1 and 2 respectively
  ///  - That node Kind is the same as the given node Kind
  ///
  /// Then:
  ///   Reads the port 0 of the connected node, and returns that term.
  ///
  /// Otherwise:
  ///   Returns the terms on ports 1 and 2 of the given node.
  ///
  /// # Example
  ///
  /// ```hvm
  /// // λa let (a, b) = a; (a, b)
  /// ([a b] [a b])
  ///
  /// // The node `(a, b)` is just a reconstruction of the destructuring of `a`,
  /// // So we can skip both steps and just return the "value" unchanged:
  ///
  /// // λa a
  /// (a a)
  /// ```
  ///
  fn decay_or_get_ports(&mut self, node: NodeId<'n>) -> Result<Term, (Term, Term)> {
    let fst_port = self.wires.enter_port(Port(node, 1));
    let snd_port = self.wires.enter_port(Port(node, 2));

    // Eta-reduce the readback inet.
    // This is not valid for all kinds of nodes, only CON/TUP/DUP, due to their interaction rules.
    if let Tree::Ctr { lab, .. } = *node {
      match (fst_port, snd_port) {
        (Some(Port(fst_node, 1)), Some(Port(snd_node, 2))) if fst_node == snd_node => {
          if let Tree::Ctr { lab: other_lab, .. } = *fst_node
            && lab == other_lab
          {
            self.scope.shift_remove(&fst_node);

            let port_zero = self.wires.enter_port(Port(fst_node, 0));
            let term = self.read_term(port_zero);
            return Ok(term);
          }
        }
        _ => {}
      }
    }

    let fst = self.read_term(fst_port);
    let snd = self.read_term(snd_port);
    Err((fst, snd))
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

/// Represents `let #tag(fst, snd) = val` / `let #tag{fst snd} = val`
struct Split {
  fan: FanKind,
  tag: Tag,
  fst: Option<Name>,
  snd: Option<Name>,
  val: Term,
}

impl Default for Split {
  fn default() -> Self {
    Self {
      fan: FanKind::Dup,
      tag: Default::default(),
      fst: Default::default(),
      snd: Default::default(),
      val: Default::default(),
    }
  }
}

impl Term {
  /// Calculates the number of times `fst` and `snd` appear in this term. If
  /// that is `>= threshold`, it inserts the split at this term, and returns
  /// `None`. Otherwise, returns `Some(uses)`.
  ///
  /// This is only really useful when called in two passes – first, with
  /// `threshold = usize::MAX`, to count the number of uses, and then with
  /// `threshold = uses`.
  ///
  /// This has the effect of inserting the split at the lowest common ancestor
  /// of all of the uses of `fst` and `snd`.
  fn insert_split(&mut self, split: &mut Split, threshold: usize) -> Option<usize> {
    maybe_grow(|| {
      let mut n = match self {
        Term::Var { nam } => usize::from(split.fst == *nam || split.snd == *nam),
        _ => 0,
      };
      for child in self.children_mut() {
        n += child.insert_split(split, threshold)?;
      }

      if n >= threshold {
        let Split { fan, tag, fst, snd, val } = std::mem::take(split);
        let nxt = Box::new(std::mem::take(self));
        *self = Term::Let {
          pat: Box::new(Pattern::Fan(fan, tag, vec![Pattern::Var(fst), Pattern::Var(snd)])),
          val: Box::new(val),
          nxt,
        };
        None
      } else {
        Some(n)
      }
    })
  }

  pub fn fix_names(&mut self, id_counter: &mut u64, book: &Book) {
    fn fix_name(nam: &mut Option<Name>, id_counter: &mut u64, bod: &mut Term) {
      if let Some(nam) = nam {
        let name = Name::new(num_to_name(*id_counter));
        *id_counter += 1;
        bod.subst(nam, &Term::Var { nam: name.clone() });
        *nam = name;
      }
    }

    maybe_grow(|| match self {
      Term::Ref { nam: def_name } => {
        if def_name.is_generated() {
          let def = book.defs.get(def_name).unwrap();
          let mut term = def.rule().body.clone();
          term.fix_names(id_counter, book);
          *self = term;
        }
      }
      _ => {
        for (child, bnd) in self.children_mut_with_binds_mut() {
          for bnd in bnd {
            fix_name(bnd, id_counter, child);
            child.fix_names(id_counter, book);
          }
        }
      }
    })
  }
}

#[derive(Default)]
struct NameGen<'n> {
  var_port_to_id: HashMap<Port<'n>, u64>,
  id_counter: u64,
}

impl<'n> NameGen<'n> {
  // Given a port, returns its name, or assigns one if it wasn't named yet.
  fn var_name(&mut self, var_port: Port<'n>) -> Name {
    let id = self.var_port_to_id.entry(var_port).or_insert_with(|| {
      let id = self.id_counter;
      self.id_counter += 1;
      id
    });
    Name::from(*id)
  }

  fn decl_name(&mut self, net: &Wires, var_port: Port<'n>) -> Option<Name> {
    // If port is linked to an erase node, return an unused variable
    let var_use = net.enter_port(var_port);
    var_use.is_some_and(|x| *x.node() != &Tree::Era).then(|| self.var_name(var_port))
  }

  pub fn unique(&mut self) -> Name {
    let id = self.id_counter;
    self.id_counter += 1;
    Name::from(id)
  }
}

fn is_op_swapped(op: hvmc::ops::Op) -> bool {
  matches!(
    op.op,
    hvmc::ops::IntOp::ShlS
      | hvmc::ops::IntOp::ShrS
      | hvmc::ops::IntOp::SubS
      | hvmc::ops::IntOp::DivS
      | hvmc::ops::IntOp::RemS
  )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReadbackError {
  InvalidNumericMatch,
  InvalidNumericOp,
  ReachedRoot,
  UnboundVar,
}

impl std::fmt::Display for ReadbackError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ReadbackError::InvalidNumericMatch => write!(f, "Invalid Numeric Match."),
      ReadbackError::InvalidNumericOp => write!(f, "Invalid Numeric Operation."),
      ReadbackError::ReachedRoot => write!(f, "Reached Root."),
      ReadbackError::UnboundVar => write!(f, "Detected Unbound Variable."),
    }
  }
}

impl Term {
  pub fn collect_unscoped(&self, unscoped: &mut HashSet<Name>, scope: &mut Vec<Name>) {
    maybe_grow(|| match self {
      Term::Var { nam } if !scope.contains(nam) => _ = unscoped.insert(nam.clone()),
      Term::Swt { arg, bnd, with: _, pred: _, arms } => {
        arg.collect_unscoped(unscoped, scope);
        arms[0].collect_unscoped(unscoped, scope);
        if let Some(bnd) = bnd {
          scope.push(Name::new(format!("{bnd}-1")));
        }
        arms[1].collect_unscoped(unscoped, scope);
        if bnd.is_some() {
          scope.pop();
        }
      }
      _ => {
        for (child, binds) in self.children_with_binds() {
          let binds: Vec<_> = binds.collect();
          for bind in binds.iter().copied().flatten() {
            scope.push(bind.clone());
          }
          child.collect_unscoped(unscoped, scope);
          for _bind in binds.into_iter().flatten() {
            scope.pop();
          }
        }
      }
    })
  }

  pub fn apply_unscoped(&mut self, unscoped: &HashSet<Name>) {
    maybe_grow(|| {
      if let Term::Var { nam } = self
        && unscoped.contains(nam)
      {
        *self = Term::Lnk { nam: std::mem::take(nam) }
      }
      if let Some(pat) = self.pattern_mut() {
        pat.apply_unscoped(unscoped);
      }
      for child in self.children_mut() {
        child.apply_unscoped(unscoped);
      }
    })
  }
}

impl Pattern {
  fn apply_unscoped(&mut self, unscoped: &HashSet<Name>) {
    maybe_grow(|| {
      if let Pattern::Var(Some(nam)) = self
        && unscoped.contains(nam)
      {
        let nam = std::mem::take(nam);
        *self = Pattern::Chn(nam);
      }
      for child in self.children_mut() {
        child.apply_unscoped(unscoped)
      }
    })
  }
}
