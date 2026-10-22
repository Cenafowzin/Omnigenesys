use super::*;

fn error(text: &str) -> Diagnostic {
    let mut errors = parse(text).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    errors.remove(0)
}

fn at(d: &Diagnostic) -> (usize, usize) {
    let s = d.span.expect("located");
    (s.line, s.column)
}

#[test]
fn reads_plain_json() {
    let (value, _) = parse(r#"{"a": [1, -2, 0.5, true, null, "x"], "b": {}}"#).unwrap();
    assert_eq!(
        value,
        serde_json::json!({"a": [1, -2, 0.5, true, null, "x"], "b": {}})
    );
}

#[test]
fn every_extension_is_off() {
    for (text, pos) in [
        ("{\n  // note\n  \"a\": 1\n}", (2, 3)),
        ("{\"a\": 1,}", (1, 8)),
        ("{'a': 1}", (1, 2)),
        ("{a: 1}", (1, 2)),
        ("{\"a\": 0x1F}", (1, 7)),
        ("{\"a\": +1}", (1, 7)),
        ("{\"a\": .5}", (1, 7)),
        ("{\"a\": NaN}", (1, 7)),
        ("{\"a\": 1 \"b\": 2}", (1, 8)),
    ] {
        let d = error(text);
        assert_eq!(at(&d), pos, "{text}: {d}");
    }
}

#[test]
fn an_empty_file_is_an_error() {
    assert_eq!(error("  \n").message, "the file is empty");
}

#[test]
fn a_duplicate_key_points_at_both() {
    let text = "{\n  \"seed\": 1,\n  \"seed\": 2\n}";
    let d = error(text);
    assert_eq!(d.message, "duplicate key 'seed'");
    assert_eq!(d.pointer, "/seed");
    assert_eq!(at(&d), (3, 3));
    let note = d.note.unwrap();
    assert_eq!(note.span.map(|s| (s.line, s.column)), Some((2, 3)));
}

#[test]
fn every_duplicate_is_reported() {
    let errors = parse(r#"{"a": {"x": 1, "x": 2}, "b": 1, "b": 2}"#).unwrap_err();
    let pointers: Vec<_> = errors.iter().map(|d| d.pointer.as_str()).collect();
    assert_eq!(pointers, ["/a/x", "/b"]);
}

#[test]
fn spans_start_at_the_key_of_a_member() {
    let text = "{\n  \"steps\": [\n    {\"id\": \"a\"},\n    {\"id\": \"b\"}\n  ]\n}";
    let (_, spans) = parse(text).unwrap();
    let pos = |p: &str| spans.locate(p).map(|s| (s.line, s.column));
    assert_eq!(pos(""), Some((1, 1)));
    assert_eq!(pos("/steps"), Some((2, 3)));
    assert_eq!(pos("/steps/1"), Some((4, 5)));
    assert_eq!(pos("/steps/1/id"), Some((4, 6)));
    let member = spans.locate("/steps/1/id").unwrap();
    assert_eq!(&text[member.start..member.end], "\"id\": \"b\"");
}

#[test]
fn locate_falls_back_to_the_closest_parent() {
    let (_, spans) = parse("{\"config\": {\"tile\": \"x\"}}").unwrap();
    let parent = spans.locate("/config").unwrap();
    assert_eq!(spans.locate("/config/missing/deeper"), Some(parent));
    assert_eq!(spans.locate("/elsewhere").map(|s| s.start), Some(0));
}

#[test]
fn columns_count_characters_not_bytes() {
    let (_, spans) = parse("{\"é\": 1, \"b\": 2}").unwrap();
    // `é` is two bytes; the column after it is still 10.
    assert_eq!(spans.locate("/b").map(|s| s.column), Some(10));
}

#[test]
fn integers_stay_exact() {
    let (v, _) =
        parse(r#"[18446744073709551615, -9223372036854775808, 9007199254740993]"#).unwrap();
    assert_eq!(v[0].as_u64(), Some(u64::MAX));
    assert_eq!(v[1].as_i64(), Some(i64::MIN));
    // Above 2^53: f64 would have made it ...992.
    assert_eq!(v[2].as_u64(), Some(9_007_199_254_740_993));
}

#[test]
fn numbers_past_the_integers_become_floats() {
    let (v, _) = parse("[18446744073709551616, 1e3, 2.0]").unwrap();
    assert!(v[0].is_f64());
    assert_eq!(v[1].as_f64(), Some(1000.0));
    assert!(v[2].is_f64(), "2.0 is written as a float and stays one");
    assert_eq!(error("[1e400]").message, "number 1e400 is out of range");
}

#[test]
fn pointers_escape_tilde_and_slash() {
    let (_, spans) = parse(r#"{"a/b": {"c~d": 1}}"#).unwrap();
    assert!(spans.0.contains_key("/a~1b/c~0d"));
}

#[test]
fn serde_paths_become_pointers() {
    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    struct Room {
        size: u32,
    }
    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    struct Map {
        rooms: Vec<Room>,
    }
    let v = serde_json::json!({"rooms": [{"size": 1}, {"size": 2}, {"size": "big"}]});
    let e = serde_path_to_error::deserialize::<_, Map>(v).unwrap_err();
    assert_eq!(pointer(e.path()), "/rooms/2/size");
}
