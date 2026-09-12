//! Frontmatter and registry text into the model, with the Layer A checks that
//! need only one document to decide.
//!
//! Unknown fields are rejected rather than ignored. A misspelled `replacess:`
//! that parsed into nothing would drop a supersession edge and move a head
//! without saying so, which is the whole failure this tool exists to prevent.

use crate::model::*;
use crate::yaml::{self, Node};

const DOC_FIELDS: &[&str] = &["id", "status", "decisions"];
const ENTRY_FIELDS: &[&str] = &[
    "key",
    "scope",
    "choice",
    "retire",
    "first",
    "replaces",
    "overrides",
    "reason",
];
const REGISTRY_FIELDS: &[&str] = &["dir", "sources", "packs", "scopes", "keys"];
const KEYDEF_FIELDS: &[&str] = &["description", "scopes"];

fn a(check: &'static str, msg: impl Into<String>) -> Finding {
    Finding::new(Layer::A, check, msg)
}

fn unknown_fields(node: &Node, allowed: &[&str], what: &str, file: &str, out: &mut Vec<Finding>) {
    let Some(map) = node.as_map() else { return };
    for (k, v) in map {
        if !allowed.contains(&k.as_str()) {
            let hint = suggest(k, allowed.iter().copied())
                .map(|s| format!("; did you mean `{}`", s))
                .unwrap_or_default();
            out.push(
                a(
                    "frontmatter-parses",
                    format!("unknown {} field `{}`{}", what, k, hint),
                )
                .at(file, v.line),
            );
        }
    }
}

fn want_str(node: &Node, key: &str, file: &str, out: &mut Vec<Finding>) -> Option<String> {
    let v = node.get(key)?;
    match v.as_str() {
        Some(s) => Some(s.to_string()),
        None => {
            out.push(
                a(
                    "frontmatter-parses",
                    format!("`{}` must be a string, found {}", key, v.kind()),
                )
                .at(file, v.line),
            );
            None
        }
    }
}

/// `want_str`, and the value must be one line.
///
/// A markdown table row has no escape for a newline, so a multi-line `choice`
/// would split one record across several rows and corrupt the projection. The
/// YAML dialect accepts block scalars, so this is reachable by writing exactly
/// what the dialect allows. Rejecting at parse time is cheaper than encoding,
/// and a choice that needs a second line is not a choice.
fn want_line(node: &Node, key: &str, file: &str, out: &mut Vec<Finding>) -> Option<String> {
    let v = want_str(node, key, file, out)?;
    if v.contains('\n') {
        out.push(
            a(
                "frontmatter-parses",
                format!("`{}` must be a single line; it carries a newline", key),
            )
            .at(file, node.get(key).map_or(node.line, |n| n.line)),
        );
        // The value is still returned. The finding above already fails the
        // parse; returning None here would additionally make the entry look
        // as though it declared neither `choice` nor `retire`, and report a
        // second error about a field the author did write.
    }
    Some(v)
}

fn want_flag(node: &Node, key: &str, file: &str, out: &mut Vec<Finding>) -> bool {
    let Some(v) = node.get(key) else { return false };
    match v.as_bool() {
        Some(b) => b,
        None => {
            out.push(
                a(
                    "frontmatter-parses",
                    format!("`{}` must be true or false, found {}", key, v.kind()),
                )
                .at(file, v.line),
            );
            false
        }
    }
}

fn want_str_list(node: &Node, key: &str, file: &str, out: &mut Vec<Finding>) -> Vec<String> {
    let Some(v) = node.get(key) else {
        return Vec::new();
    };
    let Some(items) = v.as_seq() else {
        out.push(
            a(
                "frontmatter-parses",
                format!("`{}` must be a list, found {}", key, v.kind()),
            )
            .at(file, v.line),
        );
        return Vec::new();
    };
    let mut res = Vec::new();
    for it in items {
        match it.as_str() {
            Some(s) => res.push(s.to_string()),
            None => out.push(
                a(
                    "frontmatter-parses",
                    format!("`{}` must hold strings, found {}", key, it.kind()),
                )
                .at(file, it.line),
            ),
        }
    }
    res
}

/// Where a record was found, which decides how its `id` is judged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Under the registry's `dir`, identified by a numeric filename prefix.
    Numbered,
    /// Named outright in the registry's `sources`.
    Listed,
}

/// Why an id is not a usable slug, or `None` if it is.
fn bad_slug(id: &str) -> Option<String> {
    // `ADR-0007` from a listed file would name a numbered record that no
    // numbered file backs, and SEMANTICS section 3.3 describes a reference as
    // a bare ADR id. A slug must not be able to impersonate one.
    if id.starts_with("ADR-") {
        return Some(format!(
            "`id: {}` is reserved for a numbered record under `dir`; a listed \
             document needs an id of its own",
            id
        ));
    }
    if id.len() > 64 {
        return Some(format!("`id: {}` is longer than 64 characters", id));
    }
    let mut chars = id.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return Some(format!("`id: {}` must start with a letter", id)),
    }
    if let Some(c) = chars.find(|c| !(c.is_ascii_alphanumeric() || "._-".contains(*c))) {
        return Some(format!(
            "`id: {}` carries `{}`; a slug holds letters, digits, `.`, `_` and `-`",
            id, c
        ));
    }
    None
}

/// The final path segment of `file`. Records may live in more than one
/// directory, so `file` is a repository-relative path and a finding that named
/// only the basename would be ambiguous between two of them.
pub fn basename(file: &str) -> &str {
    file.rsplit('/').next().unwrap_or(file)
}

/// The numeric prefix of a filename, as an ADR id.
fn id_from_filename(file: &str) -> Option<String> {
    let digits: String = basename(file)
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        None
    } else {
        Some(format!("ADR-{}", digits))
    }
}

/// Parse one ADR.
///
/// `file` is a repository-relative path and this crate treats it as an opaque
/// label: it is copied into findings and onto `Adr.file`, and the only code
/// that looks inside it is `id_from_filename`, which reads the final segment.
/// Nothing here touches the filesystem.
pub fn adr(file: &str, src: &str, origin: Origin) -> Result<Adr, Vec<Finding>> {
    let mut out = Vec::new();
    let Some((fm, offset, _body)) = yaml::split_frontmatter(src) else {
        return Err(vec![a(
            "frontmatter-parses",
            "no YAML frontmatter; the file must begin with `---`",
        )
        .in_file(file)]);
    };
    let doc = match yaml::parse(&fm) {
        Ok(d) => d,
        Err(e) => {
            return Err(vec![
                a("frontmatter-parses", e.message).at(file, e.line + offset - 1)
            ])
        }
    };
    if doc.as_map().is_none() {
        return Err(vec![a(
            "frontmatter-parses",
            format!("frontmatter must be a mapping, found {}", doc.kind()),
        )
        .in_file(file)]);
    }
    unknown_fields(&doc, DOC_FIELDS, "frontmatter", file, &mut out);

    let id = want_str(&doc, "id", file, &mut out);
    let id = match id {
        Some(i) => i,
        None => {
            if doc.get("id").is_none() {
                out.push(a("frontmatter-parses", "missing `id`").in_file(file));
            }
            String::new()
        }
    };
    if !id.is_empty() {
        let line = doc.get("id").map_or(1, |n| n.line);
        match origin {
            // Found under `dir` by its numeric prefix, so the number is the
            // identity and the two must agree.
            Origin::Numbered => match id_from_filename(file) {
                Some(expected) if expected != id => out.push(
                    a(
                        "id-matches-filename",
                        format!("`id: {}` but the filename says {}", id, expected),
                    )
                    .at(file, line),
                ),
                None => out.push(
                    a(
                        "id-matches-filename",
                        "the filename has no leading ADR number",
                    )
                    .in_file(file),
                ),
                _ => {}
            },
            // Named in `sources`, so the filename says nothing about the id.
            // Keyed on how the file was found rather than on whether it starts
            // with digits, or an ordinary `2024-payments.md` would be required
            // to call itself ADR-2024.
            Origin::Listed => {
                if let Some(m) = bad_slug(&id) {
                    out.push(a("id-matches-filename", m).at(file, line));
                }
            }
        }
    }

    let status = match want_str(&doc, "status", file, &mut out) {
        Some(s) => match Status::parse(&s) {
            Some(st) => st,
            None => {
                out.push(
                    a(
                        "frontmatter-parses",
                        format!(
                            "`status: {}` is not `draft` or `accepted`. \
                             Supersession is derived, never written",
                            s
                        ),
                    )
                    .at(file, doc.get("status").map_or(1, |n| n.line)),
                );
                Status::Draft
            }
        },
        None => {
            if doc.get("status").is_none() {
                out.push(a("frontmatter-parses", "missing `status`").in_file(file));
            }
            Status::Draft
        }
    };

    let decisions = parse_decisions(&doc, file, &mut out);

    // One entry per slot per document: SEMANTICS section 1.4. This is what
    // makes a bare `ADR-NNNN` reference unambiguous, so it is structural.
    for (i, e) in decisions.iter().enumerate() {
        if let Some(prev) = decisions[..i].iter().find(|p| p.slot() == e.slot()) {
            out.push(
                a(
                    "one-entry-per-slot-per-adr",
                    format!(
                        "a second entry for {}; the first is on line {}",
                        e.slot(),
                        prev.line
                    ),
                )
                .at(file, e.line),
            );
        }
    }

    if out.is_empty() {
        Ok(Adr {
            id: AdrId::new(id),
            status,
            decisions,
            file: file.to_string(),
            pack: None,
        })
    } else {
        Err(out)
    }
}

fn parse_decisions(doc: &Node, file: &str, out: &mut Vec<Finding>) -> Vec<Entry> {
    let Some(d) = doc.get("decisions") else {
        out.push(a("frontmatter-parses", "missing `decisions`").in_file(file));
        return Vec::new();
    };
    if d.is_null() {
        return Vec::new();
    }
    let Some(items) = d.as_seq() else {
        out.push(
            a(
                "frontmatter-parses",
                format!(
                    "`decisions` must be a list, found {}. A mapping could not \
                     hold one key at two scopes",
                    d.kind()
                ),
            )
            .at(file, d.line),
        );
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|it| parse_entry(it, file, out))
        .collect()
}

fn parse_entry(node: &Node, file: &str, out: &mut Vec<Finding>) -> Option<Entry> {
    let before = out.len();
    if node.as_map().is_none() {
        out.push(
            a(
                "frontmatter-parses",
                format!("each decision must be a mapping, found {}", node.kind()),
            )
            .at(file, node.line),
        );
        return None;
    }
    unknown_fields(node, ENTRY_FIELDS, "decision", file, out);

    let key = want_str(node, "key", file, out);
    if key.is_none() && node.get("key").is_none() {
        out.push(a("frontmatter-parses", "a decision is missing `key`").at(file, node.line));
    }
    let scope = want_str(node, "scope", file, out).unwrap_or_else(|| DEFAULT_SCOPE.to_string());
    let choice = want_line(node, "choice", file, out);
    let retire = want_flag(node, "retire", file, out);
    let first = want_flag(node, "first", file, out);
    let replaces = want_str_list(node, "replaces", file, out);
    let overrides = want_str(node, "overrides", file, out);
    let reason = want_line(node, "reason", file, out);

    // Exactly one of choice / retire: SEMANTICS section 1.4.
    let kind = match (choice, retire) {
        (Some(c), false) => EntryKind::Choice(c),
        (None, true) => EntryKind::Retire,
        (Some(_), true) => {
            out.push(
                a(
                    "entry-kind-exclusive",
                    "a decision cannot both choose and retire",
                )
                .at(file, node.line),
            );
            EntryKind::Retire
        }
        (None, false) => {
            out.push(
                a(
                    "entry-kind-exclusive",
                    "a decision needs `choice:` or `retire: true`",
                )
                .at(file, node.line),
            );
            EntryKind::Retire
        }
    };

    // Exactly one of first / replaces: SEMANTICS section 3.4. They agree only
    // when the entry declared both or neither, and both are wrong.
    let has_predecessor = !replaces.is_empty();
    if first == has_predecessor {
        let msg = if first {
            "`first: true` and `replaces:` are mutually exclusive"
        } else {
            "a decision needs `first: true` or a non-empty `replaces:`; \
             silence is not a claim anyone reviewed"
        };
        out.push(a("predecessor-declared", msg).at(file, node.line));
    }

    // `overrides` on a default-scoped entry: SEMANTICS section 3.5.
    if overrides.is_some() && scope == DEFAULT_SCOPE {
        out.push(
            a(
                "overrides-well-placed",
                "`overrides` on a default-scoped entry; there is nothing broader \
                 to diverge from",
            )
            .at(file, node.line),
        );
    }

    if out.len() != before {
        return None;
    }
    Some(Entry {
        key: key?,
        scope,
        kind,
        // Safe past the check above: it returned None unless exactly one of
        // the two was declared, which is what makes the sum type honest here
        // rather than a second place the rule is stated.
        lineage: if first {
            Lineage::First
        } else {
            Lineage::Replaces(replaces.into_iter().map(AdrId::new).collect())
        },
        overrides: overrides.map(AdrId::new),
        reason,
        line: node.line,
    })
}

/// A scope named in a key's own `scopes:` list must be one the registry
/// declares. Letting a key name an undeclared scope would resurrect exactly
/// the hole this field exists to close: a restriction nothing can enforce.
fn key_scope_check(
    item: &str,
    key: &str,
    declared: &[String],
    file: &str,
    line: usize,
    out: &mut Vec<Finding>,
) {
    if item == DEFAULT_SCOPE {
        out.push(
            a(
                "frontmatter-parses",
                format!(
                    "`{}` lists `*` in `scopes`; the default scope is always \
                     admitted and must not be listed",
                    key
                ),
            )
            .at(file, line),
        );
        return;
    }
    if !declared.iter().any(|s| s == item) {
        let hint = suggest(item, declared.iter().map(String::as_str))
            .map(|s| format!("; did you mean `{}`", s))
            .unwrap_or_default();
        out.push(
            a(
                "frontmatter-parses",
                format!(
                    "`{}` lists scope `{}`, which the registry does not declare{}",
                    key, item, hint
                ),
            )
            .at(file, line),
        );
    }
}

/// A registry field holding literal repository-relative paths.
///
/// Shared by `sources` and `packs` because the rule is the same one and for
/// the same reason: a pattern that matches nothing drops something silently,
/// and silence is what this tool exists to remove. A path that escapes the
/// repository is refused rather than resolved, textually, because
/// `canonicalize` follows symlinks and would decide the question elsewhere.
fn path_list(doc: &Node, key: &str, file: &str, out: &mut Vec<Finding>) -> Vec<String> {
    let list = want_str_list(doc, key, file, out);
    let line = doc.get(key).map_or(1, |n| n.line);
    for p in &list {
        if p.starts_with('/') || p.split('/').any(|seg| seg == "..") {
            out.push(
                a(
                    "frontmatter-parses",
                    format!(
                        "`{}` entry `{}` must be a relative path inside the repository",
                        key, p
                    ),
                )
                .at(file, line),
            );
        }
        if p.contains('*') || p.contains('?') {
            out.push(
                a(
                    "frontmatter-parses",
                    format!(
                        "`{}` entry `{}` looks like a pattern; list each file, \
                         so a file that stops matching is an error rather than a \
                         record that quietly disappears",
                        key, p
                    ),
                )
                .at(file, line),
            );
        }
    }
    list
}

/// Parse a `.adr.yaml` registry.
pub fn registry(file: &str, src: &str) -> Result<Registry, Vec<Finding>> {
    let mut out = Vec::new();
    let doc = match yaml::parse(src) {
        Ok(d) => d,
        Err(e) => return Err(vec![a("frontmatter-parses", e.message).at(file, e.line)]),
    };
    if doc.as_map().is_none() {
        return Err(vec![a(
            "frontmatter-parses",
            format!("the registry must be a mapping, found {}", doc.kind()),
        )
        .in_file(file)]);
    }
    unknown_fields(&doc, REGISTRY_FIELDS, "registry", file, &mut out);

    // Literal repository-relative paths, deliberately not patterns. A pattern
    // that matches nothing drops a record silently, and a pattern slightly too
    // wide captures unrelated frontmatter; a listed file that is missing is an
    // error, which is the property worth having.
    let sources = path_list(&doc, "sources", file, &mut out);
    let packs = path_list(&doc, "packs", file, &mut out);

    // `dir` is where this repository keeps its own records. A registry that
    // only vendors has none, and requiring it would make the cheapest way to
    // adopt the fleet corpus — read it, keep nothing — the one shape the tool
    // refuses.
    let dir = want_str(&doc, "dir", file, &mut out).unwrap_or_else(|| {
        if doc.get("dir").is_none() && packs.is_empty() {
            out.push(
                a(
                    "frontmatter-parses",
                    "missing `dir`; a registry needs somewhere to keep records, \
                     or a `packs` list to read somebody else's",
                )
                .in_file(file),
            );
        }
        String::new()
    });

    let scopes = want_str_list(&doc, "scopes", file, &mut out);
    for s in &scopes {
        if s == DEFAULT_SCOPE {
            out.push(
                a(
                    "frontmatter-parses",
                    "`*` is always valid and must not be listed in `scopes`",
                )
                .at(file, doc.get("scopes").map_or(1, |n| n.line)),
            );
        }
    }

    let mut keys = Vec::new();
    match doc.get("keys") {
        None => out.push(a("frontmatter-parses", "missing `keys`").in_file(file)),
        Some(k) if k.is_null() => {}
        Some(k) => match k.as_map() {
            None => out.push(
                a(
                    "frontmatter-parses",
                    format!("`keys` must be a mapping, found {}", k.kind()),
                )
                .at(file, k.line),
            ),
            Some(entries) => {
                for (name, def) in entries {
                    let (description, key_scopes) = if def.is_null() {
                        (None, None)
                    } else {
                        unknown_fields(def, KEYDEF_FIELDS, "key", file, &mut out);
                        let d = want_str(def, "description", file, &mut out);
                        let s = def.get("scopes").map(|node| {
                            let list = want_str_list(def, "scopes", file, &mut out);
                            for item in &list {
                                key_scope_check(item, name, &scopes, file, node.line, &mut out);
                            }
                            list
                        });
                        (d, s)
                    };
                    keys.push(KeyDef {
                        name: name.clone(),
                        description,
                        scopes: key_scopes,
                    });
                }
            }
        },
    }

    if out.is_empty() {
        Ok(Registry {
            dir,
            sources,
            packs,
            scopes,
            keys,
        })
    } else {
        Err(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checks(f: &[Finding]) -> Vec<&str> {
        f.iter().map(|x| x.check).collect()
    }

    const OK: &str = "---\nid: ADR-0001\nstatus: accepted\ndecisions:\n  - key: a.b\n    choice: X\n    first: true\n---\n# t\n";

    #[test]
    fn a_well_formed_adr_parses() {
        let d = adr("0001-t.md", OK, Origin::Numbered).expect("parses");
        assert_eq!(d.id, "ADR-0001");
        assert!(d.status.is_accepted());
        assert_eq!(d.decisions.len(), 1);
        assert_eq!(d.decisions[0].scope, DEFAULT_SCOPE);
        assert_eq!(d.decisions[0].choice(), Some("X"));
    }

    #[test]
    fn a_file_without_frontmatter_is_rejected() {
        let e = adr("0001-t.md", "# just a heading\n", Origin::Numbered).unwrap_err();
        assert_eq!(checks(&e), ["frontmatter-parses"]);
    }

    #[test]
    fn a_misspelled_field_is_rejected_with_a_suggestion() {
        let src = OK.replace(
            "    first: true",
            "    replacess: [ADR-0000]\n    first: true",
        );
        let e = adr("0001-t.md", &src, Origin::Numbered).unwrap_err();
        assert!(e[0].message.contains("unknown decision field `replacess`"));
        assert!(e[0].message.contains("did you mean `replaces`"));
    }

    #[test]
    fn id_must_match_the_filename() {
        let e = adr("0007-t.md", OK, Origin::Numbered).unwrap_err();
        assert_eq!(checks(&e), ["id-matches-filename"]);
    }

    #[test]
    fn superseded_is_not_a_writable_status() {
        let src = OK.replace("status: accepted", "status: superseded");
        let e = adr("0001-t.md", &src, Origin::Numbered).unwrap_err();
        assert!(e[0].message.contains("derived, never written"));
    }

    #[test]
    fn an_entry_cannot_both_choose_and_retire() {
        let src = OK.replace("    choice: X", "    choice: X\n    retire: true");
        let e = adr("0001-t.md", &src, Origin::Numbered).unwrap_err();
        assert!(checks(&e).contains(&"entry-kind-exclusive"));
    }

    #[test]
    fn an_entry_must_declare_a_predecessor() {
        let src = OK.replace("    first: true", "");
        let e = adr("0001-t.md", &src, Origin::Numbered).unwrap_err();
        assert!(checks(&e).contains(&"predecessor-declared"));
    }

    #[test]
    fn first_and_replaces_together_are_rejected() {
        let src = OK.replace(
            "    first: true",
            "    first: true\n    replaces: [ADR-0000]",
        );
        let e = adr("0001-t.md", &src, Origin::Numbered).unwrap_err();
        assert!(checks(&e).contains(&"predecessor-declared"));
    }

    #[test]
    fn overrides_on_a_default_scoped_entry_is_rejected() {
        let src = OK.replace(
            "    first: true",
            "    first: true\n    overrides: ADR-0000",
        );
        let e = adr("0001-t.md", &src, Origin::Numbered).unwrap_err();
        assert!(checks(&e).contains(&"overrides-well-placed"));
    }

    #[test]
    fn two_entries_for_one_slot_in_one_document_are_rejected() {
        let src = OK.replace(
            "    first: true",
            "    first: true\n  - key: a.b\n    choice: Y\n    first: true",
        );
        let e = adr("0001-t.md", &src, Origin::Numbered).unwrap_err();
        assert!(checks(&e).contains(&"one-entry-per-slot-per-adr"));
    }

    #[test]
    fn the_same_key_at_two_scopes_is_fine() {
        let src = OK.replace(
            "    first: true",
            "    first: true\n  - key: a.b\n    scope: cloud\n    choice: Y\n    first: true",
        );
        let d = adr("0001-t.md", &src, Origin::Numbered).expect("parses");
        assert_eq!(d.decisions.len(), 2);
    }

    #[test]
    fn decisions_must_be_a_list_and_the_message_says_why() {
        let src = "---\nid: ADR-0001\nstatus: accepted\ndecisions:\n  a.b:\n    choice: X\n---\n";
        let e = adr("0001-t.md", src, Origin::Numbered).unwrap_err();
        assert!(e[0].message.contains("one key at two scopes"));
    }

    #[test]
    fn a_registry_parses() {
        let r = registry(
            ".adr.yaml",
            "dir: docs/adr\nscopes: [cloud]\nkeys:\n  a.b:\n    description: d\n",
        )
        .expect("parses");
        assert_eq!(r.dir, "docs/adr");
        assert!(r.has_scope("cloud"));
        assert!(r.has_scope(DEFAULT_SCOPE));
        assert!(r.has_key("a.b"));
        assert_eq!(r.keys[0].description.as_deref(), Some("d"));
    }

    #[test]
    fn the_registry_must_not_list_the_default_scope() {
        let e = registry(".adr.yaml", "dir: d\nscopes: ['*']\nkeys:\n  a.b:\n").unwrap_err();
        assert!(e[0].message.contains("always valid"));
    }

    #[test]
    fn a_registry_key_needs_no_description() {
        let r = registry(".adr.yaml", "dir: d\nscopes: []\nkeys:\n  a.b:\n").expect("parses");
        assert_eq!(r.keys.len(), 1);
        assert!(r.keys[0].description.is_none());
    }

    #[test]
    fn a_multi_line_choice_is_rejected_with_its_line() {
        // Reachable by writing exactly what the YAML dialect allows: a block
        // scalar. A markdown table row has no escape for a newline, so this
        // would split one record across several rows in the projection.
        let e = adr(
            "0001-a.md",
            "---\nid: ADR-0001\nstatus: accepted\ndecisions:\n  - key: a.b\n    first: true\n    choice: |\n      one\n      two\n---\n# x\n",
            Origin::Numbered,
        )
        .unwrap_err();
        assert!(e[0].message.contains("single line"), "{:?}", e);
        assert_eq!(e[0].line, Some(6), "{:?}", e);
        assert_eq!(
            e.len(),
            1,
            "a second finding would blame a field the author did write: {:?}",
            e
        );
    }

    #[test]
    fn a_key_declares_which_scopes_it_applies_to() {
        let r = registry(
            ".adr.yaml",
            "dir: d\nscopes: [homelab, cloud]\nkeys:\n  a.b:\n    scopes: [homelab]\n  c.d:\n",
        )
        .expect("parses");
        assert_eq!(
            r.keys[0].scopes.as_deref(),
            Some(&["homelab".to_string()][..])
        );
        // Absent is not the same as empty: it declares nothing.
        assert!(r.keys[1].scopes.is_none());
        assert!(r.admits("a.b", "homelab"));
        assert!(!r.admits("a.b", "cloud"));
        assert!(r.admits("a.b", DEFAULT_SCOPE));
        assert!(r.admits("c.d", "cloud"));
    }

    #[test]
    fn a_key_cannot_name_a_scope_the_registry_does_not_declare() {
        let e = registry(
            ".adr.yaml",
            "dir: d\nscopes: [homelab, cloud]\nkeys:\n  a.b:\n    scopes: [clodu]\n",
        )
        .unwrap_err();
        assert!(e[0].message.contains("does not declare"), "{:?}", e);
        assert!(e[0].message.contains("did you mean `cloud`"), "{:?}", e);
    }

    #[test]
    fn a_key_must_not_list_the_default_scope() {
        let e = registry(
            ".adr.yaml",
            "dir: d\nscopes: [homelab]\nkeys:\n  a.b:\n    scopes: ['*']\n",
        )
        .unwrap_err();
        assert!(e[0].message.contains("always admitted"), "{:?}", e);
    }

    #[test]
    fn an_empty_key_scope_list_is_kept_not_collapsed_to_absent() {
        let r = registry(
            ".adr.yaml",
            "dir: d\nscopes: [homelab]\nkeys:\n  a.b:\n    scopes: []\n",
        )
        .expect("parses");
        assert_eq!(r.keys[0].scopes.as_deref(), Some(&[][..]));
        assert!(!r.admits("a.b", "homelab"));
        assert!(r.admits("a.b", DEFAULT_SCOPE));
    }
}
