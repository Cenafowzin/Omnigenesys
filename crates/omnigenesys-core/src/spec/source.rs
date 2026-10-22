//! Reading the file: strict JSON to a `serde_json::Value`, plus where in the
//! text each value was (PLANNING §2.1, items 1 and 2).
//!
//! Deserializing from a `Value` loses positions, so the file is read once
//! into a `Value` and an index from JSON Pointer to span; an error found
//! later (by serde, by the build) is placed through its pointer. This is
//! how cargo reports errors in `Cargo.toml`.

use std::collections::HashMap;
use std::fmt::Write;

use jsonc_parser::ast::{self, ObjectPropName};
use jsonc_parser::common::{Range, Ranged};
use jsonc_parser::{CollectOptions, CommentCollectionStrategy, ParseOptions, parse_to_ast};
use serde_json::{Map, Number, Value};

use super::{Diagnostic, Span};

/// Every JSON extension off: what is accepted is exactly JSON.
const STRICT: ParseOptions = ParseOptions {
    allow_comments: false,
    allow_loose_object_property_names: false,
    allow_trailing_commas: false,
    allow_missing_commas: false,
    allow_single_quoted_strings: false,
    allow_hexadecimal_numbers: false,
    allow_unary_plus_numbers: false,
    allow_bare_decimal_point_numbers: false,
    allow_non_finite_numbers: false,
    allow_extended_string_escapes: false,
};

/// Where each value of the file is, by JSON Pointer. An object member's
/// span starts at its key, so an error points at `"chance": ...`.
#[derive(Debug, Clone, Default)]
pub(crate) struct Spans(HashMap<String, Span>);

impl Spans {
    /// The span of `pointer`, or of its closest parent in the file (a
    /// pointer into a value the file did not write, like a default).
    pub(crate) fn locate(&self, pointer: &str) -> Option<Span> {
        let mut p = pointer;
        loop {
            if let Some(span) = self.0.get(p) {
                return Some(*span);
            }
            p = &p[..p.rfind('/')?];
        }
    }
}

/// Parses `text` as strict JSON. A duplicate key is an error: a `Value`
/// would keep the last one and drop the first without a word.
pub(crate) fn parse(text: &str) -> Result<(Value, Spans), Vec<Diagnostic>> {
    let collect = CollectOptions {
        comments: CommentCollectionStrategy::Off,
        tokens: false,
    };
    let parsed = parse_to_ast(text, &collect, &STRICT).map_err(|e| {
        let r = e.range();
        let mut d = Diagnostic::new("", e.kind().to_string());
        d.span = Some(Span {
            start: r.start,
            end: r.end,
            line: e.line_display(),
            column: e.column_display(),
        });
        vec![d]
    })?;
    let Some(root) = parsed.value else {
        return Err(vec![Diagnostic::new("", "the file is empty")]);
    };
    let mut reader = Reader {
        lines: Lines::new(text),
        spans: Spans::default(),
        errors: Vec::new(),
    };
    let mut pointer = String::new();
    reader.record(&pointer, root.range());
    let value = reader.convert(root, &mut pointer);
    if reader.errors.is_empty() {
        Ok((value, reader.spans))
    } else {
        for d in &mut reader.errors {
            d.locate(&reader.spans);
        }
        Err(reader.errors)
    }
}

struct Reader<'t> {
    lines: Lines<'t>,
    spans: Spans,
    errors: Vec<Diagnostic>,
}

impl Reader<'_> {
    fn record(&mut self, pointer: &str, range: Range) {
        let span = self.lines.span(range);
        self.spans.0.insert(pointer.to_string(), span);
    }

    /// `pointer` is the node's own; it is restored before returning.
    fn convert(&mut self, node: ast::Value<'_>, pointer: &mut String) -> Value {
        match node {
            ast::Value::NullKeyword(_) => Value::Null,
            ast::Value::BooleanLit(b) => Value::Bool(b.value),
            ast::Value::StringLit(s) => Value::String(s.value.into_owned()),
            ast::Value::NumberLit(n) => match number(n.value) {
                Some(n) => Value::Number(n),
                None => {
                    self.errors.push(Diagnostic::new(
                        pointer.as_str(),
                        format!("number {} is out of range", n.value),
                    ));
                    Value::Null
                }
            },
            ast::Value::Array(array) => {
                let mut items = Vec::with_capacity(array.elements.len());
                for (i, element) in array.elements.into_iter().enumerate() {
                    let len = pointer.len();
                    push_index(pointer, i);
                    self.record(pointer, element.range());
                    items.push(self.convert(element, pointer));
                    pointer.truncate(len);
                }
                Value::Array(items)
            }
            ast::Value::Object(object) => {
                let mut map = Map::with_capacity(object.properties.len());
                for prop in object.properties {
                    let key = match prop.name {
                        ObjectPropName::String(s) => s.value.into_owned(),
                        // Unreachable with loose property names off.
                        ObjectPropName::Word(w) => w.value.to_string(),
                    };
                    let len = pointer.len();
                    push_key(pointer, &key);
                    if map.contains_key(&key) {
                        let mut d =
                            Diagnostic::new(pointer.as_str(), format!("duplicate key '{key}'"));
                        d.span = Some(self.lines.span(prop.range));
                        d.set_note("first written here", pointer.as_str());
                        self.errors.push(d);
                    } else {
                        self.record(pointer, prop.range);
                        let value = self.convert(prop.value, pointer);
                        map.insert(key, value);
                    }
                    pointer.truncate(len);
                }
                Value::Object(map)
            }
        }
    }
}

/// An integer literal stays exact (u64, or i64 when negative); anything
/// else, or an integer too large for both, becomes f64.
fn number(text: &str) -> Option<Number> {
    let integer = if text.contains(['.', 'e', 'E']) {
        None
    } else {
        text.parse::<u64>()
            .map(Number::from)
            .or_else(|_| text.parse::<i64>().map(Number::from))
            .ok()
    };
    integer.or_else(|| text.parse::<f64>().ok().and_then(Number::from_f64))
}

/// Line and column (1-based, the column in characters) of byte offsets.
struct Lines<'t> {
    text: &'t str,
    /// Byte offset where each line starts.
    starts: Vec<usize>,
}

impl<'t> Lines<'t> {
    fn new(text: &'t str) -> Lines<'t> {
        let starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        Lines { text, starts }
    }

    fn span(&self, range: Range) -> Span {
        // starts[0] == 0, so at least one start is <= range.start.
        let line = self.starts.partition_point(|&s| s <= range.start);
        let line_start = self.starts[line - 1];
        Span {
            start: range.start,
            end: range.end,
            line,
            column: self.text[line_start..range.start].chars().count() + 1,
        }
    }
}

/// Appends an object key to a JSON Pointer (RFC 6901: `~` and `/` escaped).
pub(crate) fn push_key(pointer: &mut String, key: &str) {
    pointer.push('/');
    for c in key.chars() {
        match c {
            '~' => pointer.push_str("~0"),
            '/' => pointer.push_str("~1"),
            c => pointer.push(c),
        }
    }
}

pub(crate) fn push_index(pointer: &mut String, index: usize) {
    // Writing to a String cannot fail.
    let _ = write!(pointer, "/{index}");
}

/// The JSON Pointer of where serde failed: `/rooms/2/size`, or `""` for
/// the value itself.
pub(crate) fn pointer(path: &serde_path_to_error::Path) -> String {
    use serde_path_to_error::Segment;
    let mut p = String::new();
    for segment in path.iter() {
        match segment {
            Segment::Seq { index } => push_index(&mut p, *index),
            Segment::Map { key } => push_key(&mut p, key),
            // Not a level of the JSON: our formats have no externally
            // tagged enums.
            Segment::Enum { .. } | Segment::Unknown => {}
        }
    }
    p
}

#[cfg(test)]
mod tests;
