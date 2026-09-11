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
#[derive(Debug, Clone)]
struct Line {
    indent: usize,
    text: String,
    no: usize,
    /// Raw body kept for block scalars, which must preserve relative indent.
    raw: String,
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
            raw: raw.to_string(),
        });
    }
    Ok(out)
}

/// Strip a trailing comment. A `#` only opens one when preceded by whitespace,
/// so `choice: a#b` keeps its hash and `choice: a # b` does not.
fn strip_comment(s: &str) -> &str {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'#' && (i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\t') {
            return s[..i].trim_end();
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
fn block_scalar(
    header: &str,
    lines: &[Line],
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
    while *i < lines.len() && lines[*i].indent > parent_indent {
        let l = &lines[*i];
        let ind = *block_indent.get_or_insert(l.indent);
        if l.indent < ind {
            break;
        }
        let content = l.raw.get(ind..).unwrap_or("").to_string();
        body.push(content);
        *i += 1;
    }
    let mut text = if fold {
        body.join(" ")
    } else {
        body.join("\n")
    };
    match chomp {
        '-' => {}
        _ => text.push('\n'),
    }
    Ok(Node::new(Value::Str(text), line))
}

fn parse_block(lines: &[Line], i: &mut usize, indent: usize) -> Result<Node, YamlError> {
    if *i >= lines.len() {
        return err(lines.last().map_or(1, |l| l.no), "unexpected end of input");
    }
    if lines[*i].text.starts_with("- ") || lines[*i].text == "-" {
        parse_seq(lines, i, indent)
    } else {
        parse_map(lines, i, indent)
    }
}

fn parse_seq(lines: &[Line], i: &mut usize, indent: usize) -> Result<Node, YamlError> {
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
            items.push(parse_block(lines, i, item_indent)?);
        } else if is_map_entry(rest) {
            let mut entries = Vec::new();
            parse_map_entry(rest, lines, i, item_indent, l.no, &mut entries)?;
            parse_map_rest(lines, i, item_indent, &mut entries)?;
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

fn split_key(s: &str) -> Option<(&str, &str)> {
    if let Some(rest) = s.strip_prefix('"') {
        let end = rest.find('"')?;
        let after = rest[end + 1..].trim_start();
        return after
            .strip_prefix(':')
            .map(|v| (&rest[..end], v.trim_start()));
    }
    let b = s.as_bytes();
    for (i, c) in b.iter().enumerate() {
        if *c == b':' && (i + 1 == b.len() || b[i + 1] == b' ') {
            return Some((s[..i].trim_end(), s[i + 1..].trim_start()));
        }
    }
    None
}

fn parse_map(lines: &[Line], i: &mut usize, indent: usize) -> Result<Node, YamlError> {
    let start = lines[*i].no;
    let mut entries = Vec::new();
    parse_map_rest(lines, i, indent, &mut entries)?;
    Ok(Node::new(Value::Map(entries), start))
}

fn parse_map_rest(
    lines: &[Line],
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
        parse_map_entry(&l.text, lines, i, indent, l.no, entries)?;
    }
    Ok(())
}

fn parse_map_entry(
    text: &str,
    lines: &[Line],
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
            parse_block(lines, i, lines[*i].indent)?
        } else {
            Node::new(Value::Null, line)
        }
    } else if rest.starts_with('[') {
        if !strip_comment(rest).trim().ends_with(']') {
            return err(line, "a flow sequence must close on the same line");
        }
        flow_seq(strip_comment(rest), line)?
    } else if rest.starts_with('|') || rest.starts_with('>') {
        block_scalar(strip_comment(rest).trim(), lines, i, indent, line)?
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
    let base = lines[0].indent;
    let mut i = 0;
    let node = parse_block(&lines, &mut i, base)?;
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
}
