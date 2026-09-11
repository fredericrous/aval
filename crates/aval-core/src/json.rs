//! A small JSON reader and writer.
//!
//! Hand-rolled for the same reason amont hand-rolls its own: this crate runs on
//! the pre-commit path and pulls in nothing. The writer is what `--json` emits;
//! the reader is what the conformance harness uses to load its fixtures, so the
//! battery needs no dev-dependency either.

use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    /// Ordered, so emitted objects are byte-stable across runs.
    Obj(BTreeMap<String, Json>),
}

impl Json {
    pub fn obj() -> Json {
        Json::Obj(BTreeMap::new())
    }

    pub fn set(mut self, k: &str, v: impl Into<Json>) -> Json {
        if let Json::Obj(m) = &mut self {
            m.insert(k.to_string(), v.into());
        }
        self
    }

    /// Insert only when present, so absent fields stay absent rather than null.
    pub fn set_opt<T: Into<Json>>(self, k: &str, v: Option<T>) -> Json {
        match v {
            Some(x) => self.set(k, x),
            None => self,
        }
    }

    pub fn get(&self, k: &str) -> Option<&Json> {
        match self {
            Json::Obj(m) => m.get(k),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Json::Num(n) => Some(*n as i64),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(a) => Some(a),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }

    pub fn write(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Num(n) => {
                if n.fract() == 0.0 && n.is_finite() {
                    let _ = write!(out, "{}", *n as i64);
                } else {
                    let _ = write!(out, "{}", n);
                }
            }
            Json::Str(s) => escape(s, out),
            Json::Arr(a) => {
                out.push('[');
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    v.write(out);
                }
                out.push(']');
            }
            Json::Obj(m) => {
                out.push('{');
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    escape(k, out);
                    out.push(':');
                    v.write(out);
                }
                out.push('}');
            }
        }
    }
}

/// Indented output, for a file a person reads and edits.
///
/// `write` stays compact: it serves `--json`, where the reader is a program and
/// byte-stability is the contract. This one serves `.claude/settings.json`,
/// where the reader is a person and a single long line is hostile.
impl Json {
    pub fn write_pretty(&self, out: &mut String, depth: usize) {
        let pad = |n: usize| "  ".repeat(n);
        match self {
            Json::Arr(items) if !items.is_empty() => {
                out.push_str("[\n");
                for (i, v) in items.iter().enumerate() {
                    out.push_str(&pad(depth + 1));
                    v.write_pretty(out, depth + 1);
                    if i + 1 < items.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push_str(&pad(depth));
                out.push(']');
            }
            Json::Obj(map) if !map.is_empty() => {
                out.push_str("{\n");
                for (i, (k, v)) in map.iter().enumerate() {
                    out.push_str(&pad(depth + 1));
                    Json::Str(k.clone()).write(out);
                    out.push_str(": ");
                    v.write_pretty(out, depth + 1);
                    if i + 1 < map.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push_str(&pad(depth));
                out.push('}');
            }
            other => other.write(out),
        }
    }
}

impl fmt::Display for Json {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = String::new();
        self.write(&mut s);
        f.write_str(&s)
    }
}

impl From<&str> for Json {
    fn from(v: &str) -> Json {
        Json::Str(v.to_string())
    }
}
impl From<String> for Json {
    fn from(v: String) -> Json {
        Json::Str(v)
    }
}
impl From<bool> for Json {
    fn from(v: bool) -> Json {
        Json::Bool(v)
    }
}
impl From<i32> for Json {
    fn from(v: i32) -> Json {
        Json::Num(v as f64)
    }
}
impl From<usize> for Json {
    fn from(v: usize) -> Json {
        Json::Num(v as f64)
    }
}
impl<T: Into<Json>> From<Vec<T>> for Json {
    fn from(v: Vec<T>) -> Json {
        Json::Arr(v.into_iter().map(Into::into).collect())
    }
}

fn escape(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

// ---------------------------------------------------------------- reader

pub fn parse(src: &str) -> Result<Json, String> {
    let b: Vec<char> = src.chars().collect();
    let mut i = 0;
    let v = value(&b, &mut i)?;
    ws(&b, &mut i);
    if i != b.len() {
        return Err(format!("trailing input at character {}", i));
    }
    Ok(v)
}

fn ws(b: &[char], i: &mut usize) {
    while *i < b.len() && b[*i].is_whitespace() {
        *i += 1;
    }
}

fn value(b: &[char], i: &mut usize) -> Result<Json, String> {
    ws(b, i);
    match b.get(*i) {
        None => Err("unexpected end of input".into()),
        Some('{') => object(b, i),
        Some('[') => array(b, i),
        Some('"') => string(b, i).map(Json::Str),
        Some('t') => lit(b, i, "true", Json::Bool(true)),
        Some('f') => lit(b, i, "false", Json::Bool(false)),
        Some('n') => lit(b, i, "null", Json::Null),
        Some(_) => number(b, i),
    }
}

fn lit(b: &[char], i: &mut usize, word: &str, v: Json) -> Result<Json, String> {
    if b[*i..].starts_with(&word.chars().collect::<Vec<_>>()[..]) {
        *i += word.len();
        Ok(v)
    } else {
        Err(format!("expected `{}` at character {}", word, i))
    }
}

fn number(b: &[char], i: &mut usize) -> Result<Json, String> {
    let start = *i;
    while *i < b.len() && (b[*i].is_ascii_digit() || "+-.eE".contains(b[*i])) {
        *i += 1;
    }
    let s: String = b[start..*i].iter().collect();
    s.parse::<f64>()
        .map(Json::Num)
        .map_err(|_| format!("bad number `{}`", s))
}

fn string(b: &[char], i: &mut usize) -> Result<String, String> {
    *i += 1; // opening quote
    let mut out = String::new();
    while *i < b.len() {
        match b[*i] {
            '"' => {
                *i += 1;
                return Ok(out);
            }
            '\\' => {
                *i += 1;
                let c = *b.get(*i).ok_or("unterminated escape")?;
                out.push(match c {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    'b' => '\u{8}',
                    'f' => '\u{c}',
                    'u' => {
                        let hex: String =
                            b.get(*i + 1..*i + 5).ok_or("short \\u")?.iter().collect();
                        *i += 4;
                        char::from_u32(u32::from_str_radix(&hex, 16).map_err(|_| "bad \\u")?)
                            .ok_or("bad code point")?
                    }
                    c => c,
                });
                *i += 1;
            }
            c => {
                out.push(c);
                *i += 1;
            }
        }
    }
    Err("unterminated string".into())
}

fn array(b: &[char], i: &mut usize) -> Result<Json, String> {
    *i += 1;
    let mut out = Vec::new();
    ws(b, i);
    if b.get(*i) == Some(&']') {
        *i += 1;
        return Ok(Json::Arr(out));
    }
    loop {
        out.push(value(b, i)?);
        ws(b, i);
        match b.get(*i) {
            Some(',') => *i += 1,
            Some(']') => {
                *i += 1;
                return Ok(Json::Arr(out));
            }
            _ => return Err(format!("expected `,` or `]` at character {}", i)),
        }
    }
}

fn object(b: &[char], i: &mut usize) -> Result<Json, String> {
    *i += 1;
    let mut m = BTreeMap::new();
    ws(b, i);
    if b.get(*i) == Some(&'}') {
        *i += 1;
        return Ok(Json::Obj(m));
    }
    loop {
        ws(b, i);
        if b.get(*i) != Some(&'"') {
            return Err(format!("expected a key at character {}", i));
        }
        let k = string(b, i)?;
        ws(b, i);
        if b.get(*i) != Some(&':') {
            return Err(format!("expected `:` at character {}", i));
        }
        *i += 1;
        m.insert(k, value(b, i)?);
        ws(b, i);
        match b.get(*i) {
            Some(',') => *i += 1,
            Some('}') => {
                *i += 1;
                return Ok(Json::Obj(m));
            }
            _ => return Err(format!("expected `,` or `}}` at character {}", i)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_stable_objects() {
        let j = Json::obj().set("b", 2).set("a", "x");
        assert_eq!(j.to_string(), r#"{"a":"x","b":2}"#);
    }

    #[test]
    fn absent_options_stay_absent() {
        let j = Json::obj()
            .set("a", 1)
            .set_opt("b", None::<String>)
            .set_opt("c", Some("y"));
        assert_eq!(j.to_string(), r#"{"a":1,"c":"y"}"#);
    }

    #[test]
    fn escapes_control_characters() {
        assert_eq!(
            Json::Str("a\"b\\c\nd".into()).to_string(),
            r#""a\"b\\c\nd""#
        );
    }

    #[test]
    fn round_trips() {
        let src = r#"{"a":[1,2,{"b":true,"c":null}],"d":"x\ny"}"#;
        assert_eq!(parse(src).unwrap().to_string(), src);
    }

    #[test]
    fn reads_nested_fixtures() {
        let j = parse(r#" { "cases": [ {"name": "one", "n": 3} ] } "#).unwrap();
        let c = &j.get("cases").unwrap().as_arr().unwrap()[0];
        assert_eq!(c.get("name").unwrap().as_str(), Some("one"));
        assert_eq!(c.get("n").unwrap().as_i64(), Some(3));
    }

    #[test]
    fn rejects_trailing_input() {
        assert!(parse("{} {}").is_err());
    }
}
