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
use aval_core::parse::{self, Origin};

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
        adrs.push(
            parse::adr(name, &src, Origin::Numbered)
                .unwrap_or_else(|f| panic!("{}: {:?}", name, f)),
        );
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
    assert_eq!(
        g.resolve("a.b", DEFAULT_SCOPE)
            .expect("the fixture corpus resolves")
            .token(),
        "active"
    );
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
    assert_eq!(
        g.resolve("a.b", "cloud")
            .expect("the fixture corpus resolves")
            .token(),
        "retired"
    );
    assert_eq!(
        g.resolve("a.b", "cloud")
            .expect("the fixture corpus resolves")
            .exit(),
        6
    );
    // A sibling scope still inherits: retirement is scoped, not contagious.
    assert_eq!(
        g.resolve("a.b", "homelab")
            .expect("the fixture corpus resolves")
            .token(),
        "active"
    );
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

    let v = g
        .resolve("a.b", DEFAULT_SCOPE)
        .expect("the fixture corpus resolves");
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
    assert_eq!(
        g.resolve("a.b", DEFAULT_SCOPE)
            .expect("the fixture corpus resolves")
            .adr(),
        Some("ADR-0004")
    );
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
    assert_eq!(
        g.resolve("a.b", DEFAULT_SCOPE)
            .expect("the fixture corpus resolves")
            .adr(),
        Some("ADR-0001")
    );
}

#[test]
fn an_unoccupied_scope_falls_back_but_a_registered_key_alone_does_not() {
    let g = corpus(&[("0001-a.md", FIRST)]).expect("valid");
    assert_eq!(
        g.resolve("a.b", "cloud")
            .expect("the fixture corpus resolves")
            .adr(),
        Some("ADR-0001")
    );
    assert_eq!(
        g.resolve("c.d", DEFAULT_SCOPE)
            .expect("the fixture corpus resolves")
            .exit(),
        4
    );
    assert_eq!(
        g.resolve("c.d", "cloud")
            .expect("the fixture corpus resolves")
            .exit(),
        4
    );
}

#[test]
fn unknown_names_are_rejected_and_carry_no_adr() {
    let g = corpus(&[("0001-a.md", FIRST)]).expect("valid");
    let v = g
        .resolve("a.c", DEFAULT_SCOPE)
        .expect("the fixture corpus resolves");
    assert_eq!(v.exit(), 7);
    assert_eq!(v.adr(), None);
    let v = g
        .resolve("a.b", "clodu")
        .expect("the fixture corpus resolves");
    assert_eq!(v.exit(), 7);
    assert_eq!(v.adr(), None);
}

// ---------------------------------------------------------------------------
// Per-key scopes. A fleet corpus mixes scope axes — clusters, stack families,
// packaging shapes — and a single flat list makes `scope-declared` useless,
// because nothing then rejects `cni.routing-mode@effect-stack`. A key declaring
// which scopes it applies to restores the check without splitting the registry.

const REG_SCOPED: &str = "\
dir: docs/adr
scopes: [homelab, cloud, effect-stack]
keys:
  a.b:
    scopes: [homelab, cloud]
  c.d:
  fleet.only:
    scopes: []
";

fn scoped_corpus(docs: &[(&str, &str)]) -> Result<Graph, Vec<Finding>> {
    let mut adrs = Vec::new();
    for (name, fm) in docs {
        let src = format!("---\n{}---\n# stub\n", fm);
        adrs.push(
            parse::adr(name, &src, Origin::Numbered)
                .unwrap_or_else(|f| panic!("{}: {:?}", name, f)),
        );
    }
    Graph::build(Corpus {
        registry: parse::registry(".adr.yaml", REG_SCOPED).expect("registry parses"),
        adrs,
    })
}

#[test]
fn a_key_may_be_decided_at_a_scope_it_declares() {
    let g = scoped_corpus(&[(
        "0001-a.md",
        "id: ADR-0001\nstatus: accepted\ndecisions:\n  - key: a.b\n    scope: cloud\n    choice: X\n    first: true\n",
    )])
    .expect("valid");
    assert_eq!(
        g.resolve("a.b", "cloud")
            .expect("the fixture corpus resolves")
            .token(),
        "active"
    );
}

#[test]
fn scope_applies_rejects_a_declared_scope_the_key_does_not_cover() {
    // `effect-stack` is a perfectly good scope. It is just not an axis
    // `a.b` is decided along, so this is a Layer A error and not a verdict.
    match scoped_corpus(&[(
        "0001-a.md",
        "id: ADR-0001\nstatus: accepted\ndecisions:\n  - key: a.b\n    scope: effect-stack\n    choice: X\n    first: true\n",
    )]) {
        Ok(_) => panic!("expected `scope-applies` to reject this corpus"),
        Err(f) => {
            assert!(checks(&f).contains(&"scope-applies"), "got {:?}", checks(&f));
            // and not the wrong diagnosis
            assert!(!checks(&f).contains(&"scope-declared"), "got {:?}", checks(&f));
        }
    }
}

#[test]
fn a_restricted_key_still_decides_at_the_default_scope() {
    // The default scope is the inheritance root. Restricting a key must not
    // sever its own fallback.
    let g = scoped_corpus(&[(
        "0001-a.md",
        "id: ADR-0001\nstatus: accepted\ndecisions:\n  - key: a.b\n    choice: X\n    first: true\n",
    )])
    .expect("valid");
    assert_eq!(
        g.resolve("a.b", "homelab")
            .expect("the fixture corpus resolves")
            .adr(),
        Some("ADR-0001")
    );
    assert!(matches!(
        g.resolve("a.b", "homelab")
            .expect("the fixture corpus resolves"),
        Verdict::Active {
            inherited: true,
            ..
        }
    ));
}

#[test]
fn an_unrestricted_key_accepts_every_declared_scope() {
    // The compatibility guarantee: a registry written before `scopes:` existed
    // keeps its meaning exactly.
    let g = scoped_corpus(&[(
        "0001-a.md",
        "id: ADR-0001\nstatus: accepted\ndecisions:\n  - key: c.d\n    scope: effect-stack\n    choice: X\n    first: true\n",
    )])
    .expect("valid");
    assert_eq!(
        g.resolve("c.d", "effect-stack")
            .expect("the fixture corpus resolves")
            .token(),
        "active"
    );
}

#[test]
fn resolving_a_key_outside_its_scopes_is_unknown_and_never_inherits() {
    // The trap this closes: without the check, the query falls back to the
    // default scope and answers a question about a different axis with an
    // `active` verdict, which reads as agreement.
    let g = scoped_corpus(&[(
        "0001-a.md",
        "id: ADR-0001\nstatus: accepted\ndecisions:\n  - key: a.b\n    choice: X\n    first: true\n",
    )])
    .expect("valid");
    let v = g
        .resolve("a.b", "effect-stack")
        .expect("the fixture corpus resolves");
    assert_eq!(v.exit(), 7);
    assert_eq!(v.adr(), None);
    assert!(
        v.note(aval_core::model::Slot {
            key: "a.b",
            scope: "effect-stack"
        })
        .contains("not decided per"),
        "{:?}",
        v
    );
}

#[test]
fn an_empty_scopes_list_means_the_default_scope_only() {
    let g = scoped_corpus(&[(
        "0001-a.md",
        "id: ADR-0001\nstatus: accepted\ndecisions:\n  - key: fleet.only\n    choice: X\n    first: true\n",
    )])
    .expect("valid");
    assert_eq!(
        g.resolve("fleet.only", "*")
            .expect("the fixture corpus resolves")
            .token(),
        "active"
    );
    assert_eq!(
        g.resolve("fleet.only", "homelab")
            .expect("the fixture corpus resolves")
            .exit(),
        7
    );
}
