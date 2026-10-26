use super::*;

const PARAMS: &[(&str, Value)] = &[];

/// Parses, checks against `params` and evaluates.
fn run_with(text: &str, params: &[(&str, Value)]) -> Result<Value, String> {
    let e = Expr::parse(text).map_err(|e| e.message)?;
    let types: Vec<(&str, Type)> = params.iter().map(|(n, v)| (*n, v.kind())).collect();
    e.check(&types).map_err(|es| {
        es.iter()
            .map(|e| e.message.clone())
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    e.eval(&|n| params.iter().find(|(p, _)| *p == n).map(|(_, v)| v.clone()))
        .map_err(|e| e.message)
}

fn run(text: &str) -> Result<Value, String> {
    run_with(text, PARAMS)
}

fn int(text: &str) -> i64 {
    match run(text) {
        Ok(Value::Integer(n)) => n,
        other => panic!("{text}: {other:?}"),
    }
}

fn num(text: &str) -> f64 {
    match run(text) {
        Ok(Value::Number(n)) => n,
        other => panic!("{text}: {other:?}"),
    }
}

fn boolean(text: &str) -> bool {
    match run(text) {
        Ok(Value::Boolean(b)) => b,
        other => panic!("{text}: {other:?}"),
    }
}

/// The error, which must contain `part`.
fn fails(text: &str, part: &str) {
    match run(text) {
        Err(m) => assert!(m.contains(part), "{text}: '{m}' lacks '{part}'"),
        Ok(v) => panic!("{text} gave {v:?}, expected an error with '{part}'"),
    }
}

#[test]
fn isaac_room_count() {
    // Rebirth's target, before its rand(0, 1): 5 + ⌊stage·10/3⌋, at most 20.
    let rooms = |stage: i64| {
        run_with(
            "min(20, 5 + floor(stage * 10 / 3))",
            &[("stage", Value::Integer(stage))],
        )
    };
    let want = [8, 11, 15, 18, 20, 20];
    for (stage, want) in (1..=6).zip(want) {
        assert_eq!(rooms(stage), Ok(Value::Integer(want)), "stage {stage}");
    }
}

#[test]
fn precedence_and_associativity() {
    assert_eq!(int("2 + 3 * 4"), 14);
    assert_eq!(int("(2 + 3) * 4"), 20);
    assert_eq!(int("10 - 3 - 2"), 5);
    assert_eq!(num("12 / 3 / 2"), 2.0);
    // Unary minus binds tighter than * and +, looser than nothing else.
    assert_eq!(int("-2 * 3"), -6);
    assert_eq!(int("2 - -3"), 5);
    assert_eq!(int("--4"), 4);
    assert!(boolean("1 + 1 == 2 and not 3 < 2"));
    // and before or.
    assert!(boolean("true or false and false"));
    assert!(!boolean("(true or false) and false"));
    // not takes the whole comparison.
    assert!(boolean("not 1 == 2"));
}

#[test]
fn integers_and_numbers() {
    // / always gives a number; div and mod are floored.
    assert_eq!(num("7 / 2"), 3.5);
    assert_eq!(run("6 / 3"), Ok(Value::Number(2.0)));
    assert_eq!(int("div(7, 2)"), 3);
    assert_eq!(int("div(-7, 2)"), -4);
    assert_eq!(int("mod(-7, 2)"), 1);
    assert_eq!(int("mod(7, -2)"), -1);
    for (a, b) in [(7, 3), (-7, 3), (7, -3), (-7, -3), (0, 5)] {
        let q = int(&format!("div({a}, {b})"));
        let r = int(&format!("mod({a}, {b})"));
        assert_eq!(q * b + r, a, "{a} {b}");
    }
    // An integer meeting a number becomes one.
    assert_eq!(num("1 + 0.5"), 1.5);
    assert_eq!(run("2 * 3"), Ok(Value::Integer(6)));
    assert_eq!(run("2.0 * 3"), Ok(Value::Number(6.0)));
    // Rounding gives integers; round goes half away from zero.
    assert_eq!(int("floor(2.7)"), 2);
    assert_eq!(int("floor(-2.5)"), -3);
    assert_eq!(int("ceil(2.1)"), 3);
    assert_eq!(int("trunc(-2.7)"), -2);
    assert_eq!(int("round(2.5)"), 3);
    assert_eq!(int("round(-2.5)"), -3);
    assert_eq!(int("round(0.49999999999999994)"), 0);
    assert_eq!(int("floor(4)"), 4);
    // Comparisons across the two.
    assert!(boolean("2 == 2.0"));
    assert!(boolean("3 > 2.5"));
}

#[test]
fn functions() {
    assert_eq!(int("min(4, 2, 9)"), 2);
    assert_eq!(int("max(4, 2, 9)"), 9);
    assert_eq!(num("max(1, 2.5)"), 2.5);
    assert_eq!(run("min(1, 2.5)"), Ok(Value::Number(1.0)));
    assert_eq!(int("clamp(12, 0, 10)"), 10);
    assert_eq!(num("clamp(0.25, 0.5, 1)"), 0.5);
    assert_eq!(int("abs(-3)"), 3);
    assert_eq!(num("abs(-0.5)"), 0.5);
    assert_eq!(int("pow(3, 4)"), 81);
    assert_eq!(int("pow(2, 0)"), 1);
    assert_eq!(num("pow(0.5, 3)"), 0.125);
    assert_eq!(num("sqrt(2.25)"), 1.5);
    assert_eq!(num("lerp(10, 20, 0.25)"), 12.5);
    assert_eq!(int("if(2 > 1, 5, 6)"), 5);
    assert_eq!(run("if(true, \"a\", \"b\")"), Ok(Value::String("a".into())));
    assert!(boolean("\"forest\" == \"forest\""));
    assert!(boolean("\"a\\\"b\" != \"ab\""));
}

#[test]
fn if_and_logic_look_only_where_needed() {
    // The branch not taken is never evaluated: no division by zero.
    assert_eq!(
        run_with("if(n == 0, 0, 10 / n)", &[("n", Value::Integer(0))]),
        Ok(Value::Number(0.0)),
        "an integer branch next to a number one gives a number"
    );
    assert!(!boolean("false and 1 / 0 > 1"));
    assert!(boolean("true or 1 / 0 > 1"));
    // The other branch's type comes from the params, not from evaluating it.
    let p = [("a", Value::Integer(3)), ("b", Value::Number(0.5))];
    assert_eq!(run_with("if(true, a, b)", &p), Ok(Value::Number(3.0)));
    assert_eq!(run_with("if(true, a, a)", &p), Ok(Value::Integer(3)));
}

#[test]
fn nothing_breaks_silently() {
    fails("1 / 0", "division by zero");
    fails("1.5 / 0", "division by zero");
    fails("div(1, 0)", "division by zero");
    fails("mod(1, 0)", "division by zero");
    fails("9223372036854775807 + 1", "too large for an integer");
    fails("-9223372036854775807 - 2", "too large for an integer");
    fails("div(-9223372036854775807 - 1, -1)", "too large");
    fails("abs(-9223372036854775807 - 1)", "too large");
    fails("pow(2, 63)", "too large");
    fails("pow(1e200, 2)", "too large");
    fails("1e308 * 10", "too large");
    fails("floor(1e19)", "too large");
    fails("sqrt(-1)", "below zero");
    fails("pow(2, -1)", "0 or more");
    fails("clamp(5, 10, 0)", "above high");
    fails("99999999999999999999", "too large for an integer");
    fails("1e400", "too large");
    fails("9007199254740993 + 0.5", "2^53");
    // -0 is 0: min and equality never see the sign of zero.
    assert_eq!(num("-0.0").to_bits(), 0.0f64.to_bits());
    assert_eq!(num("0.0 * -1").to_bits(), 0.0f64.to_bits());
}

#[test]
fn types_are_checked_before_anything_runs() {
    fails("1 + true", "needs numbers");
    fails("\"a\" < \"b\"", "needs numbers");
    fails("1 == \"a\"", "cannot be compared");
    fails("not 3", "needs a boolean");
    fails("1 and true", "joins booleans");
    fails("if(1, 2, 3)", "needs a boolean first");
    fails("if(true, 1, \"a\")", "of one type");
    fails("div(7.0, 2)", "takes integers");
    fails("pow(2, 0.5)", "whole exponent");
    fails("-true", "needs a number");
    fails("floor(\"a\")", "needs a number");
    // Every type error at once.
    let e = Expr::parse("(1 + true) * (\"a\" - 2)").unwrap();
    assert_eq!(e.check(&[]).unwrap_err().len(), 2);
}

#[test]
fn names_resolve_to_params() {
    let p = [("stage", Value::Integer(2)), ("bonus", Value::Number(0.5))];
    assert_eq!(run_with("stage + bonus", &p), Ok(Value::Number(2.5)));
    let e = Expr::parse("stage * stagee").unwrap();
    let names: Vec<&str> = e.names().iter().map(|(n, _)| *n).collect();
    assert_eq!(names, ["stage", "stagee"]);
    let err = e.check(&[("stage", Type::Integer)]).unwrap_err();
    assert_eq!(err.len(), 1);
    assert_eq!(
        err[0].message,
        "no param named 'stagee' (did you mean stage?)"
    );
    assert_eq!(err[0].span, 8..14, "points at the name");
}

#[test]
fn mistakes_from_other_languages_get_a_hint() {
    fails("2 ^ 3", "pow(base, exponent)");
    fails("2 ** 3", "pow(base, exponent)");
    fails("a = 2", "==");
    fails("1 <> 2", "!=");
    fails("true && false", "and");
    fails("true || false", "or");
    fails("!true", "not");
    fails("min(1; 2)", "with ,");
    fails("x > 1 ? 2 : 3", "if(condition, then, else)");
    fails("7 % 2", "mod(a, b)");
    fails("1 < 2 < 3", "do not chain");
    fails("2,5", "decimal point");
    fails("min(2,5 1)", "decimal");
    fails(".5", "0.5");
    fails("2 x", "operator is missing");
    fails("2x", "runs into a letter");
    fails("flor(2.5)", "did you mean floor?");
    fails("floor", "is a function");
    fails("floor(1, 2)", "takes 1 argument, got 2");
    fails("min(1)", "at least 2");
    fails("(1 + 2", "never closed");
    fails("1 + 2)", "closes nothing");
    fails("min(1, 2", "never closed");
    fails("1 +", "ends too soon");
    fails("", "empty");
    fails("\"open", "never closed");
    fails("é", "unexpected character 'é'");
    // Parenthesised comparisons are fine.
    assert!(boolean("(1 < 2) == true"));
}

#[test]
fn spans_point_at_the_mistake() {
    let at = |text: &str| Expr::parse(text).unwrap_err().span;
    assert_eq!(at("1 + 2 ^ 3"), 6..7);
    assert_eq!(at("1 < 2 < 3"), 6..7);
    assert_eq!(at("floor(1, 2)"), 0..11);
    let e = Expr::parse("1 + (true)").unwrap();
    assert_eq!(e.check(&[]).unwrap_err()[0].span, 0..10);
}

#[test]
fn bounded() {
    let deep = format!("{}1{}", "(".repeat(40), ")".repeat(40));
    fails(&deep, "nested more than");
    let long = format!("1{}", " + 1".repeat(300));
    fails(&long, "more than");
    let bytes = format!("1{}", " ".repeat(MAX_LEN));
    fails(&bytes, "bytes");
    let args = format!("min({})", vec!["1"; 40].join(", "));
    fails(&args, "more than 32 arguments");
    // At the limits it still works.
    let ok = format!("{}1{}", "(".repeat(20), ")".repeat(20));
    assert_eq!(int(&ok), 1);
}

#[test]
fn the_same_bits_everywhere() {
    // Golden values: these must never change (a map made from them would).
    // Only exactly rounded operations, so native and wasm agree; the wasm
    // test `same_map.rs` checks a pipeline that uses them.
    let golden = [
        ("1 / 3", 0x3FD5_5555_5555_5555u64),
        ("sqrt(2)", 0x3FF6_A09E_667F_3BCD),
        ("pow(1.1, 10)", 0x4004_BFFC_0C03_023D),
        ("lerp(0.1, 0.7, 0.3)", 0x3FD1_EB85_1EB8_51EC),
    ];
    for (text, bits) in golden {
        assert_eq!(num(text).to_bits(), bits, "{text}: {}", num(text));
    }
}
