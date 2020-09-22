use crate::graphql_utils::QueryVariables;
use crate::language::Captures;
use graphql_parser::query as q;
use num_bigint::BigInt;
use std::borrow::Borrow;
use std::collections::BTreeMap;

pub fn match_directives<'l, 'r, 'r2: 'r, 'f, 'f2: 'f>(
    _predicate: &q::Directive<'l, &'l str>,
    _query: &q::Directive<'r, &'r str>,
    _fragments: &'f [q::FragmentDefinition<'f2, &'f2 str>],
    _variables: &QueryVariables,
    _captures: &mut Captures,
) -> Result<bool, ()> {
    // TODO: Directives
    Err(())
}

pub fn match_selections<'l, 'r, 'r2: 'r, 'f, 'f2: 'f>(
    predicate: &q::Selection<'l, &'l str>,
    query: &q::Selection<'r, &'r str>,
    fragments: &'f [q::FragmentDefinition<'f2, &'f2 str>],
    variables: &QueryVariables,
    captures: &mut Captures,
) -> Result<bool, ()> {
    match (predicate, query) {
        (q::Selection::Field(predicate), q::Selection::Field(query)) => {
            match_fields(predicate, query, fragments, variables, captures)
        }
        (_, q::Selection::FragmentSpread(fragment_spread)) => {
            if fragment_spread.directives.len() != 0 {
                // TODO: Support definitions here
                return Err(());
            }
            let fragment_definition = fragments
                .iter()
                .find(|def| def.name == fragment_spread.fragment_name);
            if let Some(fragment_definition) = fragment_definition {
                if fragment_definition.directives.len() != 0 {
                    // TODO: Support definitions here
                    return Err(());
                }
                any_ok(
                    fragment_definition.selection_set.items.iter(),
                    |selection| {
                        match_selections(predicate, selection, fragments, variables, captures)
                    },
                )
            } else {
                return Err(());
            }
        }
        // TODO: Support all the things
        _ => Err(()),
    }
}

// Iterates over each item in 'iter' and returns:
// Ok(true) if f(item) => Ok(true)
// Err(e) if f(item) => Err(e)
// Ok(false) if the above conditions are not reached
fn any_ok<T: IntoIterator, Err>(
    iter: T,
    mut f: impl FnMut(T::Item) -> Result<bool, Err>,
) -> Result<bool, Err> {
    let iter = iter.into_iter();
    for item in iter {
        if f(item)? {
            return Ok(true);
        }
    }

    Ok(false)
}

fn match_fields<'l, 'r, 'r2: 'r, 'f, 'f2: 'f>(
    predicate: &q::Field<'l, &'l str>,
    query: &q::Field<'r, &'r str>,
    fragments: &'f [q::FragmentDefinition<'f2, &'f2 str>],
    variables: &QueryVariables,
    captures: &mut Captures,
) -> Result<bool, ()> {
    if predicate.name != query.name {
        return Ok(false);
    }
    for p_argument in predicate.arguments.iter() {
        if !any_ok(query.arguments.iter(), |q_argument| {
            match_named_value(p_argument, q_argument, variables, captures)
        })? {
            return Ok(false);
        }
    }

    for p_selection in predicate.selection_set.items.iter() {
        if !any_ok(query.selection_set.items.iter(), |q_selection| {
            match_selections(p_selection, q_selection, fragments, variables, captures)
        })? {
            return Ok(false);
        }
    }

    // TODO: Support alias?
    // TODO: Match directives

    return Ok(true);
}

fn match_named_value<
    'l,
    'r,
    AP: AsRef<str>,
    AQ: AsRef<str>,
    T: q::Text<'r>,
    VP: Borrow<q::Value<'l, &'l str>>,
    VQ: Borrow<q::Value<'r, T>>,
>(
    predicate: &(AP, VP),
    query: &(AQ, VQ),
    variables: &QueryVariables,
    captures: &mut Captures,
) -> Result<bool, ()> {
    if predicate.0.as_ref() != query.0.as_ref() {
        return Ok(false);
    }

    match_value(predicate.1.borrow(), query.1.borrow(), variables, captures)
}

fn match_value<'l, 'r, T: q::Text<'r>>(
    predicate: &q::Value<'l, &'l str>,
    query: &q::Value<'r, T>,
    variables: &QueryVariables,
    captures: &mut Captures,
) -> Result<bool, ()> {
    use q::Value::*;

    match (predicate, query) {
        (_, Variable(var)) => {
            if let Some(var) = variables.get(var.as_ref()) {
                match_value(predicate, var, variables, captures)
            } else {
                Err(())
            }
        }
        // TODO: Performance: Borrow keys in Captures
        (Variable(var), q) => match q {
            Int(q) => {
                // TODO: Handle larger numbers w/out panic
                captures.insert(*var, BigInt::from(q.as_i64().unwrap()));
                Ok(true)
            }
            Boolean(q) => {
                captures.insert(*var, *q);
                Ok(true)
            }
            _ => {
                // For now, match the variable but do not capture
                // because these types have no use in the expressions (yet)
                Ok(true)
            }
        },
        (Int(p), Int(q)) => Ok(p == q),
        (Float(p), Float(q)) => Ok(p == q),
        (String(p), String(q)) => Ok(p == q),
        (Boolean(p), Boolean(q)) => Ok(p == q),
        (Null, Null) => Ok(true),
        (Enum(p), Enum(q)) => Ok(*p == q.as_ref()),
        (List(p), List(q)) => match_list(p, q, variables, captures),
        (Object(p), Object(q)) => match_object(p, q, variables, captures),
        _ => Ok(false),
    }
}

fn match_list<'l, 'r, T: q::Text<'r>>(
    predicate: &Vec<q::Value<'l, &'l str>>,
    query: &Vec<q::Value<'r, T>>,
    variables: &QueryVariables,
    captures: &mut Captures,
) -> Result<bool, ()> {
    if predicate.len() != query.len() {
        return Ok(false);
    }

    for (p, q) in predicate.iter().zip(query.iter()) {
        if !(match_value(p, q, variables, captures))? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn match_object<'l, 'r, T: q::Text<'r>>(
    predicate: &BTreeMap<&'l str, q::Value<'l, &'l str>>,
    query: &BTreeMap<T::Value, q::Value<'r, T>>,
    variables: &QueryVariables,
    captures: &mut Captures,
) -> Result<bool, ()> {
    for p_arg in predicate.iter() {
        if !any_ok(query.iter(), |q_arg| {
            match_named_value(&p_arg, &q_arg, variables, captures)
        })? {
            return Ok(false);
        }
    }
    Ok(true)
}