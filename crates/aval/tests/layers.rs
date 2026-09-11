//! Every Layer A invariant failing on its own, and the separation between the
//! three layers.
//!
//! The separation is the part worth testing hardest. An earlier draft of this
//! design made `single-head` a validity invariant *and* made competing heads an
//! ordinary verdict, which meant a resolver that validated before answering
//! could only ever return exit 3 and the contradiction verdict was unreachable.
//! `contradiction_is_reachable` is the regression test for that.

use aval_core::graph::{Graph, Verdict};
use aval_core::model::{Corpus, Finding, Registry, DEFAULT_SCOPE};
use aval_core::parse;

const REG: &str = "\
dir: docs/adr
scopes: [homelab, cloud]
keys:
  a.b:
  c.d:
";

fn registry() -> Registry {
    parse::registry(".adr.yaml", REG).expect("registry parses")
}

/// Build a corpus from `(filename, frontmatter body)` pairs.
fn corpus(docs: &[(&str, &str)]) -> Result<Graph, Vec<Finding>> {
    let mut adrs = Vec::new();
    for (name, fm) in docs {
        let src = format!("---\n{}---\n# stub\n", fm);
        adrs.push(parse::adr(name, &src).unwrap_or_else(|f| panic!("{}: {:?}", name, f)));
    }
    Graph::build(Corpus {
        registry: registry(),
        adrs,
    })
}

fn checks(f: &[Finding]) -> Vec<&str> {
    let mut v: Vec<&str> = f.iter().map(|x| x.check).collect();
    v.sort();
    v.dedup();
    v
}

fn rejects(docs: &[(&str, &str)], check: &str) {
    match corpus(docs) {
        Ok(_) => panic!("expected `{}` to reject this corpus", check),
        Err(f) => assert!(
            checks(&f).contains(&check),
            "expected `{}`, got {:?}",
            check,
            checks(&f)
        ),
    }
}

const FIRST: &str =
    "id: ADR-0001\nstatus: accepted\ndecisions:\n  - key: a.b\n    choice: One\n    first: true\n";

#[test]
fn a_minimal_corpus_is_valid() {
    let g = corpus(&[("0001-a.md", FIRST)]).expect("valid");
    assert_eq!(g.resolve("a.b", DEFAULT_SCOPE).token(), "active");
}

#[test]
fn id_unique() {
    rejects(&[("0001-a.md", FIRST), ("0001-b.md", FIRST)], "id-unique");
}

#[test]
fn key_registered() {
    rejects(
        &[(
            "0001-a.md",
            "id: ADR-0001\nstatus: accepted\ndecisions:\n  - key: nope.here\n    choice: X\n    first: true\n",
        )],
        "key-registered",
    );
}

#[test]
fn scope_declared() {
    rejects(
        &[(
            "0001-a.md",
            "id: ADR-0001\nstatus: accepted\ndecisions:\n  - key: a.b\n    scope: nowhere\n    choice: X\n    first: true\n",
        )],
        "scope-declared",
    );
}

#[test]
fn edge_resolves_when_the_target_does_not_exist() {
    rejects(
        &[(
            "0001-a.md",
            "id: ADR-0001\nstatus: accepted\ndecisions:\n  - key: a.b\n    choice: X\n    replaces: [ADR-0099]\n",
        )],
        "edge-resolves",
    );
}

#[test]
fn edge_resolves_when_the_target_has_no_entry_in_this_slot() {
    // ADR-0001 decides a.b at the default scope; ADR-0002 tries to replace it
    // from the cloud scope, which is a different slot and a different decision.
    rejects(
        &[
            ("0001-a.md", FIRST),
            (
                "0002-b.md",
                "id: ADR-0002\nstatus: accepted\ndecisions:\n  - key: a.b\n    scope: cloud\n    choice: Y\n    replaces: [ADR-0001]\n",
            ),
        ],
        "edge-resolves",
    );
}

#[test]
fn no_accepted_replaces_draft() {
    rejects(
        &[
            (
                "0001-a.md",
                "id: ADR-0001\nstatus: draft\ndecisions:\n  - key: a.b\n    choice: One\n    first: true\n",
            ),
            (
                "0002-b.md",
                "id: ADR-0002\nstatus: accepted\ndecisions:\n  - key: a.b\n    choice: Two\n    replaces: [ADR-0001]\n",
            ),
        ],
        "no-accepted-replaces-draft",
    );
}

#[test]
fn no_cycle() {
    rejects(
        &[
            (
                "0001-a.md",
                "id: ADR-0001\nstatus: accepted\ndecisions:\n  - key: a.b\n    choice: One\n    replaces: [ADR-0002]\n",
            ),
            (
                "0002-b.md",
                "id: ADR-0002\nstatus: accepted\ndecisions:\n  - key: a.b\n    choice: Two\n    replaces: [ADR-0001]\n",
            ),
        ],
        "no-cycle",
    );
}

#[test]
fn a_first_retirement_must_name_what_it_opts_out_of() {
    rejects(
        &[
            ("0001-a.md", FIRST),
            (
                "0002-b.md",
                "id: ADR-0002\nstatus: accepted\ndecisions:\n  - key: a.b\n    scope: cloud\n    retire: true\n    first: true\n",
            ),
        ],
        "retire-names-predecessor",
    );
}

#[test]
fn a_first_retirement_needs_something_inherited_to_retire() {
    // Nothing decides `c.d` at all, so there is no inherited default for the
    // cloud scope to opt out of. The answer is already undecided.
    rejects(
        &[
            ("0001-a.md", FIRST),
            (
                "0002-b.md",
                "id: ADR-0002\nstatus: accepted\ndecisions:\n  - key: c.d\n    scope: cloud\n    retire: true\n    first: true\n    overrides: ADR-0001\n",
            ),
        ],
        "retire-names-predecessor",
    );
}

#[test]
fn a_scoped_retirement_of_an_inherited_default_is_valid_and_blocks_fallback() {
    let g = corpus(&[
        ("0001-a.md", FIRST),
        (
            "0002-b.md",
            "id: ADR-0002\nstatus: accepted\ndecisions:\n  - key: a.b\n    scope: cloud\n    retire: true\n    first: true\n    overrides: ADR-0001\n",
        ),
    ])
    .expect("valid");
    assert_eq!(g.resolve("a.b", "cloud").token(), "retired");
    assert_eq!(g.resolve("a.b", "cloud").exit(), 6);
    // A sibling scope still inherits: retirement is scoped, not contagious.
    assert_eq!(g.resolve("a.b", "homelab").token(), "active");
}

// ------------------------------------------------------------------ layers

#[test]
fn contradiction_is_reachable() {
    let g = corpus(&[
        ("0001-a.md", FIRST),
        (
            "0002-b.md",
            "id: ADR-0002\nstatus: accepted\ndecisions:\n  - key: a.b\n    choice: Two\n    replaces: [ADR-0001]\n",
        ),
        (
            "0003-c.md",
            "id: ADR-0003\nstatus: accepted\ndecisions:\n  - key: a.b\n    choice: Three\n    replaces: [ADR-0001]\n",
        ),
    ])
    .expect("competing heads are NOT a Layer A failure");

    let v = g.resolve("a.b", DEFAULT_SCOPE);
    assert_eq!(v.exit(), 5, "contradiction must not be masked by exit 3");
    match v {
        Verdict::Contradiction { heads, .. } => assert_eq!(heads, ["ADR-0002", "ADR-0003"]),
        other => panic!("expected contradiction, got {:?}", other),
    }
    assert_eq!(checks(&g.single_head_findings()), ["single-head"]);
}

#[test]
fn a_reconciliation_closes_the_diamond() {
    let g = corpus(&[
        ("0001-a.md", FIRST),
        (
            "0002-b.md",
            "id: ADR-0002\nstatus: accepted\ndecisions:\n  - key: a.b\n    choice: Two\n    replaces: [ADR-0001]\n",
        ),
        (
            "0003-c.md",
            "id: ADR-0003\nstatus: accepted\ndecisions:\n  - key: a.b\n    choice: Three\n    replaces: [ADR-0001]\n",
        ),
        (
            "0004-d.md",
            "id: ADR-0004\nstatus: accepted\ndecisions:\n  - key: a.b\n    choice: Four\n    replaces: [ADR-0002, ADR-0003]\n",
        ),
    ])
    .expect("valid");
    assert_eq!(g.resolve("a.b", DEFAULT_SCOPE).adr(), Some("ADR-0004"));
    assert!(g.single_head_findings().is_empty());
}

#[test]
fn a_draft_neither_wins_nor_demotes() {
    let g = corpus(&[
        ("0001-a.md", FIRST),
        (
            "0002-b.md",
            "id: ADR-0002\nstatus: draft\ndecisions:\n  - key: a.b\n    choice: Two\n    replaces: [ADR-0001]\n",
        ),
    ])
    .expect("valid");
    assert_eq!(g.resolve("a.b", DEFAULT_SCOPE).adr(), Some("ADR-0001"));
}

#[test]
fn an_unoccupied_scope_falls_back_but_a_registered_key_alone_does_not() {
    let g = corpus(&[("0001-a.md", FIRST)]).expect("valid");
    assert_eq!(g.resolve("a.b", "cloud").adr(), Some("ADR-0001"));
    assert_eq!(g.resolve("c.d", DEFAULT_SCOPE).exit(), 4);
    assert_eq!(g.resolve("c.d", "cloud").exit(), 4);
}

#[test]
fn unknown_names_are_rejected_and_carry_no_adr() {
    let g = corpus(&[("0001-a.md", FIRST)]).expect("valid");
    let v = g.resolve("a.c", DEFAULT_SCOPE);
    assert_eq!(v.exit(), 7);
    assert_eq!(v.adr(), None);
    let v = g.resolve("a.b", "clodu");
    assert_eq!(v.exit(), 7);
    assert_eq!(v.adr(), None);
}
