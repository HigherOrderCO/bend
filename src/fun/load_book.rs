use super::{
  parser::{ParseBook, TermParser},
  Book,
};
use crate::imports::PackageLoader;
use std::{fmt::Display, path::Path};

// TODO: Refactor so that we don't mix the two syntaxes here.

/// Reads a file and parses to a definition book.
pub fn load_file_to_book(path: &Path, package_loader: impl PackageLoader) -> Result<Book, String> {
  let code = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
  load_to_book(path.display(), &code, package_loader)
}

pub fn load_to_book<T: Display>(
  origin: T,
  code: &str,
  mut package_loader: impl PackageLoader,
) -> Result<Book, String> {
  let builtins = ParseBook::builtins();
  let mut book = do_parse_book(code, origin, builtins)?;

  book.imports.load_imports(&mut package_loader)?;
  book.apply_imports()?;

  let mut book = book.to_fun()?;
  book.desugar_ctr_use();

  Ok(book)
}

pub fn do_parse_book<T: Display>(code: &str, origin: T, book: ParseBook) -> Result<ParseBook, String> {
  TermParser::new(code).parse_book(book, false).map_err(|e| format!("In {} :\n{}", origin, e))
}

pub fn do_parse_book_default<T: Display>(code: &str, origin: T) -> Result<Book, String> {
  do_parse_book(code, origin, ParseBook::builtins())?.to_fun()
}
