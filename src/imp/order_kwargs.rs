use crate::{
  fun::{parser::ParseBook, Name},
  imp::{Definition, Expr, Stmt},
};
use indexmap::IndexMap;

impl Definition {
  /// Traverses the program's definitions and adjusts the order of keyword arguments
  /// in call/constructor expressions to match the order specified in the function or constructor definition.
  pub fn order_kwargs(&mut self, book: &ParseBook) -> Result<(), String> {
    let use_map = &mut IndexMap::new();
    self.body.order_kwargs(book, use_map).map_err(|e| format!("In function '{}':\n  {}", self.name, e))
  }
}

impl Stmt {
  fn order_kwargs(&mut self, book: &ParseBook, use_map: &mut IndexMap<Name, Name>) -> Result<(), String> {
    match self {
      Stmt::Assign { val, nxt, .. } => {
        val.order_kwargs(book, use_map)?;
        if let Some(nxt) = nxt {
          nxt.order_kwargs(book, use_map)?;
        }
      }
      Stmt::Ask { val, nxt, .. } => {
        val.order_kwargs(book, use_map)?;
        nxt.order_kwargs(book, use_map)?;
      }
      Stmt::InPlace { val, nxt, .. } => {
        val.order_kwargs(book, use_map)?;
        nxt.order_kwargs(book, use_map)?;
      }
      Stmt::If { cond, then, otherwise, nxt } => {
        cond.order_kwargs(book, use_map)?;
        then.order_kwargs(book, use_map)?;
        otherwise.order_kwargs(book, use_map)?;
        if let Some(nxt) = nxt {
          nxt.order_kwargs(book, use_map)?;
        }
      }
      Stmt::Match { arg, arms, nxt, .. } => {
        arg.order_kwargs(book, use_map)?;
        for arm in arms {
          arm.rgt.order_kwargs(book, use_map)?;
        }
        if let Some(nxt) = nxt {
          nxt.order_kwargs(book, use_map)?;
        }
      }
      Stmt::Switch { arg, arms, nxt, .. } => {
        arg.order_kwargs(book, use_map)?;
        for arm in arms {
          arm.order_kwargs(book, use_map)?;
        }
        if let Some(nxt) = nxt {
          nxt.order_kwargs(book, use_map)?;
        }
      }
      Stmt::Fold { arg, arms, nxt, .. } => {
        arg.order_kwargs(book, use_map)?;
        for arm in arms {
          arm.rgt.order_kwargs(book, use_map)?;
        }
        if let Some(nxt) = nxt {
          nxt.order_kwargs(book, use_map)?;
        }
      }
      Stmt::Bend { bnd: _, arg, cond, step, base, nxt } => {
        for arg in arg {
          arg.order_kwargs(book, use_map)?;
        }
        cond.order_kwargs(book, use_map)?;
        step.order_kwargs(book, use_map)?;
        base.order_kwargs(book, use_map)?;
        if let Some(nxt) = nxt {
          nxt.order_kwargs(book, use_map)?;
        }
      }
      Stmt::With { typ: _, bod, nxt } => {
        bod.order_kwargs(book, use_map)?;
        if let Some(nxt) = nxt {
          nxt.order_kwargs(book, use_map)?;
        }
      }
      Stmt::Open { typ: _, var: _, nxt } => {
        nxt.order_kwargs(book, use_map)?;
      }
      Stmt::Use { nam, val: bod, nxt } => {
        if let Expr::Var { nam: bod } = bod.as_ref() {
          use_map.insert(nam.clone(), bod.clone());
          nxt.order_kwargs(book, use_map)?;
          use_map.pop();
        } else {
          bod.order_kwargs(book, use_map)?;
          nxt.order_kwargs(book, use_map)?;
        }
      }
      Stmt::Return { term } => term.order_kwargs(book, use_map)?,
      Stmt::Err => {}
    }
    Ok(())
  }
}

impl Expr {
  fn order_kwargs(&mut self, book: &ParseBook, use_map: &mut IndexMap<Name, Name>) -> Result<(), String> {
    match self {
      // Named arguments are only allowed when directly calling a named function.
      Expr::Call { fun, args, kwargs } => {
        if !kwargs.is_empty() {
          if let Expr::Var { nam } = fun.as_ref() {
            if let Some(names) = get_args_def_or_ctr(nam, book, use_map) {
              go_order_kwargs(&names, args, kwargs)?;
            } else {
              return Err(format!(
                "Named args are only allowed when calling a named function, not when calling variable '{nam}'."
              ));
            }
          } else {
            // TODO: Print expression
            return Err(
              "Named args are only allowed when calling a named function, not when calling an expression."
                .to_string(),
            );
          }
        }
        fun.order_kwargs(book, use_map)?;
        for arg in args {
          arg.order_kwargs(book, use_map)?;
        }
        for (_, arg) in kwargs {
          arg.order_kwargs(book, use_map)?;
        }
      }
      Expr::Lam { bod, .. } => bod.order_kwargs(book, use_map)?,
      Expr::Opr { lhs, rhs, .. } => {
        lhs.order_kwargs(book, use_map)?;
        rhs.order_kwargs(book, use_map)?;
      }
      Expr::Lst { els } | Expr::Tup { els } | Expr::Sup { els } => {
        for el in els {
          el.order_kwargs(book, use_map)?;
        }
      }
      Expr::LstMap { term, iter, cond, .. } => {
        term.order_kwargs(book, use_map)?;
        iter.order_kwargs(book, use_map)?;
        if let Some(cond) = cond {
          cond.order_kwargs(book, use_map)?;
        }
      }
      Expr::Ctr { name, args, kwargs } => match get_args_def_or_ctr(name, book, use_map) {
        Some(names) => {
          go_order_kwargs(&names, args, kwargs)?;
          for arg in args {
            arg.order_kwargs(book, use_map)?;
          }
        }
        _ => return Err(format!("Constructor '{name}' not found.")),
      },
      Expr::Map { entries } => {
        for entry in entries {
          entry.1.order_kwargs(book, use_map)?;
        }
      }
      Expr::MapGet { nam: _, key } => {
        key.order_kwargs(book, use_map)?;
      }
      Expr::TreeNode { left, right } => {
        left.order_kwargs(book, use_map)?;
        right.order_kwargs(book, use_map)?;
      }
      Expr::TreeLeaf { val } => {
        val.order_kwargs(book, use_map)?;
      }
      Expr::Era | Expr::Var { .. } | Expr::Chn { .. } | Expr::Num { .. } | Expr::Str { .. } => {}
    }
    Ok(())
  }
}

fn go_order_kwargs(
  names: &[Name],
  args: &mut Vec<Expr>,
  kwargs: &mut Vec<(Name, Expr)>,
) -> Result<(), String> {
  if args.len() + kwargs.len() != names.len() {
    return Err(
      "Named args are only allowed when calling a function with the exact number of arguments.".to_string(),
    );
  }
  let mut kwargs: IndexMap<Name, Expr> = IndexMap::from_iter(kwargs.drain(..));
  let remaining_names = &names[args.len()..];
  for name in remaining_names {
    if let Some(arg) = kwargs.shift_remove(name) {
      args.push(arg);
    } else {
      return Err(format!("Named arg '{name}' is missing."));
    }
  }
  if let Some(name) = kwargs.keys().next() {
    return Err(format!("Unexpected named arg in function call {}.", name));
  }
  Ok(())
}

fn get_args_def_or_ctr(name: &Name, book: &ParseBook, use_map: &IndexMap<Name, Name>) -> Option<Vec<Name>> {
  let name = use_map.get(name).unwrap_or(name);

  #[allow(clippy::manual_map)]
  if let Some(adt_nam) = book.ctrs.get(name) {
    Some(book.adts[adt_nam].ctrs[name].iter().map(|f| f.nam.clone()).collect())
  } else if let Some(def) = book.fun_defs.get(name) {
    Some(def.rules[0].pats.iter().flat_map(|p| p.binds().flatten().cloned()).collect())
  } else {
    None
  }
}
