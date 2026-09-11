//! The conformance battery.
//!
//! One fixture file drives every case, the fixture's own `name` is the test
//! name so a failure names the semantic case rather than an index, an unknown
//! `query.command` throws rather than skipping, and a `min_cases` guard keeps
//! an empty or mis-parsed fixture from passing green. That last one is the
//! cheapest insurance in the pattern and the reason it is written down.

use aval_core::graph::{Graph, Verdict};
use aval_core::json::{self, Json};
use aval_core::model::{Corpus, DEFAULT_SCOPE};
use aval_core::parse;
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

/// Load a corpus the way the binary does, but without the binary.
fn load(name: &str) -> Graph {
    let base = conformance_dir().join("corpora").join(name);
    let reg_src = std::fs::read_to_string(base.join(".adr.yaml"))
        .unwrap_or_else(|e| panic!("{}: {}", name, e));
    let registry = parse::registry(".adr.yaml", &reg_src)
        .unwrap_or_else(|f| panic!("{}: registry: {:?}", name, f));
    let dir = base.join(&registry.dir);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {}", dir.display(), e))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "md").unwrap_or(false))
        .collect();
    paths.sort();
    let mut adrs = Vec::new();
    for p in paths {
        let file = p.file_name().unwrap().to_string_lossy().to_string();
        if !file
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            continue;
        }
        let src = std::fs::read_to_string(&p).expect("read adr");
        match parse::adr(&file, &src) {
            Ok(a) => adrs.push(a),
            Err(f) => panic!("{}/{}: {:?}", name, file, f),
        }
    }
    Graph::build(Corpus { registry, adrs }).unwrap_or_else(|f| panic!("{}: layer A: {:?}", name, f))
}

fn want_str<'a>(exp: &'a Json, k: &str) -> Option<&'a str> {
    exp.get(k).and_then(|v| v.as_str())
}

fn run_resolve(case_name: &str, g: &Graph, q: &Json, exp: &Json) {
    let key = want_str(q, "key").expect("query.key");
    let scope = want_str(q, "scope").unwrap_or(DEFAULT_SCOPE);
    let v = g.resolve(key, scope);

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
