//! Expressions over params: `min(20, 5 + floor(stage * 10 / 3))`.
//!
//! A small language of its own (research of 2026-10-25,
//! `docs/pesquisa/relatorios/2026-10-25-expressoes-nos-params.md`): every
//! value comes out the same on every platform, native and wasm, so a map
//! made from the same file, seed and params is the same everywhere.
//!
//! - Four types: integer (i64), number (f64), boolean, string. An integer
//!   meeting a number becomes a number; `/` always gives a number; whole
//!   division is `div` and `mod` (floored, `mod` takes the sign of the
//!   divisor); `floor`, `ceil`, `round` and `trunc` give integers.
//! - Only float operations IEEE 754 makes exact everywhere: `+ − × ÷`,
//!   `sqrt`, rounding, comparisons. No `sin`, `exp`, `log` or float powers,
//!   which the standard library computes differently per platform; `pow`
//!   takes a whole exponent and is a fixed chain of multiplications.
//! - No NaN, no infinity, no silent overflow: dividing by zero, an integer
//!   too large, a number too large are errors that say where.
//! - Bounded: at most [`MAX_LEN`] bytes, [`MAX_DEPTH`] levels of nesting,
//!   [`MAX_TOKENS`] tokens and [`MAX_ARGS`] arguments to a call. No loops,
//!   so evaluating takes as long as the expression is long.
//!
//! Mistakes from other languages get a hint instead of a new meaning: `^`
//! and `**` (write `pow`), `=` (`==`), `<>`, `&&`, `||`, `!`, `;`, `?:`,
//! `%`, and comparisons in a chain (`a < b < c`).

use std::fmt;
use std::ops::Range;

/// Longest expression, in bytes.
pub const MAX_LEN: usize = 1024;
/// Deepest nesting of parentheses, calls and operators.
pub const MAX_DEPTH: usize = 32;
pub const MAX_TOKENS: usize = 256;
/// Most arguments to one call (`min` and `max` take any number).
pub const MAX_ARGS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Integer,
    Number,
    Boolean,
    String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Integer(i64),
    Number(f64),
    Boolean(bool),
    String(String),
}

impl Value {
    pub fn kind(&self) -> Type {
        match self {
            Value::Integer(_) => Type::Integer,
            Value::Number(_) => Type::Number,
            Value::Boolean(_) => Type::Boolean,
            Value::String(_) => Type::String,
        }
    }
}

/// A problem in an expression: what, and where in its text (byte offsets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprError {
    pub span: Range<usize>,
    pub message: String,
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ExprError {}

/// Every word the language uses itself; a param cannot be named one.
pub const RESERVED: &[&str] = &[
    "and", "or", "not", "true", "false", "if", "min", "max", "clamp", "abs", "floor", "ceil",
    "round", "trunc", "div", "mod", "pow", "sqrt", "lerp",
];

/// A parsed expression. [`check`](Self::check) it against the params it
/// may name, then [`eval`](Self::eval) it.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    root: Node,
}

#[derive(Debug, Clone, PartialEq)]
struct Node {
    span: Range<usize>,
    kind: Kind,
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Integer(i64),
    Number(f64),
    Boolean(bool),
    String(String),
    Name(String),
    Neg(Box<Node>),
    Not(Box<Node>),
    Binary(Op, Box<Node>, Box<Node>),
    Call(Func, Vec<Node>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl Op {
    fn is_comparison(self) -> bool {
        matches!(self, Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge)
    }

    /// (left, right) binding power, lowest first (matklad, "Simple but
    /// Powerful Pratt Parsing", 2020). Left below right: left-associative.
    fn power(self) -> (u8, u8) {
        match self {
            Op::Or => (1, 2),
            Op::And => (3, 4),
            // `not` binds at 5, so `not a == b` is `not (a == b)`.
            Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => (7, 8),
            Op::Add | Op::Sub => (9, 10),
            Op::Mul | Op::Div => (11, 12),
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "*",
            Op::Div => "/",
            Op::Eq => "==",
            Op::Ne => "!=",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
            Op::And => "and",
            Op::Or => "or",
        }
    }
}

const NOT_POWER: u8 = 5;
const NEG_POWER: u8 = 13;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Func {
    If,
    Min,
    Max,
    Clamp,
    Abs,
    Floor,
    Ceil,
    Round,
    Trunc,
    Div,
    Mod,
    Pow,
    Sqrt,
    Lerp,
}

const FUNCS: &[(&str, Func)] = &[
    ("if", Func::If),
    ("min", Func::Min),
    ("max", Func::Max),
    ("clamp", Func::Clamp),
    ("abs", Func::Abs),
    ("floor", Func::Floor),
    ("ceil", Func::Ceil),
    ("round", Func::Round),
    ("trunc", Func::Trunc),
    ("div", Func::Div),
    ("mod", Func::Mod),
    ("pow", Func::Pow),
    ("sqrt", Func::Sqrt),
    ("lerp", Func::Lerp),
];

impl Func {
    fn name(self) -> &'static str {
        FUNCS
            .iter()
            .find(|(_, f)| *f == self)
            .map_or("?", |(n, _)| n)
    }

    /// How it is called, for arity errors.
    fn usage(self) -> &'static str {
        match self {
            Func::If => "if(condition, then, else)",
            Func::Min => "min(a, b, ...)",
            Func::Max => "max(a, b, ...)",
            Func::Clamp => "clamp(x, low, high)",
            Func::Abs => "abs(x)",
            Func::Floor => "floor(x)",
            Func::Ceil => "ceil(x)",
            Func::Round => "round(x)",
            Func::Trunc => "trunc(x)",
            Func::Div => "div(a, b)",
            Func::Mod => "mod(a, b)",
            Func::Pow => "pow(base, exponent)",
            Func::Sqrt => "sqrt(x)",
            Func::Lerp => "lerp(a, b, t)",
        }
    }

    /// Arguments it takes: exactly, or at least (`min`, `max`).
    fn arity(self) -> (usize, bool) {
        match self {
            Func::Min | Func::Max => (2, true),
            Func::If | Func::Clamp | Func::Lerp => (3, false),
            Func::Div | Func::Mod | Func::Pow => (2, false),
            Func::Abs | Func::Floor | Func::Ceil | Func::Round | Func::Trunc | Func::Sqrt => {
                (1, false)
            }
        }
    }
}

// --- Reading the text into tokens -------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Integer(i64),
    Number(f64),
    String(String),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
    Comma,
    EqEq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    End,
}

fn error(span: Range<usize>, message: impl Into<String>) -> ExprError {
    ExprError {
        span,
        message: message.into(),
    }
}

fn lex(text: &str) -> Result<Vec<(Tok, Range<usize>)>, ExprError> {
    if text.len() > MAX_LEN {
        return Err(error(
            0..text.len(),
            format!(
                "the expression has {} bytes, more than {MAX_LEN}",
                text.len()
            ),
        ));
    }
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let c = bytes[i];
        let next = bytes.get(i + 1).copied();
        let tok = match c {
            b' ' | b'\t' | b'\n' | b'\r' => {
                i += 1;
                continue;
            }
            b'0'..=b'9' => {
                let (tok, end) = number(text, i)?;
                i = end;
                tok
            }
            b'"' => {
                let (s, end) = string(text, i)?;
                i = end;
                Tok::String(s)
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                Tok::Ident(text[start..i].to_string())
            }
            b'+' => single(&mut i, Tok::Plus),
            b'-' => single(&mut i, Tok::Minus),
            b'*' if next == Some(b'*') => {
                return Err(error(i..i + 2, "there is no **: write pow(base, exponent)"));
            }
            b'*' => single(&mut i, Tok::Star),
            b'/' => single(&mut i, Tok::Slash),
            b'(' => single(&mut i, Tok::LParen),
            b')' => single(&mut i, Tok::RParen),
            b',' => single(&mut i, Tok::Comma),
            b'=' if next == Some(b'=') => double(&mut i, Tok::EqEq),
            b'=' => return Err(error(i..i + 1, "to compare, write == (one = is not used)")),
            b'!' if next == Some(b'=') => double(&mut i, Tok::Ne),
            b'!' => return Err(error(i..i + 1, "write not instead of !")),
            b'<' if next == Some(b'=') => double(&mut i, Tok::Le),
            b'<' if next == Some(b'>') => {
                return Err(error(i..i + 2, "write != for 'not equal'"));
            }
            b'<' => single(&mut i, Tok::Lt),
            b'>' if next == Some(b'=') => double(&mut i, Tok::Ge),
            b'>' => single(&mut i, Tok::Gt),
            b'&' if next == Some(b'&') => return Err(error(i..i + 2, "write and instead of &&")),
            b'|' if next == Some(b'|') => return Err(error(i..i + 2, "write or instead of ||")),
            b'^' => return Err(error(i..i + 1, "there is no ^: write pow(base, exponent)")),
            b'%' => return Err(error(i..i + 1, "there is no %: write mod(a, b)")),
            b';' => return Err(error(i..i + 1, "separate arguments with , (not ;)")),
            b'?' | b':' => {
                return Err(error(
                    i..i + 1,
                    "there is no ? :: write if(condition, then, else)",
                ));
            }
            b'.' => {
                return Err(error(
                    i..i + 1,
                    "a number starts with a digit: write 0.5, not .5",
                ));
            }
            _ => {
                let ch = text[i..].chars().next().unwrap_or('?');
                return Err(error(
                    i..i + ch.len_utf8(),
                    format!("unexpected character '{ch}'"),
                ));
            }
        };
        out.push((tok, start..i));
        if out.len() > MAX_TOKENS {
            return Err(error(
                start..i,
                format!("the expression has more than {MAX_TOKENS} pieces"),
            ));
        }
    }
    out.push((Tok::End, bytes.len()..bytes.len()));
    Ok(out)
}

fn single(i: &mut usize, tok: Tok) -> Tok {
    *i += 1;
    tok
}

fn double(i: &mut usize, tok: Tok) -> Tok {
    *i += 2;
    tok
}

/// `12` is an integer; `3.5` and `2e3` are numbers. The digits must not
/// run into a letter (`3x`).
fn number(text: &str, start: usize) -> Result<(Tok, usize), ExprError> {
    let b = text.as_bytes();
    let digits = |mut i: usize| {
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        i
    };
    let mut i = digits(start);
    let mut float = false;
    if i < b.len() && b[i] == b'.' {
        if !b.get(i + 1).is_some_and(u8::is_ascii_digit) {
            return Err(error(start..i + 1, "a digit must follow the decimal point"));
        }
        i = digits(i + 1);
        float = true;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        if !b.get(j).is_some_and(u8::is_ascii_digit) {
            return Err(error(start..j, "a digit must follow the exponent"));
        }
        i = digits(j);
        float = true;
    }
    if i < b.len() && (b[i].is_ascii_alphabetic() || b[i] == b'_') {
        return Err(error(
            start..i + 1,
            "a number runs into a letter: put an operator between them (2 * x)",
        ));
    }
    let lit = &text[start..i];
    let tok = if float {
        // Rust's float parsing is correctly rounded: the same everywhere.
        let n: f64 = lit
            .parse()
            .map_err(|_| error(start..i, format!("{lit} is not a number")))?;
        if !n.is_finite() {
            return Err(error(start..i, format!("{lit} is too large")));
        }
        Tok::Number(n)
    } else {
        Tok::Integer(
            lit.parse()
                .map_err(|_| error(start..i, format!("{lit} is too large for an integer")))?,
        )
    };
    Ok((tok, i))
}

/// `"forest"`; `\"` and `\\` are the only escapes.
fn string(text: &str, start: usize) -> Result<(String, usize), ExprError> {
    let mut out = String::new();
    let mut chars = text[start + 1..].char_indices();
    while let Some((k, ch)) = chars.next() {
        let at = start + 1 + k;
        match ch {
            '"' => return Ok((out, at + 1)),
            '\\' => match chars.next() {
                Some((_, '"')) => out.push('"'),
                Some((_, '\\')) => out.push('\\'),
                _ => {
                    return Err(error(
                        at..at + 1,
                        "the only escapes in a string are \\\" and \\\\",
                    ));
                }
            },
            _ => out.push(ch),
        }
    }
    Err(error(
        start..text.len(),
        "the string is never closed with \"",
    ))
}

// --- Parsing ----------------------------------------------------------------

struct Parser {
    toks: Vec<(Tok, Range<usize>)>,
    at: usize,
    depth: usize,
}

impl Expr {
    /// Reads `text`. The first mistake comes back, with where it is.
    pub fn parse(text: &str) -> Result<Expr, ExprError> {
        let mut p = Parser {
            toks: lex(text)?,
            at: 0,
            depth: 0,
        };
        if p.peek() == &Tok::End {
            return Err(error(0..text.len(), "the expression is empty"));
        }
        let root = p.expr(0)?;
        match p.peek() {
            Tok::End => Ok(Expr { root }),
            Tok::Comma => Err(error(
                p.span(),
                "unexpected ',': numbers use a decimal point (2.5), and , only separates arguments",
            )),
            Tok::RParen => Err(error(p.span(), "this ) closes nothing")),
            _ => Err(error(
                p.span(),
                "an operator is missing before this (2 * x, not 2 x)",
            )),
        }
    }

    /// The params it names, each with where.
    pub fn names(&self) -> Vec<(&str, Range<usize>)> {
        let mut out = Vec::new();
        self.root.names(&mut out);
        out
    }

    /// Whether every name is one of `params` and every operation fits its
    /// types; gives the type of the result. Every mistake comes back.
    pub fn check(&self, params: &[(&str, Type)]) -> Result<Type, Vec<ExprError>> {
        let mut errors = Vec::new();
        match self.root.check(params, &mut errors) {
            Some(t) if errors.is_empty() => Ok(t),
            _ => Err(errors),
        }
    }

    /// The value, with `value` giving each named param's. Only for an
    /// expression that [`check`](Self::check)ed: a name `value` does not
    /// know, or a value of another type than checked, is an error too.
    pub fn eval(&self, value: &dyn Fn(&str) -> Option<Value>) -> Result<Value, ExprError> {
        self.root.eval(value)
    }
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.at].0
    }

    fn span(&self) -> Range<usize> {
        self.toks[self.at].1.clone()
    }

    fn bump(&mut self) -> (Tok, Range<usize>) {
        let t = self.toks[self.at].clone();
        if t.0 != Tok::End {
            self.at += 1;
        }
        t
    }

    fn infix(&self) -> Option<Op> {
        Some(match self.peek() {
            Tok::Plus => Op::Add,
            Tok::Minus => Op::Sub,
            Tok::Star => Op::Mul,
            Tok::Slash => Op::Div,
            Tok::EqEq => Op::Eq,
            Tok::Ne => Op::Ne,
            Tok::Lt => Op::Lt,
            Tok::Le => Op::Le,
            Tok::Gt => Op::Gt,
            Tok::Ge => Op::Ge,
            Tok::Ident(w) if w == "and" => Op::And,
            Tok::Ident(w) if w == "or" => Op::Or,
            _ => return None,
        })
    }

    fn expr(&mut self, min_power: u8) -> Result<Node, ExprError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(error(
                self.span(),
                format!("nested more than {MAX_DEPTH} levels deep"),
            ));
        }
        let mut lhs = self.prefix()?;
        // The comparison just built at this level, to refuse a chain.
        let mut compared = false;
        while let Some(op) = self.infix() {
            let (left, right) = op.power();
            if left < min_power {
                break;
            }
            let op_span = self.span();
            if op.is_comparison() && compared {
                return Err(error(
                    op_span,
                    "comparisons do not chain: write a < b and b < c",
                ));
            }
            self.bump();
            let rhs = self.expr(right)?;
            let span = lhs.span.start..rhs.span.end;
            lhs = Node {
                span,
                kind: Kind::Binary(op, Box::new(lhs), Box::new(rhs)),
            };
            compared = op.is_comparison();
        }
        self.depth -= 1;
        Ok(lhs)
    }

    fn prefix(&mut self) -> Result<Node, ExprError> {
        let (tok, span) = self.bump();
        let node = |kind| Node {
            span: span.clone(),
            kind,
        };
        Ok(match tok {
            Tok::Integer(n) => node(Kind::Integer(n)),
            Tok::Number(n) => node(Kind::Number(n)),
            Tok::String(s) => node(Kind::String(s)),
            Tok::Minus => {
                let inner = self.expr(NEG_POWER)?;
                Node {
                    span: span.start..inner.span.end,
                    kind: Kind::Neg(Box::new(inner)),
                }
            }
            Tok::LParen => {
                let inner = self.expr(0)?;
                let (close, end) = self.bump();
                if close != Tok::RParen {
                    return Err(error(end, "a ( is never closed with )"));
                }
                // The parentheses belong to the span: errors underline them.
                Node {
                    span: span.start..end.end,
                    kind: inner.kind,
                }
            }
            Tok::Ident(w) => match w.as_str() {
                "true" => node(Kind::Boolean(true)),
                "false" => node(Kind::Boolean(false)),
                "not" => {
                    let inner = self.expr(NOT_POWER)?;
                    Node {
                        span: span.start..inner.span.end,
                        kind: Kind::Not(Box::new(inner)),
                    }
                }
                "and" | "or" => {
                    return Err(error(span, format!("'{w}' needs something before it")));
                }
                _ if self.peek() == &Tok::LParen => self.call(&w, span)?,
                _ if func(&w).is_some() => {
                    let f = func(&w).expect("just matched");
                    return Err(error(
                        span,
                        format!("'{w}' is a function: write {}", f.usage()),
                    ));
                }
                _ => node(Kind::Name(w)),
            },
            Tok::End => return Err(error(span, "the expression ends too soon")),
            Tok::RParen => return Err(error(span, "a value is missing before this )")),
            Tok::Comma => return Err(error(span, "a value is missing before this ,")),
            other => {
                return Err(error(
                    span,
                    format!("a value is missing before {}", describe(&other)),
                ));
            }
        })
    }

    fn call(&mut self, name: &str, name_span: Range<usize>) -> Result<Node, ExprError> {
        let Some(f) = func(name) else {
            let hint = suggest(name, FUNCS.iter().map(|(n, _)| *n))
                .map(|s| format!(" (did you mean {s}?)"))
                .unwrap_or_default();
            let all: Vec<&str> = FUNCS.iter().map(|(n, _)| *n).collect();
            return Err(error(
                name_span,
                format!(
                    "unknown function '{name}'{hint}; the functions are {}",
                    all.join(", ")
                ),
            ));
        };
        self.bump(); // (
        let mut args = Vec::new();
        if self.peek() != &Tok::RParen {
            loop {
                args.push(self.expr(0)?);
                if args.len() > MAX_ARGS {
                    return Err(error(
                        self.span(),
                        format!("more than {MAX_ARGS} arguments"),
                    ));
                }
                match self.peek() {
                    Tok::Comma => {
                        self.bump();
                    }
                    Tok::RParen => break,
                    Tok::End => {
                        return Err(error(
                            self.span(),
                            format!("{name}( is never closed with )"),
                        ));
                    }
                    _ => {
                        return Err(error(
                            self.span(),
                            "separate arguments with , (and decimals use a point: 2.5)",
                        ));
                    }
                }
            }
        }
        let (_, close) = self.bump();
        let span = name_span.start..close.end;
        let (n, at_least) = f.arity();
        if args.len() < n || (!at_least && args.len() > n) {
            let count = if at_least {
                format!("at least {n}")
            } else {
                n.to_string()
            };
            return Err(error(
                span,
                format!(
                    "{name} takes {count} argument{}, got {}: {}",
                    if n == 1 && !at_least { "" } else { "s" },
                    args.len(),
                    f.usage()
                ),
            ));
        }
        Ok(Node {
            span,
            kind: Kind::Call(f, args),
        })
    }
}

fn func(name: &str) -> Option<Func> {
    FUNCS.iter().find(|(n, _)| *n == name).map(|(_, f)| *f)
}

fn describe(tok: &Tok) -> String {
    match tok {
        Tok::Plus => "+".into(),
        Tok::Star => "*".into(),
        Tok::Slash => "/".into(),
        Tok::EqEq => "==".into(),
        Tok::Ne => "!=".into(),
        Tok::Lt => "<".into(),
        Tok::Le => "<=".into(),
        Tok::Gt => ">".into(),
        Tok::Ge => ">=".into(),
        _ => "this".into(),
    }
}

/// The closest of `options` to `word`, if close enough to be a typo.
pub fn suggest<'a>(word: &str, options: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let limit = if word.len() <= 3 { 1 } else { 2 };
    options
        .map(|o| (edit_distance(word, o), o))
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, _)| *d)
        .map(|(_, o)| o)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != *cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

// --- Types ------------------------------------------------------------------

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Type::Integer => "an integer",
            Type::Number => "a number",
            Type::Boolean => "a boolean",
            Type::String => "a string",
        })
    }
}

/// Pushes a type error at `span`; `None` for the node that has it.
fn bad(errors: &mut Vec<ExprError>, span: &Range<usize>, message: String) -> Option<Type> {
    errors.push(error(span.clone(), message));
    None
}

fn numeric(t: Type) -> bool {
    matches!(t, Type::Integer | Type::Number)
}

/// Integer with integer stays integer; anything with a number is a number.
fn widen(a: Type, b: Type) -> Type {
    if a == Type::Integer && b == Type::Integer {
        Type::Integer
    } else {
        Type::Number
    }
}

impl Node {
    fn names<'a>(&'a self, out: &mut Vec<(&'a str, Range<usize>)>) {
        match &self.kind {
            Kind::Name(n) => out.push((n, self.span.clone())),
            Kind::Neg(x) | Kind::Not(x) => x.names(out),
            Kind::Binary(_, a, b) => {
                a.names(out);
                b.names(out);
            }
            Kind::Call(_, args) => args.iter().for_each(|a| a.names(out)),
            _ => {}
        }
    }

    /// The type, or `None` after pushing why not. A part that already
    /// failed is not reported again by what contains it.
    fn check(&self, params: &[(&str, Type)], errors: &mut Vec<ExprError>) -> Option<Type> {
        match &self.kind {
            Kind::Integer(_) => Some(Type::Integer),
            Kind::Number(_) => Some(Type::Number),
            Kind::Boolean(_) => Some(Type::Boolean),
            Kind::String(_) => Some(Type::String),
            Kind::Name(n) => match params.iter().find(|(p, _)| p == n) {
                Some((_, t)) => Some(*t),
                None => {
                    let hint = suggest(n, params.iter().map(|(p, _)| *p))
                        .map(|s| format!(" (did you mean {s}?)"))
                        .unwrap_or_default();
                    bad(errors, &self.span, format!("no param named '{n}'{hint}"))
                }
            },
            Kind::Neg(x) => {
                let t = x.check(params, errors)?;
                if numeric(t) {
                    Some(t)
                } else {
                    errors.push(error(
                        self.span.clone(),
                        format!("- needs a number, not {t}"),
                    ));
                    None
                }
            }
            Kind::Not(x) => {
                let t = x.check(params, errors)?;
                if t == Type::Boolean {
                    Some(t)
                } else {
                    errors.push(error(
                        self.span.clone(),
                        format!("not needs a boolean, not {t}"),
                    ));
                    None
                }
            }
            Kind::Binary(op, a, b) => {
                let (ta, tb) = (a.check(params, errors), b.check(params, errors));
                let (ta, tb) = (ta?, tb?);
                let sym = op.symbol();
                match op {
                    Op::Add | Op::Sub | Op::Mul if numeric(ta) && numeric(tb) => {
                        Some(widen(ta, tb))
                    }
                    Op::Div if numeric(ta) && numeric(tb) => Some(Type::Number),
                    Op::Lt | Op::Le | Op::Gt | Op::Ge if numeric(ta) && numeric(tb) => {
                        Some(Type::Boolean)
                    }
                    Op::Eq | Op::Ne if (numeric(ta) && numeric(tb)) || ta == tb => {
                        Some(Type::Boolean)
                    }
                    Op::And | Op::Or if ta == Type::Boolean && tb == Type::Boolean => {
                        Some(Type::Boolean)
                    }
                    Op::And | Op::Or => bad(
                        errors,
                        &self.span,
                        format!("{sym} joins booleans, not {ta} and {tb}"),
                    ),
                    Op::Eq | Op::Ne => bad(
                        errors,
                        &self.span,
                        format!("{ta} and {tb} cannot be compared"),
                    ),
                    _ => bad(
                        errors,
                        &self.span,
                        format!("{sym} needs numbers, not {ta} and {tb}"),
                    ),
                }
            }
            Kind::Call(f, args) => {
                let types: Vec<Option<Type>> =
                    args.iter().map(|a| a.check(params, errors)).collect();
                let types: Vec<Type> = types.into_iter().collect::<Option<_>>()?;
                let name = f.name();
                let all_numeric = |ts: &[Type]| ts.iter().all(|t| numeric(*t));
                let widest = |ts: &[Type]| ts.iter().copied().reduce(widen);
                match f {
                    Func::If => {
                        if types[0] != Type::Boolean {
                            return bad(
                                errors,
                                &self.span,
                                format!(
                                    "if needs a boolean first (a comparison), not {}",
                                    types[0]
                                ),
                            );
                        }
                        match (types[1], types[2]) {
                            (a, b) if a == b => Some(a),
                            (a, b) if numeric(a) && numeric(b) => Some(Type::Number),
                            (a, b) => bad(
                                errors,
                                &self.span,
                                format!(
                                    "the two choices of if must be of one type, not {a} and {b}"
                                ),
                            ),
                        }
                    }
                    Func::Min | Func::Max | Func::Clamp => {
                        if all_numeric(&types) {
                            widest(&types)
                        } else {
                            bad(errors, &self.span, format!("{name} needs numbers"))
                        }
                    }
                    Func::Abs => {
                        if numeric(types[0]) {
                            Some(types[0])
                        } else {
                            bad(
                                errors,
                                &self.span,
                                format!("abs needs a number, not {}", types[0]),
                            )
                        }
                    }
                    Func::Floor | Func::Ceil | Func::Round | Func::Trunc => {
                        if numeric(types[0]) {
                            Some(Type::Integer)
                        } else {
                            bad(
                                errors,
                                &self.span,
                                format!("{name} needs a number, not {}", types[0]),
                            )
                        }
                    }
                    Func::Div | Func::Mod => {
                        if types.iter().all(|t| *t == Type::Integer) {
                            Some(Type::Integer)
                        } else {
                            bad(
                                errors,
                                &self.span,
                                format!("{name} takes integers; for numbers write floor(a / b)"),
                            )
                        }
                    }
                    Func::Pow => match (types[0], types[1]) {
                        (b, Type::Integer) if numeric(b) => Some(b),
                        (b, _) if numeric(b) => bad(
                            errors,
                            &self.span,
                            "pow takes a whole exponent (an integer): fractional powers are \
                             not the same on every machine"
                                .into(),
                        ),
                        (b, _) => bad(errors, &self.span, format!("pow needs a number, not {b}")),
                    },
                    Func::Sqrt | Func::Lerp => {
                        if all_numeric(&types) {
                            Some(Type::Number)
                        } else {
                            bad(errors, &self.span, format!("{name} needs numbers"))
                        }
                    }
                }
            }
        }
    }

    // --- Values -------------------------------------------------------------

    fn fail<T>(&self, message: impl Into<String>) -> Result<T, ExprError> {
        Err(error(self.span.clone(), message))
    }

    fn eval(&self, value: &dyn Fn(&str) -> Option<Value>) -> Result<Value, ExprError> {
        Ok(match &self.kind {
            Kind::Integer(n) => Value::Integer(*n),
            Kind::Number(n) => Value::Number(*n),
            Kind::Boolean(b) => Value::Boolean(*b),
            Kind::String(s) => Value::String(s.clone()),
            Kind::Name(n) => match value(n) {
                Some(v) => v,
                None => return self.fail(format!("no value for '{n}'")),
            },
            Kind::Neg(x) => match x.eval(value)? {
                Value::Integer(n) => Value::Integer(
                    n.checked_neg()
                        .ok_or_else(|| error(self.span.clone(), "too large for an integer"))?,
                ),
                Value::Number(n) => Value::Number(self.finite(-n)?),
                _ => return self.fail("- needs a number"),
            },
            Kind::Not(x) => Value::Boolean(!x.boolean(value)?),
            Kind::Binary(Op::And, a, b) => Value::Boolean(a.boolean(value)? && b.boolean(value)?),
            Kind::Binary(Op::Or, a, b) => Value::Boolean(a.boolean(value)? || b.boolean(value)?),
            Kind::Binary(op, a, b) => self.binary(*op, a.eval(value)?, b.eval(value)?)?,
            Kind::Call(f, args) => self.call(*f, args, value)?,
        })
    }

    fn boolean(&self, value: &dyn Fn(&str) -> Option<Value>) -> Result<bool, ExprError> {
        match self.eval(value)? {
            Value::Boolean(b) => Ok(b),
            v => self.fail(format!("needs a boolean, not {}", v.kind())),
        }
    }

    /// A number that may be kept: finite, and `-0` is `0` (so `min` and
    /// equality never depend on the sign of zero).
    fn finite(&self, n: f64) -> Result<f64, ExprError> {
        if n.is_finite() {
            Ok(if n == 0.0 { 0.0 } else { n })
        } else {
            self.fail("the result is too large")
        }
    }

    /// An integer as a number; beyond 2^53 a number cannot hold it exactly.
    fn widened(&self, v: &Value) -> Result<f64, ExprError> {
        match v {
            Value::Number(n) => Ok(*n),
            Value::Integer(n) if n.unsigned_abs() <= 1 << 53 => Ok(*n as f64),
            Value::Integer(n) => self.fail(format!(
                "{n} is too large to mix with numbers (above 2^53 they lose digits)"
            )),
            v => self.fail(format!("needs a number, not {}", v.kind())),
        }
    }

    fn int(&self, v: &Value) -> Result<i64, ExprError> {
        match v {
            Value::Integer(n) => Ok(*n),
            v => self.fail(format!("needs an integer, not {}", v.kind())),
        }
    }

    fn overflow(&self) -> ExprError {
        error(self.span.clone(), "the result is too large for an integer")
    }

    fn binary(&self, op: Op, a: Value, b: Value) -> Result<Value, ExprError> {
        use Value::{Boolean, Integer, Number};
        Ok(match (op, &a, &b) {
            (Op::Add, Integer(x), Integer(y)) => {
                Integer(x.checked_add(*y).ok_or_else(|| self.overflow())?)
            }
            (Op::Sub, Integer(x), Integer(y)) => {
                Integer(x.checked_sub(*y).ok_or_else(|| self.overflow())?)
            }
            (Op::Mul, Integer(x), Integer(y)) => {
                Integer(x.checked_mul(*y).ok_or_else(|| self.overflow())?)
            }
            (Op::Add | Op::Sub | Op::Mul | Op::Div, _, _) => {
                let (x, y) = (self.widened(&a)?, self.widened(&b)?);
                Number(self.finite(match op {
                    Op::Add => x + y,
                    Op::Sub => x - y,
                    Op::Mul => x * y,
                    _ if y == 0.0 => return self.fail("division by zero"),
                    _ => x / y,
                })?)
            }
            (Op::Eq | Op::Ne, Value::String(x), Value::String(y)) => {
                Boolean((x == y) == (op == Op::Eq))
            }
            (Op::Eq | Op::Ne, Boolean(x), Boolean(y)) => Boolean((x == y) == (op == Op::Eq)),
            (_, Integer(x), Integer(y)) => Boolean(compare(op, x.cmp(y))),
            _ => {
                let (x, y) = (self.widened(&a)?, self.widened(&b)?);
                // Both finite: never unordered.
                Boolean(compare(
                    op,
                    x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                ))
            }
        })
    }

    fn call(
        &self,
        f: Func,
        args: &[Node],
        value: &dyn Fn(&str) -> Option<Value>,
    ) -> Result<Value, ExprError> {
        // `if` looks at one branch only: `if(n == 0, 0, 10 / n)` never divides by zero.
        if f == Func::If {
            let (pick, other) = if args[0].boolean(value)? {
                (&args[1], &args[2])
            } else {
                (&args[2], &args[1])
            };
            let v = pick.eval(value)?;
            // Branches of an integer and a number give a number either way:
            // the other's type, found without evaluating it.
            return Ok(match (&v, other.type_with(value)) {
                (Value::Integer(_), Some(Type::Number)) => Value::Number(self.widened(&v)?),
                _ => v,
            });
        }
        let vals = args
            .iter()
            .map(|a| a.eval(value))
            .collect::<Result<Vec<_>, _>>()?;
        let all_int = vals.iter().all(|v| matches!(v, Value::Integer(_)));
        Ok(match f {
            Func::If => unreachable!("handled above"),
            Func::Min | Func::Max => {
                let less = |a: &Value, b: &Value| -> Result<bool, ExprError> {
                    Ok(if all_int {
                        self.int(a)? < self.int(b)?
                    } else {
                        self.widened(a)? < self.widened(b)?
                    })
                };
                // Written out, not f64::min: ties keep the first, always.
                let mut best = &vals[0];
                for v in &vals[1..] {
                    let better = if f == Func::Min {
                        less(v, best)?
                    } else {
                        less(best, v)?
                    };
                    if better {
                        best = v;
                    }
                }
                self.unify(best, all_int)?
            }
            Func::Clamp => {
                let (x, lo, hi) = (&vals[0], &vals[1], &vals[2]);
                if all_int {
                    let (x, lo, hi) = (self.int(x)?, self.int(lo)?, self.int(hi)?);
                    if lo > hi {
                        return self.fail(format!("clamp: low {lo} is above high {hi}"));
                    }
                    Value::Integer(x.max(lo).min(hi))
                } else {
                    let (x, lo, hi) = (self.widened(x)?, self.widened(lo)?, self.widened(hi)?);
                    if lo > hi {
                        return self.fail(format!("clamp: low {lo} is above high {hi}"));
                    }
                    Value::Number(if x < lo {
                        lo
                    } else if x > hi {
                        hi
                    } else {
                        x
                    })
                }
            }
            Func::Abs => match &vals[0] {
                Value::Integer(n) => {
                    Value::Integer(n.checked_abs().ok_or_else(|| self.overflow())?)
                }
                v => Value::Number(self.widened(v)?.abs()),
            },
            Func::Floor | Func::Ceil | Func::Round | Func::Trunc => match &vals[0] {
                Value::Integer(n) => Value::Integer(*n),
                v => {
                    let x = self.widened(v)?;
                    // All four are exact in IEEE 754; `round` goes half away from zero.
                    let r = match f {
                        Func::Floor => x.floor(),
                        Func::Ceil => x.ceil(),
                        Func::Round => x.round(),
                        _ => x.trunc(),
                    };
                    // i64 holds [-2^63, 2^63): the bounds as f64 are exact.
                    if !(-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&r) {
                        return Err(self.overflow());
                    }
                    Value::Integer(r as i64)
                }
            },
            Func::Div | Func::Mod => {
                let (a, b) = (self.int(&vals[0])?, self.int(&vals[1])?);
                if b == 0 {
                    return self.fail("division by zero");
                }
                let q = a.checked_div(b).ok_or_else(|| self.overflow())?;
                let r = a - q * b;
                // Floored: round the quotient down when the signs differ.
                let (q, r) = if r != 0 && ((r < 0) != (b < 0)) {
                    (q - 1, r + b)
                } else {
                    (q, r)
                };
                Value::Integer(if f == Func::Div { q } else { r })
            }
            Func::Pow => {
                let e = self.int(&vals[1])?;
                if e < 0 {
                    return self.fail(format!(
                        "pow takes an exponent of 0 or more, not {e} (for 1 / x^n write 1 / pow(x, n))"
                    ));
                }
                match &vals[0] {
                    Value::Integer(b) => Value::Integer(self.int_pow(*b, e as u64)?),
                    v => Value::Number(self.finite(num_pow(self.widened(v)?, e as u64))?),
                }
            }
            Func::Sqrt => {
                let x = self.widened(&vals[0])?;
                if x < 0.0 {
                    return self.fail(format!("sqrt of {x}, below zero"));
                }
                Value::Number(self.finite(x.sqrt())?)
            }
            Func::Lerp => {
                let (a, b, t) = (
                    self.widened(&vals[0])?,
                    self.widened(&vals[1])?,
                    self.widened(&vals[2])?,
                );
                Value::Number(self.finite(a + (b - a) * t)?)
            }
        })
    }

    /// The type this node checks to, with the params' types read from
    /// their values: no evaluation, so no error from a branch not taken.
    fn type_with(&self, value: &dyn Fn(&str) -> Option<Value>) -> Option<Type> {
        let mut names = Vec::new();
        self.names(&mut names);
        let params: Vec<(&str, Type)> = names
            .iter()
            .filter_map(|(n, _)| value(n).map(|v| (*n, v.kind())))
            .collect();
        self.check(&params, &mut Vec::new())
    }

    fn unify(&self, v: &Value, all_int: bool) -> Result<Value, ExprError> {
        Ok(if all_int {
            v.clone()
        } else {
            Value::Number(self.widened(v)?)
        })
    }

    fn int_pow(&self, base: i64, mut e: u64) -> Result<i64, ExprError> {
        let (mut acc, mut b) = (1i64, base);
        while e > 0 {
            if e & 1 == 1 {
                acc = acc.checked_mul(b).ok_or_else(|| self.overflow())?;
            }
            e >>= 1;
            if e > 0 {
                b = b.checked_mul(b).ok_or_else(|| self.overflow())?;
            }
        }
        Ok(acc)
    }
}

/// Square and multiply: the same multiplications, in the same order, on
/// every machine (unlike `f64::powi`, whose precision is unspecified).
fn num_pow(base: f64, mut e: u64) -> f64 {
    let (mut acc, mut b) = (1.0f64, base);
    while e > 0 {
        if e & 1 == 1 {
            acc *= b;
        }
        e >>= 1;
        if e > 0 {
            b *= b;
        }
    }
    acc
}

fn compare(op: Op, o: std::cmp::Ordering) -> bool {
    use std::cmp::Ordering::{Equal, Greater, Less};
    match op {
        Op::Eq => o == Equal,
        Op::Ne => o != Equal,
        Op::Lt => o == Less,
        Op::Le => o != Greater,
        Op::Gt => o == Greater,
        Op::Ge => o != Less,
        _ => false,
    }
}

#[cfg(test)]
mod tests;
