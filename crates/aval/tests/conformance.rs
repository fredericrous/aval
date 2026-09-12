//! The conformance battery.
//!
//! One fixture file drives every case, the fixture's own `name` is the test
//! name so a failure names the semantic case rather than an index, an unknown
//! `query.command` throws rather than skipping, and a `min_cases` guard keeps
//! an empty or mis-parsed fixture from passing green. That last one is the
//! cheapest insurance in the pattern and the reason it is written down.

use aval::load::{LoadError, Loaded};
use aval_core::graph::{Graph, Verdict};
use aval_core::json::{self, Json};
use aval_core::model::DEFAULT_SCOPE;
use std::path::{Path, PathBuf};

fn conformance_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance")
        .canonicalize()
        .expect("conformance directory")
}

fn battery() -> Json {
    let p = conformance_dir().join("resolve.json");
    let src = std::fs::read_to_string(&p).expect("read resolve.json");
    json::parse(&src).expect("resolve.json parses")
}

/// Load a corpus with the binary's own loader. This used to be a second
/// implementation of discovery, which meant the battery asserted semantics
/// that merely resembled the binary's.
fn loaded(name: &str) -> Loaded {
    let base = conformance_dir().join("corpora").join(name);
    match aval::load::load(&base) {
        Ok(l) => l,
        Err(LoadError::NoRegistry(p)) => panic!("{}: no registry at {}", name, p.display()),
        Err(LoadError::Unreadable(m)) => panic!("{}: {}", name, m),
        Err(LoadError::Invalid(f)) => panic!("{}: layer A: {:?}", name, f),
    }
}

fn load(name: &str) -> Graph {
    loaded(name).graph
}

fn want_str<'a>(exp: &'a Json, k: &str) -> Option<&'a str> {
    exp.get(k).and_then(|v| v.as_str())
}

fn run_resolve(case_name: &str, g: &Graph, q: &Json, exp: &Json) {
    let key = want_str(q, "key").expect("query.key");
    let scope = want_str(q, "scope").unwrap_or(DEFAULT_SCOPE);
    let v = g.resolve(key, scope).expect("the fixture corpus resolves");

    if let Some(want) = want_str(exp, "verdict") {
        assert_eq!(v.token(), want, "{}: verdict", case_name);
    }
    if let Some(want) = exp.get("exit").and_then(|x| x.as_i64()) {
        assert_eq!(v.exit() as i64, want, "{}: exit code", case_name);
    }
    // `"adr": null` is an assertion in its own right: exit 7 must carry no ADR,
    // or a caller could act on a suggestion.
    if let Some(node) = exp.get("adr") {
        if node.is_null() {
            assert_eq!(v.adr(), None, "{}: expected no adr", case_name);
        } else {
            assert_eq!(v.adr(), node.as_str(), "{}: adr", case_name);
        }
    }
    if let Some(want) = want_str(exp, "matched_scope") {
        let got = match &v {
            Verdict::Active { matched_scope, .. } | Verdict::Retired { matched_scope, .. } => {
                Some(matched_scope.as_str())
            }
            _ => None,
        };
        assert_eq!(got, Some(want), "{}: matched_scope", case_name);
    }
    if let Some(want) = want_str(exp, "unknown") {
        match &v {
            Verdict::Unknown { what, .. } => {
                assert_eq!(what.as_str(), want, "{}: unknown kind", case_name)
            }
            other => panic!("{}: expected unknown, got {:?}", case_name, other),
        }
    }
    if let Some(want) = want_str(exp, "suggestion") {
        match &v {
            Verdict::Unknown { suggestion, .. } => assert_eq!(
                suggestion.as_deref(),
                Some(want),
                "{}: suggestion",
                case_name
            ),
            other => panic!("{}: expected unknown, got {:?}", case_name, other),
        }
    }
    if let Some(want) = exp.get("heads").and_then(|x| x.as_arr()) {
        match &v {
            Verdict::Contradiction { heads, .. } => {
                let want: Vec<&str> = want.iter().filter_map(|x| x.as_str()).collect();
                assert_eq!(heads.as_slice(), want.as_slice(), "{}: heads", case_name);
            }
            other => panic!("{}: expected contradiction, got {:?}", case_name, other),
        }
    }
}

fn run_show(case_name: &str, g: &Graph, q: &Json, exp: &Json) {
    let id = want_str(q, "adr").expect("query.adr");
    let adr = g
        .corpus()
        .adr(id)
        .unwrap_or_else(|| panic!("{}: no such adr {}", case_name, id));
    if let Some(want) = want_str(exp, "derived_status") {
        assert_eq!(
            g.derived_status(adr).as_str(),
            want,
            "{}: derived status",
            case_name
        );
    }
    let status = g.entry_status(adr);
    if let Some(want) = exp.get("heads").and_then(|x| x.as_arr()) {
        let mut got: Vec<&str> = status
            .iter()
            .filter(|(_, h)| *h)
            .map(|(e, _)| e.key.as_str())
            .collect();
        got.sort();
        let mut want: Vec<&str> = want.iter().filter_map(|x| x.as_str()).collect();
        want.sort();
        assert_eq!(got, want, "{}: head keys", case_name);
    }
    if let Some(want) = exp.get("superseded").and_then(|x| x.as_arr()) {
        let mut got: Vec<&str> = status
            .iter()
            .filter(|(_, h)| !*h)
            .map(|(e, _)| e.key.as_str())
            .collect();
        got.sort();
        let mut want: Vec<&str> = want.iter().filter_map(|x| x.as_str()).collect();
        want.sort();
        assert_eq!(got, want, "{}: superseded keys", case_name);
    }
}

#[test]
fn the_battery_is_non_trivial() {
    let b = battery();
    let n = b
        .get("cases")
        .and_then(|c| c.as_arr())
        .map_or(0, |c| c.len());
    let min = b.get("min_cases").and_then(|m| m.as_i64()).unwrap_or(0) as usize;
    assert!(min > 0, "resolve.json must declare min_cases");
    assert!(
        n >= min,
        "battery has {} cases, fewer than the declared minimum {}",
        n,
        min
    );
}

#[test]
fn every_case_holds() {
    let b = battery();
    let cases = b.get("cases").and_then(|c| c.as_arr()).expect("cases");
    let mut seen: Vec<&str> = Vec::new();
    for c in cases {
        let name = want_str(c, "name").expect("case.name");
        assert!(!seen.contains(&name), "duplicate case name `{}`", name);
        seen.push(name);

        let corpus = want_str(c, "corpus").expect("case.corpus");
        let g = load(corpus);
        let q = c.get("query").expect("case.query");
        let exp = c.get("expected").expect("case.expected");
        match want_str(q, "command") {
            Some("resolve") => run_resolve(name, &g, q, exp),
            Some("show") => run_show(name, &g, q, exp),
            // Throw rather than skip: a new fixture kind must fail the old
            // implementation loudly instead of quietly passing.
            other => panic!("{}: unknown query command {:?}", name, other),
        }
    }
}

/// The projection is a pure function of the graph, so it must be byte-stable.
#[test]
fn the_projection_is_deterministic() {
    let g = load("homelab-sample");
    let a = aval_core::project::render(&g);
    let b = aval_core::project::render(&g);
    assert_eq!(a, b);
    assert!(
        !a.contains("ADR-0023"),
        "a draft must not reach the projection"
    );
    assert!(
        !a.contains("Garage, served from the NAS cluster"),
        "a superseded entry must not reach the projection"
    );
}

/// A slot with competing heads must not have one of them silently printed.
#[test]
fn the_projection_omits_a_contradicted_slot() {
    let g = load("contradiction");
    let out = aval_core::project::render(&g);
    assert!(!out.contains("ADR-0002"), "{}", out);
    assert!(!out.contains("ADR-0003"), "{}", out);
}

/// The five checked-in `HEADS.md` files were decorative until this existed:
/// nothing compared them to anything, so a change to the projection could not
/// be told from a regression. SEMANTICS section 15 says a change to the
/// conformance fixtures is the signal that behaviour moved — this is what
/// makes that signal fire.
#[test]
fn every_corpus_heads_file_matches_the_projection() {
    let corpora = [
        "homelab-sample",
        "diamond",
        "contradiction",
        "retire-scoped",
        "scoped-keys",
    ];
    for name in corpora {
        let l = loaded(name);
        let path = l.adr_dir.join(aval::load::HEADS);
        let have = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {}: {}", name, path.display(), e));
        let want = aval_core::project::render(&l.graph);
        assert_eq!(
            have,
            want,
            "{}: {} is not what `aval heads --write` produces",
            name,
            path.display()
        );
    }
}

/// The defect this release exists to fix, asserted against a fixture that is
/// the literal output of `npx prettier`, not an imitation of it.
///
/// Eighteen repositories in the fleet this tool serves run prettier over their
/// markdown. A generated file that cannot survive the repository's own
/// formatter is a defect in the generator, and the workaround — excluding it
/// from formatting — does not scale to eighteen.
#[test]
fn a_prettier_formatted_projection_is_current() {
    let l = loaded("homelab-sample");
    let canonical = aval_core::project::render(&l.graph);
    let formatted = include_str!("fixtures/heads-prettier.md");

    assert_ne!(
        formatted, canonical,
        "the fixture must actually be reformatted, or this test asserts nothing"
    );
    assert_eq!(
        aval_core::project::canonicalise(formatted),
        aval_core::project::canonicalise(&canonical),
        "prettier's output must read as current"
    );
}

/// Paired with the above: tolerating the formatter must not tolerate a claim.
#[test]
fn an_edit_to_a_formatted_projection_is_still_stale() {
    let l = loaded("homelab-sample");
    let canonical = aval_core::project::canonicalise(&aval_core::project::render(&l.graph));
    let formatted = include_str!("fixtures/heads-prettier.md");

    for edit in [
        // a head that moved
        formatted.replace("ADR-0006", "ADR-0099"),
        // a row nobody decided
        formatted.replace(
            "## Undecided",
            "| made.up | — | ADR-0001 | X |\n\n## Undecided",
        ),
        // prose under the banner: the case a row-parser would have ignored
        format!("{}\nNote: under review.\n", formatted),
        // the banner itself rewritten
        formatted.replace("Do not edit.", "Maintained by hand."),
    ] {
        assert_ne!(
            aval_core::project::canonicalise(&edit),
            canonical,
            "a content change must not read as current"
        );
    }
}
