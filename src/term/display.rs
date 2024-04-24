use super::{Book, Definition, FanKind, Name, Pattern, Rule, Tag, Term};
use crate::maybe_grow;
use std::{fmt, ops::Deref};

/* Some aux structures for things that are not so simple to display */

pub struct DisplayFn<F: Fn(&mut fmt::Formatter) -> fmt::Result>(pub F);

impl<F: Fn(&mut fmt::Formatter) -> fmt::Result> fmt::Display for DisplayFn<F> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.0(f)
  }
}

pub struct DisplayJoin<F, S>(pub F, pub S);

impl<F, I, S> fmt::Display for DisplayJoin<F, S>
where
  F: (Fn() -> I),
  I: IntoIterator,
  I::Item: fmt::Display,
  S: fmt::Display,
{
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    for (i, x) in self.0().into_iter().enumerate() {
      if i != 0 {
        self.1.fmt(f)?;
      }
      x.fmt(f)?;
    }
    Ok(())
  }
}

macro_rules! display {
  ($($x:tt)*) => {
    DisplayFn(move |f| write!(f, $($x)*))
  };
}

/* The actual display implementations */

impl fmt::Display for Term {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    maybe_grow(|| match self {
      Term::Lam { tag, pat, bod } => {
        write!(f, "{}λ{} {}", tag.display_padded(), pat, bod)
      }
      Term::Var { nam } => write!(f, "{nam}"),
      Term::Lnk { nam } => write!(f, "${nam}"),
      Term::Let { pat, val, nxt } => {
        write!(f, "let {} = {}; {}", pat, val, nxt)
      }
      Term::Bnd { fun, ask, val, nxt } => {
        write!(f, "do {fun} {{ ")?;
        write!(f, "ask {} = {}; ", ask, val)?;
        let mut cur = nxt;
        while let Term::Bnd { fun: _, ask, val, nxt } = &**cur {
          cur = nxt;
          write!(f, "ask {} = {}; ", ask, val)?;
        }
        write!(f, "{} }}", cur)
      }
      Term::Use { nam, val, nxt } => {
        let Some(nam) = nam else { unreachable!() };
        write!(f, "use {} = {}; {}", nam, val, nxt)
      }
      Term::Ref { nam: def_name } => write!(f, "{def_name}"),
      Term::App { tag, fun, arg } => {
        write!(f, "{}({} {})", tag.display_padded(), fun.display_app(tag), arg)
      }
      Term::Mat { arg, bnd, with, arms } => {
        write!(f, "match ")?;
        if let Some(bnd) = bnd {
          write!(f, "{} = ", bnd)?;
        }
        write!(f, "{} ", arg)?;
        if !with.is_empty() {
          write!(f, "with {} ", DisplayJoin(|| with, ", "))?;
        }
        write!(f, "{{ ")?;
        for arm in arms {
          write!(f, "{}", var_as_str(&arm.0))?;
          for var in &arm.1 {
            write!(f, " {}", var_as_str(var))?;
          }
          write!(f, ": {}; ", arm.2)?;
        }
        write!(f, "}}")
      }
      Term::Swt { arg, bnd, with, pred, arms } => {
        write!(f, "switch ")?;
        if let Some(bnd) = bnd {
          write!(f, "{bnd} = ")?;
        }
        write!(f, "{arg} ")?;
        if !with.is_empty() {
          write!(f, "with {} ", DisplayJoin(|| with, ", "))?;
        }
        write!(f, "{{ ")?;
        for (i, arm) in arms.iter().enumerate() {
          if i == arms.len() - 1 {
            write!(f, "_")?;
            if let Some(pred) = pred {
              write!(f, " {pred}")?;
            }
          } else {
            write!(f, "{i}")?;
          }
          write!(f, ": {arm}; ")?;
        }
        write!(f, "}}")
      }
      Term::Fan { fan: FanKind::Tup, tag, els } => write!(f, "{}({})", tag, DisplayJoin(|| els.iter(), ", ")),
      Term::Fan { fan: FanKind::Dup, tag, els } => write!(f, "{}{{{}}}", tag, DisplayJoin(|| els, " ")),
      Term::Era => write!(f, "*"),
      Term::Num { val } => write!(f, "{val}"),
      Term::Nat { val } => write!(f, "#{val}"),
      Term::Str { val } => write!(f, "{val:?}"),
      Term::Opx { opr, fst, snd } => {
        write!(f, "({} {} {})", opr, fst, snd)
      }
      Term::Lst { els } => write!(f, "[{}]", DisplayJoin(|| els.iter(), ", "),),
      Term::Err => write!(f, "<Invalid>"),
    })
  }
}

impl fmt::Display for Tag {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Tag::Named(name) => write!(f, "#{name}"),
      Tag::Numeric(num) => write!(f, "#{num}"),
      Tag::Auto => Ok(()),
      Tag::Static => Ok(()),
    }
  }
}

impl fmt::Display for Pattern {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Pattern::Var(None) => write!(f, "*"),
      Pattern::Var(Some(nam)) => write!(f, "{nam}"),
      Pattern::Chn(nam) => write!(f, "${nam}"),
      Pattern::Ctr(nam, pats) => {
        write!(f, "({}{})", nam, DisplayJoin(|| pats.iter().map(|p| display!(" {p}")), ""))
      }
      Pattern::Num(num) => write!(f, "{num}"),
      Pattern::Fan(FanKind::Tup, tag, pats) => write!(f, "{}({})", tag, DisplayJoin(|| pats, ", ")),
      Pattern::Fan(FanKind::Dup, tag, pats) => write!(f, "{}{{{}}}", tag, DisplayJoin(|| pats, " ")),
      Pattern::Lst(pats) => write!(f, "[{}]", DisplayJoin(|| pats, ", ")),
      Pattern::Str(str) => write!(f, "\"{str}\""),
      Pattern::Err => write!(f, "<Invalid>"),
    }
  }
}

impl Rule {
  pub fn display<'a>(&'a self, def_name: &'a Name) -> impl fmt::Display + 'a {
    display!(
      "({}{}) = {}",
      def_name,
      DisplayJoin(|| self.pats.iter().map(|x| display!(" {x}")), ""),
      self.body
    )
  }
}

impl fmt::Display for Definition {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", DisplayJoin(|| self.rules.iter().map(|x| x.display(&self.name)), "\n"))
  }
}

impl fmt::Display for Book {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", DisplayJoin(|| self.defs.values(), "\n\n"))
  }
}

impl fmt::Display for Name {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.0.fmt(f)
  }
}

impl Term {
  fn display_app<'a>(&'a self, tag: &'a Tag) -> impl fmt::Display + 'a {
    maybe_grow(|| {
      DisplayFn(move |f| match self {
        Term::App { tag: tag2, fun, arg } if tag2 == tag => {
          write!(f, "{} {}", fun.display_app(tag), arg)
        }
        _ => write!(f, "{}", self),
      })
    })
  }
}

impl Tag {
  pub fn display_padded(&self) -> impl fmt::Display + '_ {
    DisplayFn(move |f| match self {
      Tag::Named(name) => write!(f, "#{name} "),
      Tag::Numeric(num) => write!(f, "#{num} "),
      Tag::Auto => Ok(()),
      Tag::Static => Ok(()),
    })
  }
}

fn var_as_str(nam: &Option<Name>) -> &str {
  nam.as_ref().map_or("*", Name::deref)
}

/* Pretty printing  */

impl Book {
  pub fn display_pretty(&self) -> impl fmt::Display + '_ {
    display!("{}", DisplayJoin(|| self.defs.values().map(|def| def.display_pretty()), "\n\n"))
  }
}

impl Definition {
  pub fn display_pretty(&self) -> impl fmt::Display + '_ {
    display!("{}", DisplayJoin(|| self.rules.iter().map(|x| x.display_pretty(&self.name)), "\n"))
  }
}

impl Rule {
  pub fn display_pretty<'a>(&'a self, def_name: &'a Name) -> impl fmt::Display + 'a {
    display!(
      "({}{}) =\n  {}",
      def_name,
      DisplayJoin(|| self.pats.iter().map(|x| display!(" {x}")), ""),
      self.body.display_pretty(2)
    )
  }
}

impl Term {
  pub fn display_pretty(&self, tab: usize) -> impl fmt::Display + '_ {
    maybe_grow(|| {
      DisplayFn(move |f| match self {
        Term::Lam { tag, pat, bod } => {
          write!(f, "{}λ{} {}", tag.display_padded(), pat, bod.display_pretty(tab))
        }

        Term::Var { nam } => write!(f, "{nam}"),

        Term::Lnk { nam } => write!(f, "${nam}"),

        Term::Let { pat, val, nxt } => {
          write!(f, "let {} = {};\n{:tab$}{}", pat, val.display_pretty(tab), "", nxt.display_pretty(tab))
        }

        Term::Bnd { fun, ask, val, nxt } => {
          writeln!(f, "do {fun} {{")?;
          writeln!(f, "{:tab$}ask {} = {};", "", ask, val.display_pretty(tab + 2), tab = tab + 2)?;
          let mut cur = nxt;
          while let Term::Bnd { fun: _, ask, val, nxt } = &**cur {
            cur = nxt;
            writeln!(f, "{:tab$}ask {} = {};", "", ask, val.display_pretty(tab + 2), tab = tab + 2)?;
          }
          writeln!(f, "{:tab$}{}", "", cur.display_pretty(tab + 2), tab = tab + 2)?;
          writeln!(f, "{:tab$}}}", "")
        }

        Term::Use { nam, val, nxt } => {
          write!(
            f,
            "use {} = {};\n{:tab$}{}",
            var_as_str(nam),
            val.display_pretty(tab),
            "",
            nxt.display_pretty(tab)
          )
        }

        Term::App { tag, fun, arg } => {
          write!(
            f,
            "{}({} {})",
            tag.display_padded(),
            fun.display_app_pretty(tag, tab),
            arg.display_pretty(tab)
          )
        }

        Term::Fan { fan: FanKind::Tup, tag, els } => {
          write!(f, "{}({})", tag, DisplayJoin(|| els.iter().map(|e| e.display_pretty(tab)), ", "))
        }

        Term::Fan { fan: FanKind::Dup, tag, els } => {
          write!(
            f,
            "{}{{{}}}",
            tag.display_padded(),
            DisplayJoin(|| els.iter().map(|e| e.display_pretty(tab)), " ")
          )
        }

        Term::Lst { els } => {
          write!(f, "[{}]", DisplayJoin(|| els.iter().map(|e| e.display_pretty(tab)), " "))
        }

        Term::Opx { opr, fst, snd } => {
          write!(f, "({} {} {})", opr, fst.display_pretty(tab), snd.display_pretty(tab))
        }

        Term::Mat { bnd, arg, with, arms } => {
          write!(f, "match ")?;
          if let Some(bnd) = bnd {
            write!(f, "{} = ", bnd)?;
          }
          write!(f, "{} ", arg.display_pretty(tab))?;
          if !with.is_empty() {
            write!(f, "with {} ", DisplayJoin(|| with, ", "))?;
          }
          write!(f, "{{ ")?;
          for arm in arms {
            write!(f, "\n{:tab$}{}", "", var_as_str(&arm.0), tab = tab + 2)?;
            for var in &arm.1 {
              write!(f, " {}", var_as_str(var))?;
            }
            write!(f, ": {}; ", arm.2.display_pretty(tab + 4))?;
          }
          write!(f, "\n{:tab$}}}", "")
        }

        Term::Swt { bnd, arg, with, pred, arms } => {
          write!(f, "switch ")?;
          if let Some(bnd) = bnd {
            write!(f, "{bnd} = ")?;
          }
          write!(f, "{} ", arg.display_pretty(tab))?;
          if !with.is_empty() {
            write!(f, "with {} ", DisplayJoin(|| with, ", "))?;
          }
          writeln!(f, "{{")?;
          for (i, arm) in arms.iter().enumerate() {
            if i == arms.len() - 1 {
              write!(f, "{:tab$}_", "", tab = tab + 2)?;
              if let Some(pred) = pred {
                write!(f, " {pred}")?;
              }
            } else {
              write!(f, "{:tab$}{i}", "", tab = tab + 2)?;
            }
            writeln!(f, ": {};", arm.display_pretty(tab + 4))?;
          }
          write!(f, "{:tab$}}}", "")
        }

        Term::Nat { val } => write!(f, "#{val}"),
        Term::Num { val } => write!(f, "{val}"),
        Term::Str { val } => write!(f, "{val:?}"),
        Term::Ref { nam } => write!(f, "{nam}"),
        Term::Era => write!(f, "*"),
        Term::Err => write!(f, "<Error>"),
      })
    })
  }

  fn display_app_pretty<'a>(&'a self, tag: &'a Tag, tab: usize) -> impl fmt::Display + 'a {
    maybe_grow(|| {
      DisplayFn(move |f| match self {
        Term::App { tag: tag2, fun, arg } if tag2 == tag => {
          write!(f, "{} {}", fun.display_app_pretty(tag, tab), arg.display_pretty(tab))
        }
        _ => write!(f, "{}", self.display_pretty(tab)),
      })
    })
  }
}
