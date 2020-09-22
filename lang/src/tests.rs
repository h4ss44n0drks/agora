use crate::*;
use num_bigint::BigInt;

trait IntoTestResult {
    fn into(self) -> Result<BigInt, CostError>;
}

impl IntoTestResult for u64 {
    fn into(self) -> Result<BigInt, CostError> {
        Ok(BigInt::from(self))
    }
}

impl IntoTestResult for CostError {
    fn into(self) -> Result<BigInt, CostError> {
        Err(self)
    }
}

fn test(model: &str, query: &str, variables: &str, result: impl IntoTestResult) {
    let model = CostModel::compile(model).unwrap();
    let cost = model.cost(query, variables);
    assert_eq!(result.into(), cost);
}

#[test]
fn query_match() {
    let model = "
        query { a } when true => 11;
        query { b } when false => 12;
        query { b } when 1 == 1 => 2 + 2;
        # Never used, because the above matches the same conditions. 
        query { b } when true => 7;
    ";
    test(model, "query { a }", "", 11);
    test(model, "query { b }", "", 4);
}

#[test]
fn field_args() {
    let model = "
        query { a(skip: 10) } => 15;
        query { a(skip: $skip) } when $skip > 10 => $skip * (2 + 0);
        query { a } => 55;
        # This is a comment
        query { b(skip: $skip, bob: $bob) } when $skip == $bob && true => $bob; # This is also a comment
        query { b } => 99;
    ";
    test(model, "query { a(skip: 10) }", "", 15);
    test(model, "query { a(skip: 11) }", "", 22);
    test(model, "query { a(skip: 9) }", "", 55);
    test(model, "query { a }", "", 55);
    test(model, "query { b }", "", 99);
    test(model, "query { b(skip: 9) }", "", 99);
    test(model, "query { b(skip: 9, bob: 10) }", "", 99);
    test(model, "query { b(skip: 10, bob: 10) }", "", 10);
    test(model, "query { b(skip: 10, bob: 10), a }", "", 65);
}

#[test]
fn sums_top_levels() {
    let model = "
        query { a(skip: $skip) } => $skip;
        query { b(bob: $bob) } => 10;
        query { c } => 9;
        query { a } => 99;
        query { d } => 1;
    ";
    test(
        model,
        "query { a(skip: 10), b }",
        "",
        CostError::QueryNotCosted,
    );
    test(model, "query { a(skip: 10), b(bob: 5) }", "", 20);
    test(model, "query { a, c, d }", "", 109);
}

#[test]
fn var_substitutions() {
    let query = "query pairs($skip: Int!) { pairs(skip: $skip) { id } }";
    let variables = "{\"skip\":1}";
    let model = "query { pairs(skip: $k) } => $k;";

    test(model, query, variables, 1);
}

#[test]
fn default() {
    let query = "query { nonsense }";
    let model = "query { abc } => 2; default => 10;";
    test(model, query, "", 10);
}

#[test]
fn matching_object() {
    let model = "
        query { a(where: { age_gt: 18 }) } => 1;
        query { a(where: $where) } => 2;
        default => 3;
    ";

    test(model, "query { a(where: { age_gt: 18, non: 1 }) }", "", 1);
    test(model, "query { a(where: { age_gt: 21 }) }", "", 2);
    test(model, "query { a }", "", 3);
}

#[test]
fn matching_list() {
    let model = "
        query { a(val_in: [1, 2]) } => 1;
        query { a(val_in: $in) } => 2;
        default => 3;
    ";

    test(model, "query { a(val_in: [1, 2]) }", "", 1);
    test(model, "query { a(val_in: [2, 3]) }", "", 2);
    test(model, "query { a }", "", 3);
}

#[test]
fn fragments() {
    let model = "
        query { pairs(skip: $skip) { id reserveUSD } } => 1;
        query { pairs(skip: $skip) { id } } => 2;
        default => 3;
    ";

    let query_1 = "
        {
            pairs(skip: 1) { ...fields }
        }
        fragment fields on Name {
            id, reserveUSD
        }
    ";

    let query_2 = "
        {
            pairs(skip: 1) { ...fields }
        }
        fragment fields on Name {
            id
        }
    ";

    let query_3 = "
        {
            pairs(skip: 1) { ...fields }
        }
        fragment fields on Name {
            reserveUSD
        }
    ";

    test(model, query_1, "", 1);
    test(model, query_2, "", 2);
    test(model, query_3, "", 3);
}

#[test]
fn invalid_query() {
    test("default => 1;", "blah", "", CostError::FailedToParseQuery);
}

#[test]
fn invalid_variables() {
    test(
        "default => 1;",
        "query { a }",
        "blah",
        CostError::FailedToParseVariables,
    );
}

#[test]
fn invalid_model() {
    // Missing semicolon
    let model = "query { a } => 1 query { b } => 2;";
    assert!(CostModel::compile(model).is_err());
}

#[test]
fn nested_query() {
    let model = "
        query { users { id tokens { id } } } => 1;
        query { users { id } } => 2;
        default => 3;
    ";
    test(model, "query { users { id tokens { id } } }", "", 1);
    test(model, "query { users { id tokens { id and } } }", "", 1);
    test(model, "query { users { id tokens { and } } }", "", 2);
    test(
        model,
        "query { we { are { the { knights { who { say { ni } } } } } } }",
        "",
        3,
    );
}

#[test]
fn query_not_costed() {
    test("query { a } => 2;", "{ b }", "", CostError::QueryNotCosted);
}