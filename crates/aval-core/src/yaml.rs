//! A parser for the restricted YAML dialect `aval` frontmatter is written in.
//!
//! Not a YAML implementation. The dialect is defined in SEMANTICS section 3.7
//! and covers exactly what a decision record needs: block mappings, block
//! sequences of mappings, flow sequences of scalars, block scalars, comments
//! and booleans. Anything outside it is REJECTED with a line number rather than
//! guessed at, because a frontmatter construct this parser misread would move a
//! decision head without saying so.
//!
//! Two properties matter more than breadth:
//!
//! * every node carries the line it came from, so a structural error can point
//!   at it; and
//! * duplicate mapping keys are an error, not a silent last-wins. YAML's own
//!   last-wins rule is how `decisions` would have quietly lost an entry had it
//!   been a mapping instead of a list.

use std::borrow::Cow;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Bool(bool),
    Seq(Vec<Node>),
    Map(Vec<(String, Node)>),
    Null,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub value: Value,
    pub line: usize,
}

impl Node {
    fn new(value: Value, line: usize) -> Self {
        Node { value, line }
    }

    pub fn as_str(&self) -> Option<&str> {
        match &self.value {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match &self.value {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_seq(&self) -> Option<&[Node]> {
        match &self.value {
            Value::Seq(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&[(String, Node)]> {
        match &self.value {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Node> {
        self.as_map()?
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    pub fn is_null(&self) -> bool {
        matches!(self.value, Value::Null)
    }

    /// What to call this shape in an error message.
    pub fn kind(&self) -> &'static str {
        match self.value {
            Value::Str(_) => "a string",
            Value::Bool(_) => "a boolean",
            Value::Seq(_) => "a list",
            Value::Map(_) => "a mapping",
            Value::Null => "empty",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct YamlError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for YamlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

fn err<T>(line: usize, message: impl Into<String>) -> Result<T, YamlError> {
    Err(YamlError {
        line,
        message: message.into(),
    })
}

/// One significant line: blank lines and whole-line comments are dropped.
///
/// `no` is 1-based and indexes back into the raw source, which is how a block
/// scalar reads the lines this dropped.
#[derive(Debug, Clone)]
struct Line {
    indent: usize,
    text: String,
    no: usize,
}

fn lex(src: &str) -> Result<Vec<Line>, YamlError> {
    let mut out = Vec::new();
    for (i, raw) in src.lines().enumerate() {
        let no = i + 1;
        if raw.contains('\t') && raw.trim_start_matches(' ').len() != raw.trim_start().len() {
            return err(no, "tabs cannot be used for indentation");
        }
        let trimmed = raw.trim_start_matches(' ');
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        out.push(Line {
            indent: raw.len() - trimmed.len(),
            text: trimmed.to_string(),
            no,
        });
    }
    Ok(out)
}

/// Strip a trailing comment. A `#` only opens one when preceded by whitespace,
/// so `choice: a#b` keeps its hash and `choice: a # b` does not.
///
/// A `#` inside quotes opens nothing. Without that, `choice: "Kong # the
/// gateway"` was silently truncated to `Kong` — the value parsed, so no check
/// fired, and the projection stated a decision nobody wrote. Quoting is the
/// documented way to protect a value, so it has to actually protect it.
///
/// A quote opens a scalar only as the first non-space byte, which is the same
/// rule `scalar` applies when it decides a value is quoted at all. Without it,
/// the apostrophe in `choice: it's fine # note` would open a string that never
/// closes and the comment would survive into the value.
fn strip_comment(s: &str) -> &str {
    let b = s.as_bytes();
    let start = b.iter().position(|c| *c != b' ' && *c != b'\t');
    let mut i = 0;
    let mut quote: Option<u8> = None;
    while i < b.len() {
        match quote {
            // Inside `"`, a backslash escapes the next byte; inside `'`, a
            // doubled quote does. Both mirror what `scalar` decodes, so this
            // agrees with the value that actually comes out.
            Some(b'"') => match b[i] {
                b'\\' => i += 1,
                b'"' => quote = None,
                _ => {}
            },
            Some(_) => {
                if b[i] == b'\'' {
                    if i + 1 < b.len() && b[i + 1] == b'\'' {
                        i += 1;
                    } else {
                        quote = None;
                    }
                }
            }
            None if Some(i) == start && (b[i] == b'"' || b[i] == b'\'') => quote = Some(b[i]),
            None if b[i] == b'#' && (i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\t') => {
                return s[..i].trim_end()
            }
            None => {}
        }
        i += 1;
    }
    s.trim_end()
}

fn scalar(raw: &str, line: usize) -> Result<Node, YamlError> {
    let s = strip_comment(raw).trim();
    if s.is_empty() {
        return Ok(Node::new(Value::Null, line));
    }
    if s == "true" {
        return Ok(Node::new(Value::Bool(true), line));
    }
    if s == "false" {
        return Ok(Node::new(Value::Bool(false), line));
    }
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        return Ok(Node::new(
            Value::Str(s[1..s.len() - 1].replace("\\\"", "\"")),
            line,
        ));
    }
    if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
        return Ok(Node::new(
            Value::Str(s[1..s.len() - 1].replace("''", "'")),
            line,
        ));
    }
    if s.starts_with('&') || s.starts_with('*') || s.starts_with('{') || s.starts_with('!') {
        return err(
            line,
            format!(
                "unsupported YAML construct `{}`; see SEMANTICS section 3.7",
                s
            ),
        );
    }
    Ok(Node::new(Value::Str(s.to_string()), line))
}

/// `[a, b, c]` — scalars only. A nested flow collection is out of dialect.
fn flow_seq(raw: &str, line: usize) -> Result<Node, YamlError> {
    let inner = raw.trim();
    let inner = &inner[1..inner.len() - 1];
    if inner.contains('[') || inner.contains('{') {
        return err(line, "nested flow collections are not supported");
    }
    let mut items = Vec::new();
    for part in inner.split(',') {
        let p = part.trim();
        if p.is_empty() {
            if inner.trim().is_empty() {
                continue;
            }
            return err(line, "empty item in a flow sequence");
        }
        items.push(scalar(p, line)?);
    }
    Ok(Node::new(Value::Seq(items), line))
}

/// `|`, `>`, with optional `-` or `+` chomping.
///
/// Read from the RAW source rather than from the lexed lines, because `lex`
/// drops blank lines and whole-line comments and inside a block scalar both are
/// CONTENT. A rule body (§2.4) is markdown carried in a pack: its paragraph
/// breaks are blank lines and its sub-headings begin with `#`, and the earlier
/// shape silently deleted both — a body that went through a pack came back with
/// its paragraphs run together and its headings gone, with nothing reporting
/// it. The lexed lines are still what the parser walks; this only reads past
/// them and then skips `i` forward to the first line it did not consume.
fn block_scalar(
    header: &str,
    lines: &[Line],
    raw: &[&str],
    i: &mut usize,
    parent_indent: usize,
    line: usize,
) -> Result<Node, YamlError> {
    let fold = header.starts_with('>');
    let chomp = header.chars().nth(1).unwrap_or(' ');
    if !matches!(chomp, '-' | '+' | ' ') || header.len() > 2 {
        return err(
            line,
            format!("unsupported block scalar header `{}`", header),
        );
    }
    let mut body: Vec<String> = Vec::new();
    let mut block_indent: Option<usize> = None;
    // `line` is 1-based and names the header's own line, so this starts at the
    // one after it.
    let mut at = line;
    while at < raw.len() {
        let text = raw[at];
        let trimmed = text.trim_start_matches(' ');
        let indent = text.len() - trimmed.len();
        if trimmed.is_empty() {
            // A blank line is part of the block when the block continues past
            // it. Trailing ones are decided by chomping, below.
            body.push(String::new());
            at += 1;
            continue;
        }
        if indent <= parent_indent {
            break;
        }
        let ind = *block_indent.get_or_insert(indent);
        if indent < ind {
            break;
        }
        body.push(text.get(ind..).unwrap_or("").to_string());
        at += 1;
    }
    while *i < lines.len() && lines[*i].no <= at {
        *i += 1;
    }

    // Trailing blank lines are chomping's business, not the body's.
    let mut end = body.len();
    while end > 0 && body[end - 1].is_empty() {
        end -= 1;
    }
    let trailing = body.len() - end;
    body.truncate(end);

    let mut text = if fold { folded(&body) } else { body.join("\n") };
    match chomp {
        '-' => {}
        '+' => {
            text.push('\n');
            for _ in 0..trailing {
                text.push('\n');
            }
        }
        // Clip: one trailing newline, and none at all for an empty block, which
        // is what YAML says and what makes `body: |` round-trip.
        _ => {
            if !text.is_empty() {
                text.push('\n');
            }
        }
    }
    Ok(Node::new(Value::Str(text), line))
}

/// Folding, `>`: lines join with a space, and a blank line is a real newline.
///
/// Plain `join(" ")` was right only while `lex` was deleting the blank lines —
/// with them present it would emit a double space where the author wrote a
/// paragraph break.
fn folded(body: &[String]) -> String {
    let mut out = String::new();
    let mut breaks = 0usize;
    for l in body {
        if l.is_empty() {
            breaks += 1;
            continue;
        }
        if out.is_empty() {
            out.push_str(l);
        } else if breaks > 0 {
            for _ in 0..breaks {
                out.push('\n');
            }
            out.push_str(l);
        } else {
            out.push(' ');
            out.push_str(l);
        }
        breaks = 0;
    }
    out
}

fn parse_block(
    lines: &[Line],
    raw: &[&str],
    i: &mut usize,
    indent: usize,
) -> Result<Node, YamlError> {
    if *i >= lines.len() {
        return err(lines.last().map_or(1, |l| l.no), "unexpected end of input");
    }
    if lines[*i].text.starts_with("- ") || lines[*i].text == "-" {
        parse_seq(lines, raw, i, indent)
    } else {
        parse_map(lines, raw, i, indent)
    }
}

fn parse_seq(
    lines: &[Line],
    raw: &[&str],
    i: &mut usize,
    indent: usize,
) -> Result<Node, YamlError> {
    let start = lines[*i].no;
    let mut items = Vec::new();
    while *i < lines.len() && lines[*i].indent == indent {
        let l = lines[*i].clone();
        if !(l.text.starts_with("- ") || l.text == "-") {
            break;
        }
        let rest = l.text[1..].trim_start();
        // Position of the item's own content, which is the indent of any
        // continuation lines belonging to it.
        let item_indent = indent + (l.text.len() - rest.len());
        *i += 1;
        if rest.is_empty() {
            items.push(parse_block(lines, raw, i, item_indent)?);
        } else if is_map_entry(rest) {
            let mut entries = Vec::new();
            parse_map_entry(rest, lines, raw, i, item_indent, l.no, &mut entries)?;
            parse_map_rest(lines, raw, i, item_indent, &mut entries)?;
            items.push(Node::new(Value::Map(entries), l.no));
        } else {
            items.push(scalar(rest, l.no)?);
        }
    }
    Ok(Node::new(Value::Seq(items), start))
}

/// `key: ...` at the head of a line. A plain scalar containing `: ` would be
/// ambiguous, which is why `choice:` values holding a colon must be quoted.
fn is_map_entry(s: &str) -> bool {
    match split_key(s) {
        Some((k, _)) => !k.is_empty() && !k.starts_with('-'),
        None => false,
    }
}

/// A mapping key and the text after its colon.
///
/// Keys may be plain, double-quoted or single-quoted. Single quotes are
/// prettier's choice under `singleQuote: true`, and prettier formats a
/// consumer's `.adr.yaml` at pre-commit: an `areas:` glob such as
/// `"packages/ui/**"` comes back as `'packages/ui/**'`. Reading that key with
/// its quotes still on made the glob malformed (1.7.1). Inside single quotes
/// `''` is one quote, as YAML says; that is the only escape the style has.
fn split_key(s: &str) -> Option<(Cow<'_, str>, &str)> {
    if let Some(rest) = s.strip_prefix('"') {
        let end = rest.find('"')?;
        let after = rest[end + 1..].trim_start();
        return after
            .strip_prefix(':')
            .map(|v| (Cow::Borrowed(&rest[..end]), v.trim_start()));
    }
    if let Some(rest) = s.strip_prefix('\'') {
        let b = rest.as_bytes();
        let mut j = 0;
        let end = loop {
            match b.get(j) {
                None => return None,
                Some(b'\'') if b.get(j + 1) == Some(&b'\'') => j += 2,
                Some(b'\'') => break j,
                Some(_) => j += 1,
            }
        };
        let after = rest[end + 1..].trim_start();
        let key = &rest[..end];
        let key = if key.contains("''") {
            Cow::Owned(key.replace("''", "'"))
        } else {
            Cow::Borrowed(key)
        };
        return after.strip_prefix(':').map(|v| (key, v.trim_start()));
    }
    let b = s.as_bytes();
    for (i, c) in b.iter().enumerate() {
        if *c == b':' && (i + 1 == b.len() || b[i + 1] == b' ') {
            return Some((Cow::Borrowed(s[..i].trim_end()), s[i + 1..].trim_start()));
        }
    }
    None
}

fn parse_map(
    lines: &[Line],
    raw: &[&str],
    i: &mut usize,
    indent: usize,
) -> Result<Node, YamlError> {
    let start = lines[*i].no;
    let mut entries = Vec::new();
    parse_map_rest(lines, raw, i, indent, &mut entries)?;
    Ok(Node::new(Value::Map(entries), start))
}

fn parse_map_rest(
    lines: &[Line],
    raw: &[&str],
    i: &mut usize,
    indent: usize,
    entries: &mut Vec<(String, Node)>,
) -> Result<(), YamlError> {
    while *i < lines.len() && lines[*i].indent == indent {
        let l = lines[*i].clone();
        if l.text.starts_with("- ") || l.text == "-" {
            break;
        }
        if !is_map_entry(&l.text) {
            return err(l.no, format!("expected `key: value`, found `{}`", l.text));
        }
        *i += 1;
        parse_map_entry(&l.text, lines, raw, i, indent, l.no, entries)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn parse_map_entry(
    text: &str,
    lines: &[Line],
    raw: &[&str],
    i: &mut usize,
    indent: usize,
    line: usize,
    entries: &mut Vec<(String, Node)>,
) -> Result<(), YamlError> {
    let (key, rest) = split_key(text).expect("checked by is_map_entry");
    let key = key.trim().to_string();
    if entries.iter().any(|(k, _)| *k == key) {
        return err(line, format!("duplicate key `{}`", key));
    }
    let rest = rest.trim();
    let value = if rest.is_empty() {
        // A child block is indented further; otherwise the value is empty.
        if *i < lines.len() && lines[*i].indent > indent {
            parse_block(lines, raw, i, lines[*i].indent)?
        } else {
            Node::new(Value::Null, line)
        }
    } else if rest.starts_with('[') {
        if !strip_comment(rest).trim().ends_with(']') {
            return err(line, "a flow sequence must close on the same line");
        }
        flow_seq(strip_comment(rest), line)?
    } else if rest.starts_with('|') || rest.starts_with('>') {
        block_scalar(strip_comment(rest).trim(), lines, raw, i, indent, line)?
    } else {
        scalar(rest, line)?
    };
    entries.push((key, value));
    Ok(())
}

/// Parse a complete frontmatter document. Always a mapping at the root.
pub fn parse(src: &str) -> Result<Node, YamlError> {
    let lines = lex(src)?;
    if lines.is_empty() {
        return Ok(Node::new(Value::Map(Vec::new()), 1));
    }
    let raw: Vec<&str> = src.lines().collect();
    let base = lines[0].indent;
    let mut i = 0;
    let node = parse_block(&lines, &raw, &mut i, base)?;
    if i < lines.len() {
        return err(
            lines[i].no,
            format!(
                "unexpected indentation; expected a key at column {}",
                base + 1
            ),
        );
    }
    Ok(node)
}

/// Whether two documents **declare** the same thing.
///
/// `Node` derives `PartialEq`, which includes `line`, so two files saying the
/// same thing with a blank line between different keys are unequal — and that
/// is the right default for a parser whose whole job is pointing at lines. It
/// is the wrong question for a vendored pack. A pack lives in repositories
/// whose pre-commit hook runs a formatter over every file, and a formatter that
/// requoted a scalar or moved a blank line has changed nothing this tool reads:
/// the file still declares the decisions the producer published. Comparing
/// bytes made that file "edited" by nobody, and re-vendoring it rewrote it
/// again on the next run.
///
/// So: values, and nothing else. Line numbers, comments, quoting style and
/// blank lines are all gone by the time a document is a `Node`.
///
/// Sequences compare **in order** — `replaces` is a list whose order the writer
/// fixes, and a reordering is a different document until something says
/// otherwise. Mappings compare **without** order, by key set and value, because
/// the dialect forbids a duplicate key (so a key set is a set) and the pack
/// writer sorts keys that a hand-written file need not.
pub fn same_values(a: &Node, b: &Node) -> bool {
    match (&a.value, &b.value) {
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Null, Value::Null) => true,
        (Value::Seq(x), Value::Seq(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(i, j)| same_values(i, j))
        }
        // Equal lengths plus every key of `x` matched in `y` is equal key sets,
        // given the parser has already refused a duplicate key in either.
        (Value::Map(x), Value::Map(y)) => {
            x.len() == y.len()
                && x.iter().all(|(k, v)| {
                    y.iter()
                        .find(|(k2, _)| k2 == k)
                        .is_some_and(|(_, v2)| same_values(v, v2))
                })
        }
        _ => false,
    }
}

/// Split `---\n<frontmatter>\n---\n<body>`.
///
/// Returns the frontmatter text and the line the frontmatter starts on, so a
/// parse error can be reported against the file rather than against the slice.
pub fn split_frontmatter(src: &str) -> Option<(String, usize, &str)> {
    let rest = src.strip_prefix("---\n")?;
    let end = rest
        .find("\n---\n")
        .or_else(|| rest.strip_suffix("\n---").map(|s| s.len()))?;
    let body_at = (end + 5).min(rest.len());
    Some((rest[..end].to_string(), 2, &rest[body_at..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_quoted_keys_are_keys() {
        let n = parse("areas:\n  'packages/ui/**': [ui]\n  \"cmd/**\": [cli]\n  'it''s': []\n")
            .expect("parses");
        let m = n.get("areas").and_then(|a| a.as_map()).expect("map");
        let keys: Vec<&str> = m.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["packages/ui/**", "cmd/**", "it's"]);
    }

    fn p(s: &str) -> Node {
        parse(s).expect("parses")
    }

    #[test]
    fn scalars_booleans_and_comments() {
        let n = p("id: ADR-0001\nstatus: accepted  # trailing\nfirst: true\nempty:\n");
        assert_eq!(n.get("id").unwrap().as_str(), Some("ADR-0001"));
        assert_eq!(n.get("status").unwrap().as_str(), Some("accepted"));
        assert_eq!(n.get("first").unwrap().as_bool(), Some(true));
        assert!(n.get("empty").unwrap().is_null());
    }

    #[test]
    fn a_hash_without_leading_space_stays_in_the_value() {
        let n = p("choice: build#42\n");
        assert_eq!(n.get("choice").unwrap().as_str(), Some("build#42"));
    }

    #[test]
    fn flow_sequences() {
        let n = p("replaces: [ADR-0009, ADR-0011]\nscopes: []\n");
        let r = n.get("replaces").unwrap().as_seq().unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].as_str(), Some("ADR-0011"));
        assert!(n.get("scopes").unwrap().as_seq().unwrap().is_empty());
    }

    #[test]
    fn nested_mappings() {
        let n = p("keys:\n  storage.object-store:\n    description: The store\n");
        let d = n
            .get("keys")
            .unwrap()
            .get("storage.object-store")
            .unwrap()
            .get("description")
            .unwrap();
        assert_eq!(d.as_str(), Some("The store"));
    }

    #[test]
    fn sequence_of_mappings() {
        let n = p("decisions:\n  - key: a\n    choice: one\n  - key: b\n    retire: true\n");
        let s = n.get("decisions").unwrap().as_seq().unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].get("choice").unwrap().as_str(), Some("one"));
        assert_eq!(s[1].get("retire").unwrap().as_bool(), Some(true));
    }

    #[test]
    fn folded_block_scalar() {
        let n = p("reason: >-\n  the uplink moved\n  behind CGNAT\nnext: x\n");
        assert_eq!(
            n.get("reason").unwrap().as_str(),
            Some("the uplink moved behind CGNAT")
        );
        assert_eq!(n.get("next").unwrap().as_str(), Some("x"));
    }

    #[test]
    fn literal_block_scalar_keeps_newlines() {
        let n = p("note: |-\n  one\n  two\n");
        assert_eq!(n.get("note").unwrap().as_str(), Some("one\ntwo"));
    }

    /// What a pack's `body: |` has to survive: a blank line between paragraphs
    /// and a markdown sub-heading. `lex` drops both — a blank line and a line
    /// opening with `#` — so before this the text came back with its paragraphs
    /// run together and its headings deleted, silently.
    #[test]
    fn a_literal_block_keeps_blank_lines_and_hash_lines() {
        let n = p("body: |\n  one\n\n  ### two\n  three\nnext: x\n");
        assert_eq!(
            n.get("body").unwrap().as_str(),
            Some("one\n\n### two\nthree\n")
        );
        assert_eq!(n.get("next").unwrap().as_str(), Some("x"));
    }

    /// The round trip the pack relies on: a body rendered indented under `|`
    /// reads back as itself once the clip newline is trimmed, including a body
    /// whose source ended without a trailing newline.
    #[test]
    fn a_body_round_trips_through_a_literal_block() {
        for body in [
            "one\n\ntwo",
            "a\n\n\nb",
            "- list\n- items\n\n```\ncode block\n```",
            "single line",
            "trailing spaces kept mid-body\n\nlast",
        ] {
            let mut src = String::from("body: |\n");
            for l in body.lines() {
                if l.is_empty() {
                    src.push('\n');
                } else {
                    src.push_str(&format!("  {}\n", l));
                }
            }
            src.push_str("after: y\n");
            let n = p(&src);
            let got = n.get("body").unwrap().as_str().unwrap();
            assert_eq!(got.trim_end_matches('\n'), body, "{:?}", src);
            assert_eq!(n.get("after").unwrap().as_str(), Some("y"));
        }
    }

    #[test]
    fn blank_lines_inside_a_folded_block_are_paragraph_breaks() {
        let n = p("reason: >-\n  one\n  two\n\n  three\n");
        assert_eq!(n.get("reason").unwrap().as_str(), Some("one two\nthree"));
    }

    #[test]
    fn chomping_decides_the_trailing_newlines() {
        assert_eq!(p("a: |-\n  x\n\n\n").get("a").unwrap().as_str(), Some("x"));
        assert_eq!(p("a: |\n  x\n\n\n").get("a").unwrap().as_str(), Some("x\n"));
        assert_eq!(
            p("a: |+\n  x\n\n\n").get("a").unwrap().as_str(),
            Some("x\n\n\n")
        );
    }

    #[test]
    fn every_node_carries_its_line() {
        let n = p("id: ADR-0001\nstatus: accepted\n");
        assert_eq!(n.get("id").unwrap().line, 1);
        assert_eq!(n.get("status").unwrap().line, 2);
    }

    #[test]
    fn duplicate_keys_are_an_error_not_last_wins() {
        let e = parse("a: 1\na: 2\n").unwrap_err();
        assert_eq!(e.line, 2);
        assert!(e.message.contains("duplicate key"));
    }

    #[test]
    fn tabs_in_indentation_are_rejected() {
        let e = parse("a:\n\tb: 1\n").unwrap_err();
        assert!(e.message.contains("tab"));
    }

    #[test]
    fn out_of_dialect_constructs_are_rejected_with_a_line() {
        for src in [
            "a: &anchor x\n",
            "a: *alias\n",
            "a: !!str x\n",
            "a: {b: 1}\n",
        ] {
            let e = parse(src).unwrap_err();
            assert_eq!(e.line, 1, "{}", src);
            assert!(e.message.contains("SEMANTICS"), "{}", src);
        }
    }

    #[test]
    fn a_plain_value_may_contain_a_colon() {
        // Only the FIRST `: ` splits the key, so the rest is the value as
        // written. Quoting it is allowed but not required.
        let n = p("choice: Kafka: the sequel\n");
        assert_eq!(n.get("choice").unwrap().as_str(), Some("Kafka: the sequel"));
    }

    #[test]
    fn a_line_that_is_not_a_mapping_entry_is_rejected() {
        let e = parse("id: ADR-1\nnonsense\n").unwrap_err();
        assert_eq!(e.line, 2);
    }

    #[test]
    fn frontmatter_splits_from_body() {
        let (fm, at, body) = split_frontmatter("---\nid: ADR-1\n---\n# Title\n").unwrap();
        assert_eq!(fm, "id: ADR-1");
        assert_eq!(at, 2);
        assert_eq!(body, "# Title\n");
    }

    #[test]
    fn a_file_without_frontmatter_is_none() {
        assert!(split_frontmatter("# Title\n").is_none());
    }

    /// The case this exists for: a formatter went over a vendored pack. It
    /// requoted every scalar, dropped a blank line, added a comment and
    /// reordered two keys, and changed nothing the tool reads.
    #[test]
    fn a_reformatted_document_declares_the_same_thing() {
        let a = p("aval: \"1.3.0\"\nscopes:\n  - \"browser\"\n\nkeys:\n  x.y:\n    description: \"it's here\"\n");
        let b = p(
            "# formatted\nkeys:\n  x.y:\n    description: 'it''s here'\naval: '1.3.0'\nscopes: [browser]\n",
        );
        assert_ne!(a, b, "the derived equality is byte-ish, which is the point");
        assert!(same_values(&a, &b));
    }

    #[test]
    fn a_changed_declaration_is_not_the_same_thing() {
        let base = p("a: \"one\"\nb:\n  - \"x\"\n  - \"y\"\n");
        // A changed scalar.
        assert!(!same_values(
            &base,
            &p("a: \"two\"\nb:\n  - \"x\"\n  - \"y\"\n")
        ));
        // A missing key, in either direction.
        let short = p("a: \"one\"\n");
        assert!(!same_values(&base, &short));
        assert!(!same_values(&short, &base));
        // An added key.
        assert!(!same_values(
            &base,
            &p("a: \"one\"\nb:\n  - \"x\"\n  - \"y\"\nc: \"new\"\n")
        ));
        // A reordered sequence: `replaces` is a list whose order is the
        // writer's, so this is a different document.
        assert!(!same_values(
            &base,
            &p("a: \"one\"\nb:\n  - \"y\"\n  - \"x\"\n")
        ));
        // A value that changed shape. `true` and `"true"` are two
        // declarations, which is why the pack writer quotes unconditionally.
        assert!(!same_values(&p("a: true\n"), &p("a: \"true\"\n")));
        // Empty is not an empty list.
        assert!(!same_values(&p("a:\n"), &p("a: []\n")));
        assert!(same_values(&p("a:\n"), &p("a: # gone\n")));
    }
}
