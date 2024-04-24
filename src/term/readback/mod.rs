use hvmc::ast::Net;

use crate::diagnostics::{DiagnosticOrigin, Diagnostics, Severity};

use super::{encoding::Labels, AdtEncoding, Book, Term};

mod linear;
mod non_linear;
mod normalize_vars;

pub fn readback(
  net: &Net,
  book: &Book,
  labels: &Labels,
  linear: bool,
  info: &mut Diagnostics,
  adt_encoding: AdtEncoding,
) -> Term {
  let readback = if linear { linear::readback_linear } else { non_linear::readback_non_linear };
  let mut term = readback(net, book, labels, info);

  let resugar_errs = term.resugar_adts(book, adt_encoding);
  term.resugar_builtins();

  for err in resugar_errs {
    info.add_diagnostic(err, Severity::Warning, DiagnosticOrigin::Readback);
  }

  term
}
