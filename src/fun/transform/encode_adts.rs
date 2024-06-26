use crate::{
  fun::{Book, Definition, Name, Num, Pattern, Rule, Term},
  AdtEncoding,
};

impl Book {
  /// Defines a function for each constructor in each ADT in the book.
  pub fn encode_adts(&mut self, adt_encoding: AdtEncoding) {
    let mut defs = vec![];
    let mut tags = vec![];

    for (adt_name, adt) in self.adts.iter() {
      for (ctr_idx, (ctr_name, fields)) in adt.ctrs.iter().enumerate() {
        let ctrs: Vec<_> = adt.ctrs.keys().cloned().collect();

        let body = match adt_encoding {
          AdtEncoding::Scott => encode_ctr_scott(fields.iter().map(|f| &f.nam), ctrs, ctr_name),
          AdtEncoding::NumScott => {
            let tag = make_tag(adt_name == ctr_name, ctr_name);
            let body = encode_ctr_num_scott(fields.iter().map(|f| &f.nam), &tag);
            let tag_def = make_tag_def(ctr_idx, &tag, adt.source.is_builtin());
            tags.push((tag, tag_def));
            body
          }
        };

        let rules = vec![Rule { pats: vec![], body }];
        let def = Definition::new(ctr_name.clone(), rules, adt.source.clone());
        defs.push((ctr_name.clone(), def));
      }
    }
    self.defs.extend(defs);
    self.defs.extend(tags);
  }
}

fn make_tag(is_object: bool, ctr_name: &Name) -> Name {
  if is_object {
    Name::new(format!("{ctr_name}/tag"))
  } else {
    let (typ, ctr) = ctr_name.rsplit_once('/').expect("To split at '/'");
    Name::new(format!("{typ}/{ctr}/tag"))
  }
}

fn encode_ctr_scott<'a>(
  ctr_args: impl DoubleEndedIterator<Item = &'a Name> + Clone,
  ctrs: Vec<Name>,
  ctr_name: &Name,
) -> Term {
  let ctr = Term::Var { nam: ctr_name.clone() };
  let app = Term::call(ctr, ctr_args.clone().cloned().map(|nam| Term::Var { nam }));
  let lam = Term::rfold_lams(app, ctrs.into_iter().map(Some));
  Term::rfold_lams(lam, ctr_args.cloned().map(Some))
}

fn encode_ctr_num_scott<'a>(ctr_args: impl DoubleEndedIterator<Item = &'a Name> + Clone, tag: &str) -> Term {
  let nam = Name::new("%x");
  // λa1 .. λan λx (x TAG a1 .. an)
  let term = Term::Var { nam: nam.clone() };
  let tag = Term::r#ref(tag);
  let term = Term::app(term, tag);
  let term = Term::call(term, ctr_args.clone().cloned().map(|nam| Term::Var { nam }));
  let term = Term::lam(Pattern::Var(Some(nam)), term);
  Term::rfold_lams(term, ctr_args.cloned().map(Some))
}

fn make_tag_def(ctr_idx: usize, tag: &Name, builtin: bool) -> Definition {
  let tag_rule = vec![Rule { pats: vec![], body: Term::Num { val: Num::U24(ctr_idx as u32) } }];
  Definition::new_gen(tag.clone(), tag_rule, builtin)
}
