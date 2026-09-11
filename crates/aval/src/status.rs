//! `status-single-source`: a document that claims approval twice.
//!
//! Frontmatter owns approval. A document that also carries a prose status line
//! saying `accepted` or `proposed` has two claims about the same thing, and
//! nothing keeps them in step — which is how a specification ends up reading
//! as unstarted while its decision has shipped.
//!
//! The grammar is deliberately narrow. An earlier draft scanned prose for
//! approval words and would have fired on "we **rejected** Kong because of the
//! CRD churn", on "the `/v1` API is **deprecated** in favour of v2", and on an
//! options table row reading `| Kong | rejected |`. None of those is a claim
//! about the document. So this matches a status *line*, in the header block
//! above the first `##`, in the shapes real documents actually use, and only
//! when its value opens with one of four approval words.
//!
//! Rollout prose is silent by construction: `**Status:** phases 1-2
//! IMPLEMENTED; phase 3 open` opens with `phases`, which is not an approval
//! word, and rollout is not something this tool models.

use crate::load::Loaded;
use aval_core::model::{Finding, Layer};

/// The vocabulary `migrate` already recognises. Four words, not a longer list
/// invented here: a wider vocabulary is a wider false-positive surface, and
/// these are the ones the legacy templates in this fleet actually use.
const APPROVAL_WORDS: &[&str] = &["proposed", "accepted", "superseded", "deprecated"];

pub fn check(l: &Loaded) -> Vec<Finding> {
    let mut out = Vec::new();
    for (rel, src) in &l.files {
        let Some((line_no, value)) = prose_status(src) else {
            continue;
        };
        let head = value.split_whitespace().next().unwrap_or("").to_lowercase();
        let head = head.trim_matches(|c: char| !c.is_ascii_alphabetic());
        if !APPROVAL_WORDS.contains(&head) {
            continue;
        }
        out.push(
            Finding::new(
                Layer::C,
                "status-single-source",
                format!(
                    "a prose status line says `{}` while the frontmatter owns approval; \
                     one of the two will go stale",
                    value.trim()
                ),
            )
            .at(rel.clone(), line_no),
        );
    }
    out
}

/// The value of a status line in the header block, with its line number.
///
/// The header block ends at the first `##`, which is what keeps a fenced
/// example or a quoted historical status deeper in the document out of range.
/// A fence opened before that heading is skipped too.
fn prose_status(src: &str) -> Option<(usize, String)> {
    let body = match aval_core::yaml::split_frontmatter(src) {
        // Only a document that already declares approval can claim it twice.
        Some((_, offset, body)) => (offset, body),
        None => return None,
    };
    let (offset, body) = body;
    let mut fenced = false;
    for (i, line) in body.lines().enumerate() {
        let t = line.trim();
        if t.starts_with("```") || t.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if t.starts_with("## ") {
            return None;
        }
        if let Some(v) = status_value(t) {
            return Some((offset + i, v));
        }
    }
    None
}

/// `- **Status**: v`, `- Status: v`, `**Status:** v`, `> STATUS: v` and the
/// bold/blockquote combinations of those. Every form the fleet's documents
/// actually use, and nothing looser.
fn status_value(line: &str) -> Option<String> {
    let t = line
        .trim_start_matches("> ")
        .trim_start_matches("- ")
        .trim_start_matches("> ")
        .trim();
    let t = t.strip_prefix("**").unwrap_or(t);
    let rest = t.strip_prefix_ci("status")?;
    let rest = rest.strip_prefix("**").unwrap_or(rest);
    let rest = rest.strip_prefix(':')?;
    let rest = rest
        .trim_start()
        .strip_prefix("**")
        .unwrap_or(rest.trim_start());
    Some(rest.trim().to_string())
}

/// Case-insensitive `strip_prefix`, for the one word this module matches.
trait StripCi {
    fn strip_prefix_ci(&self, p: &str) -> Option<&str>;
}

impl StripCi for str {
    fn strip_prefix_ci(&self, p: &str) -> Option<&str> {
        if self.len() >= p.len() && self[..p.len()].eq_ignore_ascii_case(p) {
            Some(&self[p.len()..])
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(line: &str) -> Option<String> {
        status_value(line)
    }

    #[test]
    fn the_shapes_real_documents_use_are_matched() {
        assert_eq!(v("- **Status**: Proposed").as_deref(), Some("Proposed"));
        assert_eq!(v("- **Status:** Proposed").as_deref(), Some("Proposed"));
        assert_eq!(v("- Status: accepted").as_deref(), Some("accepted"));
        assert_eq!(v("**Status:** proposed").as_deref(), Some("proposed"));
        assert_eq!(
            v("> STATUS: Increment 1 shipped").as_deref(),
            Some("Increment 1 shipped")
        );
        assert_eq!(v("> **Status:** draft.").as_deref(), Some("draft."));
    }

    #[test]
    fn ordinary_prose_is_not_a_status_line() {
        // Every one of these appears in real architecture writing, and an
        // earlier draft that scanned for approval words fired on all of them.
        assert_eq!(v("We **rejected** Kong because of the CRD churn"), None);
        assert_eq!(v("The `/v1` API is **deprecated** in favour of v2"), None);
        assert_eq!(v("| Kong | rejected | heavy |"), None);
        assert_eq!(v("This supersedes the manual runbook"), None);
        assert_eq!(v("- Deciders: platform"), None);
        assert_eq!(v("Status reporting is out of scope"), None);
    }

    #[test]
    fn a_rollout_status_line_carries_no_approval_word() {
        // Matched as a status line, and then ignored, because rollout is not
        // approval and this tool does not model rollout at all.
        let value = v("**Status:** phases 1-2 IMPLEMENTED; phase 3 open").unwrap();
        let head = value.split_whitespace().next().unwrap().to_lowercase();
        assert!(!APPROVAL_WORDS.contains(&head.as_str()), "{}", value);
    }

    #[test]
    fn an_approval_word_is_recognised_whatever_its_case_or_punctuation() {
        for line in [
            "**Status:** accepted.",
            "- **Status**: ACCEPTED",
            "- Status: Accepted (2026-07)",
        ] {
            let value = v(line).unwrap();
            let head = value.split_whitespace().next().unwrap().to_lowercase();
            let head = head.trim_matches(|c: char| !c.is_ascii_alphabetic());
            assert!(APPROVAL_WORDS.contains(&head), "{}", line);
        }
    }

    #[test]
    fn only_the_header_block_is_in_range() {
        let doc = "---\nid: x\n---\n# T\n\n## Context\n\n- **Status**: Accepted\n";
        assert!(prose_status(doc).is_none(), "past the first heading");

        let fenced = "---\nid: x\n---\n# T\n\n```\n- **Status**: Accepted\n```\n";
        assert!(prose_status(fenced).is_none(), "inside a fence");

        let header = "---\nid: x\n---\n# T\n\n- **Status**: Accepted\n\n## Context\n";
        assert!(prose_status(header).is_some());
    }

    #[test]
    fn a_document_without_frontmatter_is_not_claiming_twice() {
        assert!(prose_status("# T\n\n- **Status**: Accepted\n").is_none());
    }
}
