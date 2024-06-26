use super::{
  parser::{ParseBook, TermParser},
  Book, Name,
};
use crate::{
  diagnostics::{Diagnostics, DiagnosticsConfig},
  imports::PackageLoader,
};
use std::path::Path;

// TODO: Refactor so that we don't mix the two syntaxes here.

/// Reads a file and parses to a definition book.
pub fn load_file_to_book(
  path: &Path,
  package_loader: impl PackageLoader,
  diag: DiagnosticsConfig,
) -> Result<Book, Diagnostics> {
  let code = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
  load_to_book(path, &code, package_loader, diag)
}

pub fn load_to_book(
  origin: &Path,
  code: &str,
  package_loader: impl PackageLoader,
  diag: DiagnosticsConfig,
) -> Result<Book, Diagnostics> {
  let builtins = ParseBook::builtins();
  let book = do_parse_book(code, origin, builtins)?;
  book.load_imports(package_loader, diag)
}

pub fn do_parse_book(code: &str, origin: &Path, mut book: ParseBook) -> Result<ParseBook, String> {
  book.source = Name::new(origin.to_string_lossy());
  TermParser::new(code).parse_book(book, false).map_err(|e| format!("In {} :\n{}", origin.display(), e))
}

pub fn do_parse_book_default(code: &str, origin: &Path) -> Result<Book, String> {
  do_parse_book(code, origin, ParseBook::builtins())?.to_fun()
}
