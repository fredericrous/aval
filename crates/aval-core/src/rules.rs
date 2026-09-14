//! Rule files: markdown whose headings are the declarations.
//!
//! A rule file is prose a person reads and a grammar a tool reads, in one
//! document, because the alternative — a YAML list of rules beside a markdown
//! file explaining them — is two files that drift, which is the failure this
//! whole tool exists to end. The heading carries the id and the level; the
//! first paragraph under it is the statement the session hook prints; the rest
//! is the translation, fetched by id when somebody asks why.
//!
//! # Why the grammar is exact
//!
//! `## <id> [constraint]` and `## <id> [heuristic]`, and nothing else. A `## `
//! line that does not match is an error rather than body text: a missing
//! bracket or a misspelled level would otherwise fold a rule into the previous
//! rule's body, where it is still perfectly readable prose — so a reviewer
//! reading the pull request sees a rule and the tool has none. Nobody is told.
//! The same argument rules out a `# ` heading after the first rule: it reads as
//! the start of a second document, and everything under it would silently
//! become one rule's body.
//!
//! Fenced code is exempt from both, because a `## ` inside a fence is an
//! example — a shell comment, a diff hunk — and a check that cried wolf on
//! sample code would be turned off, and then it would catch nothing.
//!
//! # What is not here
//!
//! No status, no supersession, no dates. A rule's authority is the authority of
//! the record that adopts it, and that record is what the graph already tracks
//! (SEMANTICS section 2.4). Editing a rule's wording is an edit, reviewed in the
//! pull request; changing what it means is a new id.

use crate::model::*;
use crate::yaml;

const FILE_FIELDS: &[&str] = &["adopts", "source"];

fn a(msg: impl Into<String>) -> Finding {
    Finding::new(Layer::A, "rules-parse", msg)
}

/// Why `id` is not a usable rule id, or `None` if it is.
///
/// The key grammar of SEMANTICS section 1.1, applied to a second namespace: a
/// dotted lowercase identifier, at least two segments. Two segments because a
/// bare `names` is a word and `names.reveal-intent` is an address — an id that
/// short would collide across imported rule sets as soon as there were two.
///
/// Rule ids and decision keys are separate namespaces and a rule id equal to a
/// key is not an error. They are never looked up in the same place: `resolve`
/// takes a key, `rule` takes a rule id.
pub fn bad_id(id: &str) -> Option<String> {
    if id.is_empty() {
        return Some("a rule id cannot be empty".into());
    }
    if id.len() > 64 {
        return Some(format!("`{}` is longer than 64 characters", id));
    }
    let segments: Vec<&str> = id.split('.').collect();
    if segments.len() < 2 {
        return Some(format!(
            "`{}` has one segment; a rule id is dotted, like `names.reveal-intent`",
            id
        ));
    }
    for s in &segments {
        if s.is_empty() {
            return Some(format!("`{}` has an empty segment", id));
        }
        if let Some(c) = s
            .chars()
            .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-'))
        {
            return Some(format!(
                "`{}` carries `{}`; a rule id holds lowercase letters, digits, `-` and `.`",
                id, c
            ));
        }
    }
    None
}

/// A heading line, split into id and level, or `None` when it is not one.
///
/// Exact by construction: two hashes, one space, the id, one space, the level
/// in brackets, nothing else. Anything looser would accept the typo this
/// grammar exists to reject.
fn heading(line: &str) -> Option<(&str, Level)> {
    let rest = line.strip_prefix("## ")?;
    let (id, tail) = rest.rsplit_once(' ')?;
    let level = tail.strip_prefix('[')?.strip_suffix(']')?;
    Level::parse(level).map(|l| (id, l))
}

/// True for a fence line, which opens or closes a region nothing is read from.
fn is_fence(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("```") || t.starts_with("~~~")
}

/// Where one rule's text begins and ends, in lines.
struct Span {
    id: String,
    level: Level,
    line: usize,
    /// Index of the first body line, exclusive of the heading.
    from: usize,
    to: usize,
}

/// Parse one rules file.
///
/// `file` is a repository-relative path and is treated as an opaque label, as
/// `parse::adr` treats its own: it is copied into findings and onto each rule.
/// Nothing here touches the filesystem.
///
/// The file is refused whole, like a pack and for the same reason: a half-read
/// rules file is a set of constraints with one silently missing, and a
/// constraint nobody is shown is one nobody follows.
pub fn parse(file: &str, src: &str) -> Result<Vec<Rule>, Vec<Finding>> {
    let mut out = Vec::new();
    let Some((fm, offset, body)) = yaml::split_frontmatter(src) else {
        return Err(vec![a(
            "no YAML frontmatter; a rules file must begin with `---` and name \
             the record that adopts it",
        )
        .in_file(file)]);
    };
    // The body starts after the closing `---`, so a line number inside it is
    // its own index plus everything the frontmatter took.
    let body_offset = offset + fm.lines().count();

    let doc = match yaml::parse(&fm) {
        Ok(d) => d,
        Err(e) => return Err(vec![a(e.message).at(file, e.line + offset - 1)]),
    };
    if doc.as_map().is_none() {
        return Err(vec![a(format!(
            "frontmatter must be a mapping, found {}",
            doc.kind()
        ))
        .in_file(file)]);
    }
    for (k, v) in doc.as_map().unwrap_or(&[]) {
        if !FILE_FIELDS.contains(&k.as_str()) {
            let hint = suggest(k, FILE_FIELDS.iter().copied())
                .map(|s| format!("; did you mean `{}`", s))
                .unwrap_or_default();
            out.push(
                a(format!("unknown rules-file field `{}`{}", k, hint))
                    .at(file, v.line + offset - 1),
            );
        }
    }

    let adopts = match doc.get("adopts").and_then(|n| n.as_str()) {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        Some(_) | None => {
            out.push(
                a(
                    "missing `adopts`; a rule carries the authority of the record \
                     that adopts it, so there is no such thing as an unadopted one",
                )
                .in_file(file),
            );
            String::new()
        }
    };
    let source = match doc.get("source") {
        None => None,
        Some(n) => match n.as_str() {
            Some(s) => {
                let line = n.line + offset - 1;
                if s.contains('\n') {
                    out.push(a("`source` must be a single line").at(file, line));
                }
                if let Some(c) = unprintable(s) {
                    out.push(
                        a(format!(
                            "`source` carries U+{:04X}, which is not printable text",
                            c as u32
                        ))
                        .at(file, line),
                    );
                }
                Some(s.trim().to_string())
            }
            None => {
                out.push(
                    a(format!("`source` must be a string, found {}", n.kind()))
                        .at(file, n.line + offset - 1),
                );
                None
            }
        },
    };

    let lines: Vec<&str> = body.lines().collect();
    let mut spans: Vec<Span> = Vec::new();
    let mut fenced = false;
    for (i, raw) in lines.iter().enumerate() {
        let at = body_offset + i + 1;
        if is_fence(raw) {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if raw.starts_with("## ") || *raw == "##" {
            match heading(raw) {
                Some((id, level)) => {
                    if let Some(m) = bad_id(id) {
                        out.push(a(m).at(file, at));
                    }
                    if let Some(prev) = spans.last_mut() {
                        prev.to = i;
                    }
                    spans.push(Span {
                        id: id.to_string(),
                        level,
                        line: at,
                        from: i + 1,
                        to: lines.len(),
                    });
                }
                None => out.push(
                    a(format!(
                        "`{}` is not a rule heading; a rule reads \
                             `## <id> [constraint]` or `## <id> [heuristic]`, and a \
                             heading that nearly does would become the previous \
                             rule's body with nobody told",
                        raw.trim_end()
                    ))
                    .at(file, at),
                ),
            }
            continue;
        }
        // A second `# ` heading reads as the start of another document, and
        // everything under it would silently become one rule's body.
        if raw.starts_with("# ") && !spans.is_empty() {
            out.push(
                a(format!(
                    "`{}` is a document heading after a rule; a rules file is \
                         one document, and rules are `## ` headings",
                    raw.trim_end()
                ))
                .at(file, at),
            );
        }
    }

    if spans.is_empty() {
        out.push(
            a(
                "this file is listed in `rules` and declares none; a rule is a \
                 `## <id> [constraint]` or `## <id> [heuristic]` heading",
            )
            .in_file(file),
        );
    }

    let mut rules = Vec::new();
    for s in &spans {
        let text = &lines[s.from..s.to];
        let (statement, body) = split_statement(text);
        if statement.is_empty() {
            out.push(
                a(format!(
                    "`{}` has no statement; the first paragraph under the \
                         heading is the one line that is printed",
                    s.id
                ))
                .at(file, s.line),
            );
            continue;
        }
        if let Some(c) = unprintable(&statement) {
            out.push(
                a(format!(
                    "`{}` carries U+{:04X} in its statement, which is not \
                         printable text; this line is read by people and by agents \
                         and must look the same to both",
                    s.id, c as u32
                ))
                .at(file, s.line),
            );
            continue;
        }
        // The body travels in a pack, as a literal block scalar, and reaches an
        // agent through `aval rule`. Both make it the same text a reviewer
        // reads, so section 3.7's rule applies to it as much as to a `choice` —
        // and a tab, which is a control character, is also the one thing the
        // frontmatter dialect cannot carry back out of a block scalar.
        if let Some(c) = unprintable(&body) {
            out.push(
                a(format!(
                    "`{}` carries U+{:04X} in its body, which is not printable \
                     text; a tab in particular cannot survive the trip through a \
                     pack, so indent with spaces",
                    s.id, c as u32
                ))
                .at(file, s.line),
            );
            continue;
        }
        let mut rule = Rule::new(
            s.id.clone(),
            s.level,
            AdrId::new(adopts.clone()),
            statement,
            file,
        );
        rule.body = body;
        rule.source = source.clone();
        rules.push(rule);
    }

    // Two rules with one id, inside one file. The cross-file case is
    // `rule-id-unique`, which needs the whole corpus; this one needs only the
    // file and reports it where the author can see both.
    for (i, r) in rules.iter().enumerate() {
        if rules[..i].iter().any(|p| p.id == r.id) {
            out.push(a(format!("`{}` is declared twice in this file", r.id)).in_file(file));
        }
    }

    if out.is_empty() {
        Ok(rules)
    } else {
        Err(out)
    }
}

/// The first paragraph, joined with single spaces, and everything after it.
///
/// Joined rather than kept as written because the statement is printed as one
/// line — by the hook, by `aval rules` — and a hard wrap in the source is a
/// typographic choice, not part of what the rule says.
fn split_statement(lines: &[&str]) -> (String, String) {
    let mut i = 0;
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    let start = i;
    while i < lines.len() && !lines[i].trim().is_empty() {
        i += 1;
    }
    let statement = lines[start..i]
        .iter()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();
    let body = lines[i.min(lines.len())..].join("\n").trim().to_string();
    (statement, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FM: &str = "---\nadopts: ADR-0011\nsource: Clean Code (Robert C. Martin, 2008)\n---\n";

    fn ok(body: &str) -> Vec<Rule> {
        parse("docs/principles/clean-code.md", &format!("{}{}", FM, body)).expect("parses")
    }

    fn err(body: &str) -> Vec<Finding> {
        parse("docs/principles/clean-code.md", &format!("{}{}", FM, body)).unwrap_err()
    }

    #[test]
    fn a_rule_file_parses() {
        let r = ok("# Clean Code, restated\n\nIntro prose, ignored.\n\n\
                    ## names.reveal-intent [constraint]\n\n\
                    Names reveal intention: an identifier says what it holds,\n\
                    in the vocabulary of the domain.\n\n\
                    Body, with a reason.\n\n\
                    ### A sub-heading\n\nMore body.\n\n\
                    ## functions.few-arguments [heuristic]\n\n\
                    A function takes no more inputs than it uses.\n");
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].id, "names.reveal-intent");
        assert_eq!(r[0].level, Level::Constraint);
        assert_eq!(r[0].adopts, "ADR-0011");
        assert_eq!(
            r[0].statement,
            "Names reveal intention: an identifier says what it holds, in the \
             vocabulary of the domain."
        );
        assert!(r[0].body.starts_with("Body, with a reason."));
        assert!(r[0].body.contains("### A sub-heading"));
        assert!(r[0].body.ends_with("More body."));
        assert_eq!(
            r[0].source.as_deref(),
            Some("Clean Code (Robert C. Martin, 2008)")
        );
        assert_eq!(r[1].level, Level::Heuristic);
        // The last rule has no body, and an empty body is not an error.
        assert_eq!(r[1].body, "");
    }

    #[test]
    fn a_near_miss_heading_is_an_error_not_body_text() {
        for bad in [
            "## names.reveal-intent [constrain]",
            "## names.reveal-intent constraint",
            "## names.reveal-intent [constraint] ",
            "## names.reveal-intent[constraint]",
            "## Names",
        ] {
            let e = err(&format!("## a.b [constraint]\n\nOne.\n\n{}\n\nTwo.\n", bad));
            assert!(
                e.iter()
                    .any(|f| f.message.contains("is not a rule heading")),
                "{}: {:?}",
                bad,
                e
            );
        }
    }

    #[test]
    fn a_hash_inside_a_fence_is_example_text() {
        let r = ok("## a.b [constraint]\n\nOne.\n\n```sh\n## not a heading\n# nor this\n```\n");
        assert_eq!(r.len(), 1);
        assert!(r[0].body.contains("## not a heading"));
    }

    #[test]
    fn a_document_heading_after_a_rule_is_an_error() {
        let e = err("## a.b [constraint]\n\nOne.\n\n# Another document\n\nText.\n");
        assert!(
            e.iter().any(|f| f.message.contains("document heading")),
            "{:?}",
            e
        );
    }

    #[test]
    fn a_rule_needs_a_statement() {
        let e = err("## a.b [constraint]\n\n## c.d [heuristic]\n\nTwo.\n");
        assert!(e[0].message.contains("has no statement"), "{:?}", e);
    }

    #[test]
    fn a_file_with_no_rule_is_an_error() {
        let e = err("# Principles\n\nJust prose.\n");
        assert!(e[0].message.contains("declares none"), "{:?}", e);
    }

    #[test]
    fn the_id_grammar_is_the_key_grammar() {
        assert!(bad_id("names.reveal-intent").is_none());
        assert!(bad_id("a.b.c").is_none());
        assert!(bad_id("names").is_some());
        assert!(bad_id("Names.Reveal").is_some());
        assert!(bad_id("names..reveal").is_some());
        assert!(bad_id("names.reveal_intent").is_some());
        let e = err("## Names.Reveal [constraint]\n\nOne.\n");
        assert!(e[0].message.contains("lowercase"), "{:?}", e);
    }

    #[test]
    fn frontmatter_is_checked() {
        let e = parse(
            "r.md",
            "---\nsource: A book\n---\n## a.b [constraint]\n\nOne.\n",
        )
        .unwrap_err();
        assert!(e[0].message.contains("missing `adopts`"), "{:?}", e);

        let e = parse(
            "r.md",
            "---\nadopts: ADR-1\nadopted: ADR-2\n---\n## a.b [constraint]\n\nOne.\n",
        )
        .unwrap_err();
        assert!(e[0].message.contains("unknown rules-file field"), "{:?}", e);
        assert!(e[0].message.contains("did you mean `adopts`"), "{:?}", e);

        let e = parse("r.md", "## a.b [constraint]\n\nOne.\n").unwrap_err();
        assert!(e[0].message.contains("no YAML frontmatter"), "{:?}", e);
    }

    #[test]
    fn a_statement_must_be_printable() {
        let e = err("## a.b [constraint]\n\nOne\u{202E}two.\n");
        assert!(e[0].message.contains("U+202E"), "{:?}", e);
    }

    #[test]
    fn one_id_twice_in_one_file_is_an_error() {
        let e = err("## a.b [constraint]\n\nOne.\n\n## a.b [heuristic]\n\nTwo.\n");
        assert!(e[0].message.contains("declared twice"), "{:?}", e);
    }

    #[test]
    fn findings_carry_the_line_of_the_heading() {
        // Four frontmatter lines, then the body: the reported line must be the
        // one in the FILE, not in the slice the parser was handed.
        let e = err("\n\n## nope\n");
        assert_eq!(e[0].line, Some(7), "{:?}", e);
    }
}
