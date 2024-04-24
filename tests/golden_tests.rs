use hvml::{
  compile_book, desugar_book,
  diagnostics::{Diagnostics, DiagnosticsConfig, Severity, ToStringVerbose},
  run_book,
  term::{
    encoding::{encode_term, Labels},
    load_book::do_parse_book,
    parser::TermParser,
    AdtEncoding, Book, Ctx, Name,
  },
  CompileOpts, RunOpts,
};
use insta::assert_snapshot;
use itertools::Itertools;
use std::{
  collections::HashMap,
  fmt::Write,
  io::Read,
  path::{Path, PathBuf},
  str::FromStr,
};
use stdext::function_name;
use walkdir::WalkDir;

fn format_output(output: std::process::Output) -> String {
  format!("{}{}", String::from_utf8_lossy(&output.stderr), String::from_utf8_lossy(&output.stdout))
}

const TESTS_PATH: &str = "/tests/golden_tests/";

type RunFn = dyn Fn(&str, &Path) -> Result<String, Diagnostics>;

fn run_single_golden_test(path: &Path, run: &[&RunFn]) -> Result<(), String> {
  let code = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
  let file_name = path.to_str().and_then(|path| path.rsplit_once(TESTS_PATH)).unwrap().1;

  // unfortunately we need to do this
  let file_path = format!("{}{}", &TESTS_PATH[1 ..], file_name);
  let file_path = Path::new(&file_path);

  let mut results: HashMap<&Path, Vec<String>> = HashMap::new();
  for fun in run {
    let result = fun(&code, file_path).unwrap_or_else(|err| err.to_string());
    results.entry(file_path).or_default().push(result);
  }
  let results = results.into_values().map(|v| v.join("\n")).collect_vec();

  let mut settings = insta::Settings::clone_current();
  settings.set_prepend_module_to_snapshot(false);
  settings.set_omit_expression(true);
  settings.set_input_file(path);

  settings.bind(|| {
    for result in results {
      assert_snapshot!(file_name, result);
    }
  });

  Ok(())
}

fn run_golden_test_dir(test_name: &str, run: &RunFn) {
  run_golden_test_dir_multiple(test_name, &[run])
}

fn run_golden_test_dir_multiple(test_name: &str, run: &[&RunFn]) {
  let root = PathBuf::from(format!(
    "{}{TESTS_PATH}{}",
    env!("CARGO_MANIFEST_DIR"),
    test_name.rsplit_once(':').unwrap().1
  ));

  let walker = WalkDir::new(&root).sort_by_file_name().max_depth(2).into_iter().filter_entry(|e| {
    let path = e.path();
    path == root || path.is_dir() || (path.is_file() && path.extension().is_some_and(|x| x == "hvm"))
  });

  for entry in walker {
    let entry = entry.unwrap();
    let path = entry.path();
    if path.is_file() {
      eprintln!("Testing {}", path.display());
      run_single_golden_test(path, run).unwrap();
    }
  }
}

/* Snapshot/regression/golden tests

 Each tests runs all the files in tests/golden_tests/<test name>.

 The test functions decide how exactly to process the test programs
 and what to save as a snapshot.
*/

#[test]
fn compile_term() {
  run_golden_test_dir(function_name!(), &|code, _| {
    let mut term = TermParser::new_term(code)?;
    let mut vec = Vec::new();
    term.check_unbound_vars(&mut HashMap::new(), &mut vec);

    if !vec.is_empty() {
      return Err(vec.into_iter().map(|e| e.to_string_verbose(true)).join("\n").into());
    }

    term.make_var_names_unique();
    term.linearize_vars();
    let net = encode_term(&term, &mut Default::default()).map_err(|e| e.to_string_verbose(true))?;

    Ok(format!("{}", net))
  })
}

#[test]
fn compile_file() {
  run_golden_test_dir(function_name!(), &|code, path| {
    let mut book = do_parse_book(code, path)?;
    let compile_opts = CompileOpts::default_strict();
    let diagnostics_cfg = DiagnosticsConfig::default_strict();
    let res = compile_book(&mut book, compile_opts, diagnostics_cfg, None)?;
    Ok(format!("{}{}", res.diagnostics, res.core_book))
  })
}

#[test]
fn compile_file_o_all() {
  run_golden_test_dir(function_name!(), &|code, path| {
    let mut book = do_parse_book(code, path)?;
    let opts = CompileOpts::default_strict().set_all();
    let diagnostics_cfg =
      DiagnosticsConfig { recursion_cycle: Severity::Warning, ..DiagnosticsConfig::default_strict() };
    let res = compile_book(&mut book, opts, diagnostics_cfg, None)?;
    Ok(format!("{}{}", res.diagnostics, res.core_book))
  })
}

#[test]
fn compile_file_o_no_all() {
  run_golden_test_dir(function_name!(), &|code, path| {
    let mut book = do_parse_book(code, path)?;
    let compile_opts =
      CompileOpts { adt_encoding: AdtEncoding::Scott, ..CompileOpts::default_strict().set_no_all() };
    let diagnostics_cfg = DiagnosticsConfig::default_strict();
    let res = compile_book(&mut book, compile_opts, diagnostics_cfg, None)?;
    Ok(format!("{}", res.core_book))
  })
}

#[test]
fn run_file() {
  run_golden_test_dir_multiple(function_name!(), &[
    (&|_code, path| {
      let output = std::process::Command::new(env!("CARGO_BIN_EXE_hvml"))
        .args([
          "run",
          path.to_str().unwrap(),
          "-Dall",
          "-A=recursion-cycle",
          "-O=all",
          "-O=linearize-matches",
          "-O=no-pre-reduce",
          "-L",
          "-1",
        ])
        .output()
        .expect("Run process");

      Ok(format!("Lazy mode:\n{}", format_output(output)))
    }),
    (&|_code, path| {
      let output = std::process::Command::new(env!("CARGO_BIN_EXE_hvml"))
        .args(["run", path.to_str().unwrap(), "-Dall", "-Oall"])
        .output()
        .expect("Run process");

      Ok(format!("Strict mode:\n{}", format_output(output)))
    }),
  ])
}

#[test]
fn run_lazy() {
  run_golden_test_dir(function_name!(), &|code, path| {
    let book = do_parse_book(code, path)?;
    let compile_opts = CompileOpts::default_lazy();
    let diagnostics_cfg = DiagnosticsConfig {
      recursion_cycle: Severity::Allow,
      recursion_pre_reduce: Severity::Allow,
      unused_definition: Severity::Allow,
      ..DiagnosticsConfig::new(Severity::Error, true)
    };
    let run_opts = RunOpts::lazy();

    let (res, info) = run_book(book, None, run_opts, compile_opts, diagnostics_cfg, None)?;
    Ok(format!("{}{}", info.diagnostics, res))
  })
}

#[test]
fn readback() {
  run_golden_test_dir(function_name!(), &|code, _| {
    let net = hvmc::ast::Net::from_str(code)?;
    let book = Book::default();
    let mut non_linear_diags = Diagnostics::default();
    let non_linear_term =
      hvml::term::readback(&net, &book, &Labels::default(), false, &mut non_linear_diags, AdtEncoding::Scott);
    let mut linear_diags = Diagnostics::default();
    let linear_term =
      hvml::term::readback(&net, &book, &Labels::default(), true, &mut linear_diags, AdtEncoding::Scott);
    Ok(format!(
      "non-linear:\n{}{}\n\nlinear:\n{}{}",
      non_linear_diags, non_linear_term, linear_diags, linear_term
    ))
  })
}

#[test]
fn simplify_matches() {
  run_golden_test_dir(function_name!(), &|code, path| {
    let diagnostics_cfg = DiagnosticsConfig::new(Severity::Error, true);
    let mut book = do_parse_book(code, path)?;
    let mut ctx = Ctx::new(&mut book, diagnostics_cfg);

    ctx.check_shared_names();
    ctx.set_entrypoint();
    ctx.book.encode_adts(AdtEncoding::TaggedScott);
    ctx.fix_match_defs()?;
    ctx.book.encode_builtins();
    ctx.resolve_refs()?;
    ctx.desugar_match_defs()?;
    ctx.fix_match_terms()?;
    ctx.check_unbound_vars()?;
    ctx.book.make_var_names_unique();
    ctx.book.linearize_match_binds();
    ctx.book.linearize_match_with();
    ctx.check_unbound_vars()?;
    ctx.book.make_var_names_unique();
    ctx.book.apply_use();
    ctx.book.make_var_names_unique();
    ctx.prune(false, AdtEncoding::TaggedScott);

    Ok(ctx.book.to_string())
  })
}

#[test]
fn parse_file() {
  run_golden_test_dir(function_name!(), &|code, path| {
    let book = do_parse_book(code, path)?;
    Ok(book.to_string())
  })
}

#[test]
fn encode_pattern_match() {
  run_golden_test_dir(function_name!(), &|code, path| {
    let mut result = String::new();
    for adt_encoding in [AdtEncoding::TaggedScott, AdtEncoding::Scott] {
      let diagnostics_cfg = DiagnosticsConfig::default_strict();
      let mut book = do_parse_book(code, path)?;
      let mut ctx = Ctx::new(&mut book, diagnostics_cfg);
      ctx.check_shared_names();
      ctx.set_entrypoint();
      ctx.book.encode_adts(adt_encoding);
      ctx.fix_match_defs()?;
      ctx.book.encode_builtins();
      ctx.resolve_refs()?;
      ctx.desugar_match_defs()?;
      ctx.fix_match_terms()?;
      ctx.check_unbound_vars()?;
      ctx.book.make_var_names_unique();
      ctx.book.linearize_match_binds();
      ctx.book.linearize_match_with();
      ctx.book.encode_matches(adt_encoding);
      ctx.check_unbound_vars()?;
      ctx.book.make_var_names_unique();
      ctx.book.apply_use();
      ctx.book.make_var_names_unique();
      ctx.book.linearize_vars();
      ctx.prune(false, adt_encoding);

      writeln!(result, "{adt_encoding:?}:").unwrap();
      writeln!(result, "{}\n", ctx.book).unwrap();
    }
    Ok(result)
  })
}

#[test]
fn desugar_file() {
  run_golden_test_dir(function_name!(), &|code, path| {
    let compile_opts = CompileOpts::default_strict();
    let diagnostics_cfg = DiagnosticsConfig {
      unused_definition: Severity::Allow,
      ..DiagnosticsConfig::new(Severity::Error, true)
    };
    let mut book = do_parse_book(code, path)?;
    desugar_book(&mut book, compile_opts, diagnostics_cfg, None)?;
    Ok(book.to_string())
  })
}

#[test]
#[ignore = "to not delay golden tests execution"]
fn hangs() {
  let expected_normalization_time = 5;

  run_golden_test_dir(function_name!(), &move |code, path| {
    let book = do_parse_book(code, path)?;
    let compile_opts = CompileOpts { pre_reduce: false, ..CompileOpts::default_strict().set_all() };
    let diagnostics_cfg = DiagnosticsConfig {
      recursion_cycle: Severity::Allow,
      recursion_pre_reduce: Severity::Allow,
      ..DiagnosticsConfig::default_strict()
    };

    let thread = std::thread::spawn(move || {
      run_book(book, None, RunOpts::default(), compile_opts, diagnostics_cfg, None)
    });
    std::thread::sleep(std::time::Duration::from_secs(expected_normalization_time));

    if !thread.is_finished() {
      Ok("Hangs".into())
    } else if let Err(diags) = thread.join().unwrap() {
      Err(format!("Doesn't hang. (Compilation failed)\n{diags}").into())
    } else {
      Err("Doesn't hang. (Ran to the end)".to_string().into())
    }
  })
}

#[test]
fn compile_entrypoint() {
  run_golden_test_dir(function_name!(), &|code, path| {
    let mut book = do_parse_book(code, path)?;
    book.entrypoint = Some(Name::new("foo"));
    let diagnostics_cfg = DiagnosticsConfig::new(Severity::Error, true);
    let res = compile_book(&mut book, CompileOpts::default_strict(), diagnostics_cfg, None)?;
    Ok(format!("{}{}", res.diagnostics, res.core_book))
  })
}

#[test]
fn run_entrypoint() {
  run_golden_test_dir(function_name!(), &|code, path| {
    let mut book = do_parse_book(code, path)?;
    book.entrypoint = Some(Name::new("foo"));
    let compile_opts = CompileOpts::default_strict().set_all();
    let diagnostics_cfg = DiagnosticsConfig::new(Severity::Error, true);
    let (res, info) = run_book(book, None, RunOpts::default(), compile_opts, diagnostics_cfg, None)?;
    Ok(format!("{}{}", info.diagnostics, res))
  })
}

#[test]
fn cli() {
  run_golden_test_dir(function_name!(), &|_code, path| {
    let mut args_path = PathBuf::from(path);
    assert!(args_path.set_extension("args"));

    let mut args_buf = String::with_capacity(16);
    let mut args_file = std::fs::File::open(args_path).expect("File exists");
    args_file.read_to_string(&mut args_buf).expect("Read args");
    let args = args_buf.lines();

    let output =
      std::process::Command::new(env!("CARGO_BIN_EXE_hvml")).args(args).output().expect("Run command");

    Ok(format_output(output))
  })
}

#[test]
fn mutual_recursion() {
  run_golden_test_dir(function_name!(), &|code, path| {
    let diagnostics_cfg =
      DiagnosticsConfig { recursion_cycle: Severity::Error, ..DiagnosticsConfig::new(Severity::Allow, true) };
    let mut book = do_parse_book(code, path)?;
    let mut opts = CompileOpts::default_strict();
    opts.merge = true;
    let res = compile_book(&mut book, opts, diagnostics_cfg, None)?;
    Ok(format!("{}{}", res.diagnostics, res.core_book))
  })
}

#[test]
fn io() {
  run_golden_test_dir_multiple(function_name!(), &[
    (&|code, path| {
      let book = do_parse_book(code, path)?;
      let compile_opts = CompileOpts::default_lazy();
      let diagnostics_cfg = DiagnosticsConfig::default_lazy();
      let (res, info) = run_book(book, None, RunOpts::lazy(), compile_opts, diagnostics_cfg, None)?;
      Ok(format!("Lazy mode:\n{}{}", info.diagnostics, res))
    }),
    (&|code, path| {
      let book = do_parse_book(code, path)?;
      let compile_opts = CompileOpts::default_strict();
      let diagnostics_cfg = DiagnosticsConfig::default_strict();
      let (res, info) = run_book(book, None, RunOpts::default(), compile_opts, diagnostics_cfg, None)?;
      Ok(format!("Strict mode:\n{}{}", info.diagnostics, res))
    }),
  ])
}

#[test]
fn examples() -> Result<(), Diagnostics> {
  let examples_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");

  for entry in WalkDir::new(examples_path)
    .min_depth(1)
    .into_iter()
    .filter_map(|e| e.ok())
    .filter(|e| e.path().extension().map_or(false, |ext| ext == "hvm"))
  {
    let path = entry.path();
    eprintln!("Testing {}", path.display());
    let code = std::fs::read_to_string(path).map_err(|e| e.to_string())?;

    let book = do_parse_book(&code, path).unwrap();
    let mut compile_opts = CompileOpts::default_strict();
    compile_opts.linearize_matches = hvml::OptLevel::Extra;
    let diagnostics_cfg = DiagnosticsConfig::default_strict();
    let (res, _) = run_book(book, None, RunOpts::default(), compile_opts, diagnostics_cfg, None)?;

    let mut settings = insta::Settings::clone_current();
    settings.set_prepend_module_to_snapshot(false);
    settings.set_omit_expression(true);
    settings.set_input_file(path);

    settings.bind(|| {
      assert_snapshot!(format!("examples__{}", path.file_name().unwrap().to_str().unwrap()), res);
    });
  }

  Ok(())
}

#[test]
fn scott_triggers_unused() {
  run_golden_test_dir(function_name!(), &|code, path| {
    let mut book = do_parse_book(code, path)?;
    let mut opts = CompileOpts::default_strict();
    opts.adt_encoding = AdtEncoding::Scott;
    let diagnostics_cfg =
      DiagnosticsConfig { unused_definition: Severity::Error, ..DiagnosticsConfig::default_strict() };
    let res = compile_book(&mut book, opts, diagnostics_cfg, None)?;
    Ok(format!("{}{}", res.diagnostics, res.core_book))
  })
}
