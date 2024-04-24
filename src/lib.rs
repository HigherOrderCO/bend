#![feature(box_patterns)]
#![feature(let_chains)]

use builtins::{create_host, CORE_BUILTINS_USES};
use diagnostics::{Diagnostics, DiagnosticsConfig, Severity, WarningType};
use hvmc::{
  ast::Net,
  dispatch_dyn_net,
  host::Host,
  run::{DynNet, Heap, Rewrites},
};
use hvmc_net::{
  mutual_recursion,
  pre_reduce::{pre_reduce, MAX_REWRITES_DEFAULT},
};
use parking_lot::Mutex;
use std::{sync::Arc, time::Instant};
use term::{encode_book, encoding::Labels, AdtEncoding, Book, Ctx, Name, Term};

pub mod builtins;
pub mod diagnostics;
pub mod hvmc_net;
pub mod term;

pub use term::load_book::load_file_to_book;

use crate::term::readback;

pub const ENTRY_POINT: &str = "main";
pub const HVM1_ENTRY_POINT: &str = "Main";

pub fn check_book(
  book: &mut Book,
  diagnostics_cfg: DiagnosticsConfig,
  compile_opts: CompileOpts,
) -> Result<(), Diagnostics> {
  // TODO: Do the checks without having to do full compilation
  let res = compile_book(book, compile_opts, diagnostics_cfg, None)?;
  print!("{}", res.diagnostics);
  Ok(())
}

pub fn compile_book(
  book: &mut Book,
  opts: CompileOpts,
  diagnostics_cfg: DiagnosticsConfig,
  args: Option<Vec<Term>>,
) -> Result<CompileResult, Diagnostics> {
  let mut diagnostics = desugar_book(book, opts.clone(), diagnostics_cfg, args)?;
  let (mut core_book, labels) = encode_book(book, &mut diagnostics)?;

  if opts.eta {
    core_book.values_mut().for_each(Net::eta_reduce);
  }

  mutual_recursion::check_cycles(&core_book, &mut diagnostics)?;

  if opts.pre_reduce || diagnostics.config.warning_severity(WarningType::RecursionPreReduce) > Severity::Allow
  {
    pre_reduce(
      &mut core_book,
      book.hvmc_entrypoint(),
      opts.pre_reduce_rewrites,
      opts.pre_reduce_memory,
      !opts.pre_reduce,
      &mut diagnostics,
    )?;
  }

  if opts.eta {
    core_book.values_mut().for_each(Net::eta_reduce);
  }

  if opts.inline {
    diagnostics.start_pass();
    if let Err(e) = core_book.inline() {
      diagnostics.add_book_error(e);
    }
    diagnostics.fatal(())?;
  }

  if opts.prune {
    let mut prune_entrypoints = vec![book.hvmc_entrypoint().to_string()];
    let mut builtin_uses = CORE_BUILTINS_USES.concat().iter().map(|x| x.to_string()).collect::<Vec<_>>();
    prune_entrypoints.append(&mut builtin_uses);
    core_book.prune(&prune_entrypoints);
  }

  Ok(CompileResult { core_book, labels, diagnostics })
}

pub fn desugar_book(
  book: &mut Book,
  opts: CompileOpts,
  diagnostics_cfg: DiagnosticsConfig,
  args: Option<Vec<Term>>,
) -> Result<Diagnostics, Diagnostics> {
  let mut ctx = Ctx::new(book, diagnostics_cfg);

  ctx.check_shared_names();

  ctx.set_entrypoint();

  ctx.book.encode_adts(opts.adt_encoding);

  ctx.fix_match_defs()?;

  ctx.apply_args(args)?;

  ctx.book.encode_builtins();

  ctx.resolve_refs()?;

  ctx.book.apply_bnd();

  ctx.desugar_match_defs()?;

  ctx.fix_match_terms()?;

  ctx.check_unbound_vars()?;

  ctx.book.make_var_names_unique();

  // Auto match linearization
  match opts.linearize_matches {
    OptLevel::Disabled => (),
    OptLevel::Enabled => ctx.book.linearize_match_binds(),
    OptLevel::Extra => ctx.book.linearize_matches(),
  }
  // Manual match linearization
  ctx.book.linearize_match_with();

  ctx.book.encode_matches(opts.adt_encoding);

  // sanity check
  ctx.check_unbound_vars()?;

  ctx.book.make_var_names_unique();
  ctx.book.apply_use();
  ctx.book.make_var_names_unique();
  ctx.book.linearize_vars();

  // sanity check
  ctx.check_unbound_vars()?;

  // Optimizing passes
  if opts.float_combinators {
    ctx.book.float_combinators();
  }

  ctx.prune(opts.prune, opts.adt_encoding);

  if opts.merge {
    ctx.book.merge_definitions();
  }

  ctx.book.make_var_names_unique();

  if !ctx.info.has_errors() { Ok(ctx.info) } else { Err(ctx.info) }
}

pub fn run_book(
  mut book: Book,
  max_memory: Option<usize>,
  run_opts: RunOpts,
  compile_opts: CompileOpts,
  diagnostics_cfg: DiagnosticsConfig,
  args: Option<Vec<Term>>,
) -> Result<(Term, RunInfo), Diagnostics> {
  let CompileResult { core_book, labels, diagnostics } =
    compile_book(&mut book, compile_opts.clone(), diagnostics_cfg, args)?;

  // TODO: Printing should be taken care by the cli module, but we'd
  // like to print any warnings before running so that the user can
  // cancel the run if a problem is detected.
  eprint!("{diagnostics}");

  // Turn the book into an Arc so that we can use it for logging, debugging, etc.
  // from anywhere else in the program
  // This "freezes" the book and prevents further modification.
  let book = Arc::new(book);
  let labels = Arc::new(labels);

  let debug_hook = run_opts.debug_hook(&book, &labels, compile_opts.adt_encoding);

  let host = create_host(book.clone(), labels.clone(), compile_opts.adt_encoding);
  host.lock().insert_book(&core_book);

  let (res_lnet, stats) = run_compiled(host, max_memory, run_opts, debug_hook, book.hvmc_entrypoint());

  let (res_term, diagnostics) =
    readback_with_errors(&res_lnet, &book, &labels, run_opts.linear, compile_opts.adt_encoding);

  let info = RunInfo { stats, diagnostics, net: res_lnet, book, labels };
  Ok((res_term, info))
}

/// Utility function to count the amount of nodes in an hvm-core AST net
pub fn count_nodes<'l>(net: &'l hvmc::ast::Net) -> usize {
  let mut visit: Vec<&'l hvmc::ast::Tree> = vec![&net.root];
  let mut count = 0usize;
  for (l, r) in &net.redexes {
    visit.push(l);
    visit.push(r);
  }
  while let Some(tree) = visit.pop() {
    // If it is not 0-ary, then we'll count it as a node.
    if tree.children().next().is_some() {
      count += 1;
    }
    for subtree in tree.children() {
      visit.push(subtree);
    }
  }
  count
}

pub fn run_compiled(
  host: Arc<Mutex<Host>>,
  mem_size: Option<usize>,
  run_opts: RunOpts,
  hook: Option<impl FnMut(&Net)>,
  entrypoint: &str,
) -> (Net, RunStats) {
  let heap = Heap::new(mem_size).expect("memory allocation failed");
  let mut root = DynNet::new(&heap, run_opts.lazy_mode);
  let max_rwts = run_opts.max_rewrites.map(|x| x.clamp(usize::MIN, usize::MAX));
  dispatch_dyn_net!(&mut root => {
    root.boot(host.lock().defs.get(entrypoint).expect("No main function."));

    let start_time = Instant::now();

    match (hook, run_opts.lazy_mode, max_rwts) {
      (Some(hook), true, _) => normal_lazy_debug(hook, host.clone(), root),
      (Some(hook), false, _) => normal_strict_debug(hook, host.clone(), root),
      (_, lazy_mode, Some(max_rwts)) if !lazy_mode => {
        if !run_opts.single_core {
          panic!("Parallel mode does not yet support rewrite limit");
        }
        normal_strict_rwts(max_rwts, root);
      },
      (_, true, Some(_)) => {
        panic!("Lazy mode does not yet support rewrite limit");
      },
      (_, false, None) if !run_opts.single_core => root.parallel_normal(),
      _ => root.normal(),
    }

    let elapsed = start_time.elapsed().as_secs_f64();

    let net = host.lock().readback(root);

    let stats = RunStats { rewrites: root.rwts, used: count_nodes(&net), run_time: elapsed };
    (net, stats)
  })
}

pub fn readback_with_errors(
  net: &Net,
  book: &Arc<Book>,
  labels: &Arc<Labels>,
  linear: bool,
  adt_encoding: AdtEncoding,
) -> (Term, Diagnostics) {
  let mut diags = Diagnostics::default();
  let term = readback(net, book, labels, linear, &mut diags, adt_encoding);

  (term, diags)
}

fn normal_lazy_debug<M: hvmc::run::Mode>(
  mut hook: impl FnMut(&Net),
  host: Arc<Mutex<Host>>,
  root: &mut hvmc::run::Net<M>,
) {
  let mut visit = vec![hvmc::run::Port::new_var(root.root.addr())];
  while let Some(prev) = visit.pop() {
    let next = root.weak_normal(prev, root.root.clone());

    let readback = host.lock().readback(root);
    hook(&readback);

    if next.is_full_node() {
      visit.push(hvmc::run::Port::new_var(next.addr()));
      visit.push(hvmc::run::Port::new_var(next.addr().other_half()));
    }
  }
}

fn normal_strict_debug<M: hvmc::run::Mode>(
  mut hook: impl FnMut(&Net),
  host: Arc<Mutex<Host>>,
  root: &mut hvmc::run::Net<M>,
) {
  while !root.redexes.is_empty() {
    let readback = host.lock().readback(root);
    hook(&readback);
    root.reduce(1);
  }
}

fn normal_strict_rwts<M: hvmc::run::Mode>(mut max_rwts: usize, root: &mut hvmc::run::Net<M>) {
  root.expand();
  while !root.redexes.is_empty() {
    let old_rwts = root.rwts.total();
    root.reduce(max_rwts);
    let delta_rwts = root.rwts.total() - old_rwts;
    if (max_rwts as u64) < delta_rwts {
      eprintln!("Warning: Exceeded max rwts");
      break;
    }
    max_rwts -= delta_rwts as usize;
    root.expand();
  }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RunOpts {
  pub single_core: bool,
  pub debug: bool,
  pub linear: bool,
  pub lazy_mode: bool,
  pub max_memory: Option<usize>,
  pub max_rewrites: Option<usize>,
  pub pretty: bool,
}

impl RunOpts {
  pub fn lazy() -> Self {
    Self { lazy_mode: true, single_core: true, ..Self::default() }
  }

  fn debug_hook<'a>(
    &'a self,
    book: &'a Book,
    labels: &'a Labels,
    adt_encoding: AdtEncoding,
  ) -> Option<impl FnMut(&Net) + 'a> {
    self.debug.then_some({
      move |net: &_| {
        let mut diags = Diagnostics::default();
        let res_term = readback(net, book, labels, self.linear, &mut diags, adt_encoding);
        eprint!("{diags}");
        if self.pretty {
          println!("{}\n---------------------------------------", res_term.display_pretty(0));
        } else {
          println!("{}\n---------------------------------------", res_term);
        }
      }
    })
  }
}

#[derive(Clone, Copy, Debug, Default)]
pub enum OptLevel {
  #[default]
  Disabled,
  Enabled,
  Extra,
}

impl OptLevel {
  pub fn enabled(&self) -> bool {
    !matches!(self, OptLevel::Disabled)
  }

  pub fn is_extra(&self) -> bool {
    matches!(self, OptLevel::Extra)
  }
}

#[derive(Clone, Debug)]
pub struct CompileOpts {
  /// Selects the encoding for the ADT syntax.
  pub adt_encoding: AdtEncoding,

  /// Enables [term::transform::eta_reduction].
  pub eta: bool,

  /// Enables [term::transform::definition_pruning] and [hvmc_net::prune].
  pub prune: bool,

  /// Enables [hvmc_net::pre_reduce].
  pub pre_reduce: bool,

  /// Enables [term::transform::linearize_matches].
  pub linearize_matches: OptLevel,

  /// Enables [term::transform::float_combinators].
  pub float_combinators: bool,

  /// Enables [term::transform::definition_merge]
  pub merge: bool,

  /// Enables [term::transform::inline].
  pub inline: bool,

  pub pre_reduce_memory: Option<usize>,

  pub pre_reduce_rewrites: u64,

  pub pre_reduce_skip: Vec<Name>,
}

impl CompileOpts {
  /// Set all opts as true and keep the current adt encoding.
  #[must_use]
  pub fn set_all(self) -> Self {
    Self {
      eta: true,
      prune: true,
      pre_reduce: true,
      float_combinators: true,
      merge: true,
      inline: true,
      linearize_matches: OptLevel::Extra,
      ..self
    }
  }

  /// Set all opts as false and keep the current adt encoding.
  #[must_use]
  pub fn set_no_all(self) -> Self {
    Self {
      adt_encoding: self.adt_encoding,
      pre_reduce_memory: self.pre_reduce_memory,
      pre_reduce_rewrites: self.pre_reduce_rewrites,
      ..Self::default()
    }
  }

  /// All optimizations disabled, except float_combinators and linearize_matches
  pub fn default_strict() -> Self {
    Self { float_combinators: true, ..Self::default() }
  }

  // Disable optimizations that don't work or are unnecessary on lazy mode
  pub fn default_lazy() -> Self {
    Self::default()
  }

  pub fn check_for_strict(&self) {
    if !self.float_combinators {
      println!(
        "Warning: Running in strict mode without enabling the float_combinators pass can lead to some functions expanding infinitely."
      );
    }
    if !self.linearize_matches.enabled() {
      println!(
        "Warning: Running in strict mode without enabling the linearize_matches pass can lead to some functions expanding infinitely."
      );
    }
  }
}

impl Default for CompileOpts {
  fn default() -> Self {
    Self {
      eta: false,
      prune: false,
      pre_reduce: false,
      linearize_matches: OptLevel::Enabled,
      float_combinators: false,
      merge: false,
      inline: false,
      adt_encoding: AdtEncoding::default(),
      pre_reduce_memory: None,
      pre_reduce_rewrites: MAX_REWRITES_DEFAULT,
      pre_reduce_skip: vec![],
    }
  }
}

pub struct CompileResult {
  pub diagnostics: Diagnostics,
  pub core_book: hvmc::ast::Book,
  pub labels: Labels,
}

pub struct RunInfo {
  pub stats: RunStats,
  pub diagnostics: Diagnostics,
  pub net: Net,
  pub book: Arc<Book>,
  pub labels: Arc<Labels>,
}

pub struct RunStats {
  pub rewrites: Rewrites,
  pub used: usize,
  pub run_time: f64,
}

fn maybe_grow<R, F>(f: F) -> R
where
  F: FnOnce() -> R,
{
  stacker::maybe_grow(1024 * 32, 1024 * 1024, f)
}
