//! Diagnostics on stderr, with the line of the file they are about:
//!
//! ```text
//! error: step 'walls': unknown operator 'Scater'
//!   --> island.json:12:20
//!    |
//! 12 |     {"id": "walls", "operator": "Scater",
//!    |                     ^^^^^^^^^^^^^^^^^^^^
//! ```

use std::fmt::Write;
use std::path::Path;

use omnigenesys_core::spec::{Diagnostic, Span};

use crate::Failed;

/// Prints every error and a count; returns the failure to propagate.
pub fn errors(path: &Path, text: &str, errors: &[Diagnostic]) -> Failed {
    for e in errors {
        print("error", path, text, e);
    }
    let plural = if errors.len() == 1 { "" } else { "s" };
    eprintln!("{}: {} error{plural}", path.display(), errors.len());
    Failed
}

pub fn print(kind: &str, path: &Path, text: &str, d: &Diagnostic) {
    eprint!("{}", render(kind, path, text, d));
}

fn render(kind: &str, path: &Path, text: &str, d: &Diagnostic) -> String {
    let mut out = format!("{kind}: ");
    if let Some(id) = &d.step_id {
        let _ = write!(out, "step '{id}': ");
    }
    out.push_str(&d.message);
    out.push('\n');
    match d.span {
        Some(span) => {
            let _ = writeln!(
                out,
                "  --> {}:{}:{}",
                path.display(),
                span.line,
                span.column
            );
            out.push_str(&snippet(text, span));
        }
        None if d.pointer.is_empty() => {
            let _ = writeln!(out, "  --> {}", path.display());
        }
        None => {
            let _ = writeln!(out, "  --> {} at {}", path.display(), d.pointer);
        }
    }
    if let Some(note) = &d.note {
        let _ = write!(out, "   = note: {}", note.message);
        if let Some(s) = note.span {
            let _ = write!(out, " ({}:{}:{})", path.display(), s.line, s.column);
        }
        out.push('\n');
    }
    out
}

/// The span's line with a caret under the span (up to the end of the line).
fn snippet(text: &str, span: Span) -> String {
    let Some(line) = text.lines().nth(span.line - 1) else {
        return String::new();
    };
    let number = span.line.to_string();
    let gutter = " ".repeat(number.len());
    let line_start = text[..span.start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = line_start + line.len();
    let end = span.end.clamp(span.start, line_end);
    let width = text[span.start..end].chars().count().max(1);
    let pad = " ".repeat(span.column - 1);
    format!(
        "{gutter} |\n{number} | {line}\n{gutter} | {pad}{}\n",
        "^".repeat(width)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic(start: usize, end: usize, line: usize, column: usize) -> Diagnostic {
        Diagnostic {
            message: String::new(),
            step_id: None,
            pointer: String::new(),
            span: Some(Span {
                start,
                end,
                line,
                column,
            }),
            note: None,
        }
    }

    #[test]
    fn underlines_the_span_on_its_line() {
        let text = "{\n  \"a\": 1,\n  \"op\": \"Scater\"\n}";
        let start = text.find("\"op\"").unwrap();
        let mut d = diagnostic(start, start + 14, 3, 3);
        d.message = "unknown operator 'Scater'".to_string();
        d.step_id = Some("walls".to_string());
        let shown = render("error", Path::new("p.json"), text, &d);
        assert_eq!(
            shown,
            "error: step 'walls': unknown operator 'Scater'\n  \
             --> p.json:3:3\n  \
             |\n\
             3 |   \"op\": \"Scater\"\n  \
             |   ^^^^^^^^^^^^^^\n"
        );
    }

    #[test]
    fn a_span_over_several_lines_is_cut_at_the_first() {
        let text = "{\n  \"size\": {\n    \"w\": 0\n  }\n}";
        let start = text.find("\"size\"").unwrap();
        let d = diagnostic(start, text.len() - 2, 2, 3);
        let shown = render("error", Path::new("p.json"), text, &d);
        assert!(
            // `"size": {` is 9 characters; the rest of the span is cut.
            shown.ends_with("2 |   \"size\": {\n  |   ^^^^^^^^^\n"),
            "{shown}"
        );
    }
}
