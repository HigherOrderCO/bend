use bend::{
  check_book, compile_book, desugar_book,
  diagnostics::{Diagnostics, DiagnosticsConfig, Severity},
  fun::{Book, Name},
  hvm::hvm_book_show_pretty,
  imports::DefaultLoader,
  load_file_to_book, run_book, AdtEncoding, CompileOpts, OptLevel, RunOpts,
};
use clap::{Args, CommandFactory, Parser, Subcommand};
use std::{
  path::{Path, PathBuf},
  process::ExitCode,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
  #[command(subcommand)]
  pub mode: Mode,

  #[arg(short, long, global = true)]
  pub verbose: bool,

  #[arg(long, global = true, help = "HVM command or path to HVM binary")]
  pub hvm_bin: Option<String>,

  #[arg(short = 'e', long, global = true, help = "Use other entrypoint rather than main or Main")]
  pub entrypoint: Option<String>,
}

#[derive(Subcommand, Clone, Debug)]
enum Mode {
  /// Checks that the program is syntactically and semantically correct.
  Check {
    #[arg(
      short = 'O',
      value_delimiter = ' ',
      action = clap::ArgAction::Append,
      long_help = r#"Enables or disables the given optimizations
      float_combinators is enabled by default on strict mode."#,
    )]
    comp_opts: Vec<OptArgs>,

    #[command(flatten)]
    warn_opts: CliWarnOpts,

    #[arg(help = "Path to the input file")]
    path: PathBuf,
  },
  /// Compiles the program and runs it with the Rust HVM implementation.
  Run(RunArgs),
  /// Compiles the program and runs it with the C HVM implementation.
  RunC(RunArgs),
  /// Compiles the program and runs it with the Cuda HVM implementation.
  RunCu(RunArgs),
  /// Compiles the program to hvm and prints to stdout.
  GenHvm(GenArgs),
  /// Compiles the program to standalone C and prints to stdout.
  GenC(GenArgs),
  /// Compiles the program to standalone Cuda and prints to stdout.
  GenCu(GenArgs),
  /// Runs the lambda-term level desugaring passes.
  Desugar {
    #[arg(
      short = 'O',
      value_delimiter = ' ',
      action = clap::ArgAction::Append,
      long_help = r#"Enables or disables the given optimizations
      float_combinators is enabled by default on strict mode."#,
    )]
    comp_opts: Vec<OptArgs>,

    #[arg(short = 'p', help = "Debug and normalization pretty printing")]
    pretty: bool,

    #[command(flatten)]
    warn_opts: CliWarnOpts,

    #[arg(help = "Path to the input file")]
    path: PathBuf,
  },
}

#[derive(Args, Clone, Debug)]
struct RunArgs {
  #[arg(short = 'p', help = "Debug and normalization pretty printing")]
  pretty: bool,

  #[command(flatten)]
  run_opts: CliRunOpts,

  #[arg(
    short = 'O',
    value_delimiter = ' ',
    action = clap::ArgAction::Append,
    long_help = r#"Enables or disables the given optimizations
    float_combinators is enabled by default on strict mode."#,
  )]
  comp_opts: Vec<OptArgs>,

  #[command(flatten)]
  warn_opts: CliWarnOpts,

  #[arg(help = "Path to the input file")]
  path: PathBuf,

  #[arg(value_parser = |arg: &str| bend::fun::parser::TermParser::new(arg).parse_term())]
  arguments: Option<Vec<bend::fun::Term>>,
}

#[derive(Args, Clone, Debug)]
struct GenArgs {
  #[arg(
    short = 'O',
    value_delimiter = ' ',
    action = clap::ArgAction::Append,
    long_help = r#"Enables or disables the given optimizations
    float_combinators is enabled by default on strict mode."#,
  )]
  comp_opts: Vec<OptArgs>,

  #[command(flatten)]
  warn_opts: CliWarnOpts,

  #[arg(help = "Path to the input file")]
  path: PathBuf,
}

#[derive(Args, Clone, Debug)]
struct CliRunOpts {
  #[arg(short = 'l', help = "Linear readback (show explicit dups)")]
  linear: bool,

  #[arg(short = 's', long = "stats", help = "Shows runtime stats and rewrite counts")]
  print_stats: bool,
}

#[derive(Args, Debug, Clone)]
#[group(multiple = true)]
struct CliWarnOpts {
  #[arg(
    short = 'W',
    long = "warn",
    value_delimiter = ' ',
    action = clap::ArgAction::Append,
    help = "Show the specified compilation warning",
  )]
  pub warns: Vec<WarningArgs>,

  #[arg(
    short = 'D',
    long = "deny",
    value_delimiter = ' ',
    action = clap::ArgAction::Append,
    help = "Deny the specified compilation warning",
  )]
  pub denies: Vec<WarningArgs>,

  #[arg(
    short = 'A',
    long = "allow",
    value_delimiter = ' ',
    action = clap::ArgAction::Append,
    help = "Allow the specified compilation warning",
  )]
  pub allows: Vec<WarningArgs>,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum OptArgs {
  All,
  NoAll,
  Eta,
  NoEta,
  Prune,
  NoPrune,
  LinearizeMatches,
  LinearizeMatchesAlt,
  NoLinearizeMatches,
  FloatCombinators,
  NoFloatCombinators,
  Merge,
  NoMerge,
  Inline,
  NoInline,
  CheckNetSize,
  NoCheckNetSize,
  AdtScott,
  AdtNumScott,
}

fn compile_opts_from_cli(args: &Vec<OptArgs>) -> CompileOpts {
  use OptArgs::*;
  let mut opts = CompileOpts::default();

  for arg in args {
    match arg {
      All => opts = opts.set_all(),
      NoAll => opts = opts.set_no_all(),
      Eta => opts.eta = true,
      NoEta => opts.eta = false,
      Prune => opts.prune = true,
      NoPrune => opts.prune = false,
      FloatCombinators => opts.float_combinators = true,
      NoFloatCombinators => opts.float_combinators = false,
      Merge => opts.merge = true,
      NoMerge => opts.merge = false,
      Inline => opts.inline = true,
      NoInline => opts.inline = false,
      CheckNetSize => opts.check_net_size = true,
      NoCheckNetSize => opts.check_net_size = false,

      LinearizeMatches => opts.linearize_matches = OptLevel::Enabled,
      LinearizeMatchesAlt => opts.linearize_matches = OptLevel::Alt,
      NoLinearizeMatches => opts.linearize_matches = OptLevel::Disabled,

      AdtScott => opts.adt_encoding = AdtEncoding::Scott,
      AdtNumScott => opts.adt_encoding = AdtEncoding::NumScott,
    }
  }

  opts
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum WarningArgs {
  All,
  IrrefutableMatch,
  RedundantMatch,
  UnreachableMatch,
  UnusedDefinition,
  RepeatedBind,
  RecursionCycle,
  ImportShadow,
  MissingMain,
}

fn main() -> ExitCode {
  #[cfg(not(feature = "cli"))]
  compile_error!("The 'cli' feature is needed for the Bend cli");

  let cli = Cli::parse();

  if let Err(diagnostics) = execute_cli_mode(cli) {
    eprint!("{diagnostics}");
    return ExitCode::FAILURE;
  }
  ExitCode::SUCCESS
}

fn execute_cli_mode(mut cli: Cli) -> Result<(), Diagnostics> {
  let arg_verbose = cli.verbose;
  let entrypoint = cli.entrypoint.take();

  let load_book = |path: &Path, diag: DiagnosticsConfig| -> Result<Book, Diagnostics> {
    let package_loader = DefaultLoader::new(path);
    let mut book = load_file_to_book(path, package_loader, diag)?;
    book.entrypoint = entrypoint.map(Name::new);

    if arg_verbose {
      println!("{book}");
    }

    Ok(book)
  };

  // Path/command for the HVM binary
  // CLI option -> Env var -> Default
  let hvm_bin = if let Some(hvm_bin) = cli.hvm_bin {
    hvm_bin
  } else if let Ok(hvm_bin) = std::env::var("HVM_BIN") {
    hvm_bin
  } else {
    "hvm".to_string()
  };

  let gen_cmd = match &cli.mode {
    Mode::GenC(..) => "gen-c",
    Mode::GenCu(..) => "gen-cu",
    _ => "gen",
  };

  let run_cmd = match &cli.mode {
    Mode::RunC(..) => "run-c",
    Mode::RunCu(..) => "run-cu",
    _ => "run",
  };

  match cli.mode {
    Mode::Check { comp_opts, warn_opts, path } => {
      let diagnostics_cfg = set_warning_cfg_from_cli(DiagnosticsConfig::default(), warn_opts);
      let compile_opts = compile_opts_from_cli(&comp_opts);

      let mut book = load_book(&path, diagnostics_cfg)?;
      let diagnostics = check_book(&mut book, diagnostics_cfg, compile_opts)?;
      eprintln!("{}", diagnostics);
    }

    Mode::GenHvm(GenArgs { comp_opts, warn_opts, path, .. }) => {
      let diagnostics_cfg = set_warning_cfg_from_cli(DiagnosticsConfig::default(), warn_opts);
      let opts = compile_opts_from_cli(&comp_opts);

      let mut book = load_book(&path, diagnostics_cfg)?;
      let compile_res = compile_book(&mut book, opts, diagnostics_cfg, None)?;

      eprint!("{}", compile_res.diagnostics);
      println!("{}", hvm_book_show_pretty(&compile_res.hvm_book));
    }

    Mode::Run(RunArgs { pretty, run_opts, comp_opts, warn_opts, path, arguments })
    | Mode::RunC(RunArgs { pretty, run_opts, comp_opts, warn_opts, path, arguments })
    | Mode::RunCu(RunArgs { pretty, run_opts, comp_opts, warn_opts, path, arguments }) => {
      let CliRunOpts { linear, print_stats } = run_opts;

      let diagnostics_cfg =
        set_warning_cfg_from_cli(DiagnosticsConfig::new(Severity::Allow, arg_verbose), warn_opts);

      let compile_opts = compile_opts_from_cli(&comp_opts);

      compile_opts.check_for_strict();

      let run_opts = RunOpts { linear_readback: linear, pretty, hvm_path: hvm_bin };

      let book = load_book(&path, diagnostics_cfg)?;
      if let Some((term, stats, diags)) =
        run_book(book, run_opts, compile_opts, diagnostics_cfg, arguments, run_cmd)?
      {
        eprint!("{diags}");
        if pretty {
          println!("Result:\n{}", term.display_pretty(0));
        } else {
          println!("Result: {}", term);
        }
        if print_stats {
          println!("{stats}");
        }
      }
    }

    Mode::GenC(GenArgs { comp_opts, warn_opts, path })
    | Mode::GenCu(GenArgs { comp_opts, warn_opts, path }) => {
      let diagnostics_cfg = set_warning_cfg_from_cli(DiagnosticsConfig::default(), warn_opts);
      let opts = compile_opts_from_cli(&comp_opts);

      let mut book = load_book(&path, diagnostics_cfg)?;
      let compile_res = compile_book(&mut book, opts, diagnostics_cfg, None)?;

      let out_path = ".out.hvm";
      std::fs::write(out_path, hvm_book_show_pretty(&compile_res.hvm_book)).map_err(|x| x.to_string())?;

      let gen_fn = |out_path: &str| {
        let mut process = std::process::Command::new(hvm_bin);
        process.arg(gen_cmd).arg(out_path);
        process.output().map_err(|e| format!("While running hvm: {e}"))
      };

      let std::process::Output { stdout, stderr, status } = gen_fn(out_path)?;
      let out = String::from_utf8_lossy(&stdout);
      let err = String::from_utf8_lossy(&stderr);
      let status = if !status.success() { status.to_string() } else { String::new() };

      if let Err(e) = std::fs::remove_file(out_path) {
        eprintln!("Error removing HVM output file. {e}");
      }

      eprintln!("{err}");
      println!("{out}");
      println!("{status}");
    }

    Mode::Desugar { path, comp_opts, warn_opts, pretty } => {
      let diagnostics_cfg = set_warning_cfg_from_cli(DiagnosticsConfig::default(), warn_opts);

      let opts = compile_opts_from_cli(&comp_opts);

      let mut book = load_book(&path, diagnostics_cfg)?;
      let diagnostics = desugar_book(&mut book, opts, diagnostics_cfg, None)?;

      eprint!("{diagnostics}");
      if pretty {
        println!("{}", book.display_pretty())
      } else {
        println!("{book}");
      }
    }
  };
  Ok(())
}

fn set_warning_cfg_from_cli(mut cfg: DiagnosticsConfig, warn_opts: CliWarnOpts) -> DiagnosticsConfig {
  fn set(cfg: &mut DiagnosticsConfig, severity: Severity, cli_val: WarningArgs) {
    match cli_val {
      WarningArgs::All => {
        cfg.irrefutable_match = severity;
        cfg.redundant_match = severity;
        cfg.unreachable_match = severity;
        cfg.unused_definition = severity;
        cfg.repeated_bind = severity;
        cfg.recursion_cycle = severity;
        cfg.import_shadow = severity;
      }
      WarningArgs::IrrefutableMatch => cfg.irrefutable_match = severity,
      WarningArgs::RedundantMatch => cfg.redundant_match = severity,
      WarningArgs::UnreachableMatch => cfg.unreachable_match = severity,
      WarningArgs::UnusedDefinition => cfg.unused_definition = severity,
      WarningArgs::RepeatedBind => cfg.repeated_bind = severity,
      WarningArgs::RecursionCycle => cfg.recursion_cycle = severity,
      WarningArgs::ImportShadow => cfg.import_shadow = severity,
      WarningArgs::MissingMain => cfg.missing_main = severity, // TODO: Should `WarningArgs::All` modify this as well?
    }
  }

  let cmd = Cli::command();
  let matches = cmd.get_matches();
  let subcmd_name = matches.subcommand_name().expect("To have a subcommand");
  let arg_matches = matches.subcommand_matches(subcmd_name).expect("To have a subcommand");

  if let Some(warn_opts_ids) = arg_matches.get_many::<clap::Id>("CliWarnOpts") {
    let mut allows = warn_opts.allows.into_iter();
    let mut warns = warn_opts.warns.into_iter();
    let mut denies = warn_opts.denies.into_iter();
    for id in warn_opts_ids {
      match id.as_ref() {
        "allows" => set(&mut cfg, Severity::Allow, allows.next().unwrap()),
        "denies" => set(&mut cfg, Severity::Error, denies.next().unwrap()),
        "warns" => set(&mut cfg, Severity::Warning, warns.next().unwrap()),
        _ => unreachable!(),
      }
    }
  }
  cfg
}
