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
        Err(LoadError::NoRegistry(p)) | Err(LoadError::NoCorpora(p)) => {
            panic!("{}: no registry at {}", name, p.display())
        }
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
            assert_eq!(
                v.adr().map(|a| a.as_str()),
                node.as_str(),
                "{}: adr",
                case_name
            );
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
        "rules-sample",
        "relevance-sample",
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

// --- the relevance battery -------------------------------------------------
//
// A second fixture file, driven the same way and asserting two things the
// resolve battery cannot reach: that a ranking is reproducible, and that the
// verdict beside a ranked key is the resolver's own (SEMANTICS section 5.1).

fn relevance_battery() -> Json {
    let p = conformance_dir().join("relevant.json");
    let src = std::fs::read_to_string(&p).expect("read relevant.json");
    json::parse(&src).expect("relevant.json parses")
}

/// Every field a case may assert. An unknown one throws, for the reason an
/// unknown `query.command` does: a fixture nobody reads is a fixture that
/// passes green while asserting nothing.
const RELEVANCE_FIELDS: &[&str] = &[
    "order",
    "states",
    "exits",
    "adrs",
    "unresolved",
    "mentions",
    "co_changed",
    "elsewhere",
    "rules",
];

fn query_of(q: &Json) -> aval::relevant::Query {
    aval::relevant::Query {
        paths: q
            .get("paths")
            .and_then(|p| p.as_arr())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        text: want_str(q, "text").map(str::to_string),
        changed: false,
        top: q
            .get("top")
            .and_then(|t| t.as_i64())
            .unwrap_or(aval::relevant::DEFAULT_TOP as i64) as usize,
        scope: want_str(q, "scope").unwrap_or(DEFAULT_SCOPE).to_string(),
    }
}

fn keys_of(payload: &Json) -> &[Json] {
    payload
        .get("keys")
        .and_then(|k| k.as_arr())
        .expect("payload.keys")
}

fn row<'a>(payload: &'a Json, key: &str) -> &'a Json {
    keys_of(payload)
        .iter()
        .find(|r| r.get("key").and_then(|k| k.as_str()) == Some(key))
        .unwrap_or_else(|| panic!("`{}` is not in the ranking: {}", key, payload))
}

fn pairs(exp: &Json, field: &str) -> Vec<(String, Json)> {
    match exp.get(field) {
        Some(Json::Obj(m)) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        None => Vec::new(),
        other => panic!("`{}` must be an object, found {:?}", field, other),
    }
}

fn run_relevance(case: &str, l: &Loaded, q: &Json, exp: &Json) {
    if let Json::Obj(m) = exp {
        for k in m.keys() {
            assert!(
                RELEVANCE_FIELDS.contains(&k.as_str()),
                "{}: unknown expectation `{}`",
                case,
                k
            );
        }
    }
    let query = query_of(q);
    let reply = aval::relevant::relevant_in(l, &query);
    assert_eq!(reply.exit, 0, "{}: a ranking always exits 0", case);
    assert!(!reply.is_error, "{}: a ranking is never an error", case);
    let p = &reply.json;

    // Reproducible, byte for byte, over the same corpus and the same query.
    let again = aval::relevant::relevant_in(l, &query);
    assert_eq!(
        p.to_string(),
        again.json.to_string(),
        "{}: two rankings of one corpus differ",
        case
    );
    assert_eq!(
        reply.text, again.text,
        "{}: the text rendering differs",
        case
    );

    if let Some(order) = exp.get("order").and_then(|o| o.as_arr()) {
        let want: Vec<&str> = order.iter().filter_map(|x| x.as_str()).collect();
        let got: Vec<&str> = keys_of(p)
            .iter()
            .filter_map(|r| r.get("key").and_then(|k| k.as_str()))
            .collect();
        assert_eq!(got, want, "{}: ranked order", case);
        // The compact routing list is the same keys in the same order.
        let deps: Vec<&str> = p
            .get("dependencies")
            .and_then(|d| d.as_arr())
            .expect("dependencies")
            .iter()
            .filter_map(|d| d.get("key").and_then(|k| k.as_str()))
            .collect();
        assert_eq!(deps, want, "{}: dependencies", case);
    }
    for (key, want) in pairs(exp, "states") {
        assert_eq!(
            row(p, &key).get("state").and_then(|s| s.as_str()),
            want.as_str(),
            "{}: state of {}",
            case,
            key
        );
    }
    for (key, want) in pairs(exp, "exits") {
        assert_eq!(
            row(p, &key).get("exit").and_then(|e| e.as_i64()),
            want.as_i64(),
            "{}: exit of {}",
            case,
            key
        );
    }
    for (key, want) in pairs(exp, "adrs") {
        assert_eq!(
            row(p, &key).get("adr").and_then(|a| a.as_str()),
            want.as_str(),
            "{}: adr of {}",
            case,
            key
        );
    }
    // How many of the caller's paths each path signal actually matched. A
    // count rather than the list: which path matched is the same fact, and a
    // fixture asserting the list would pin the order a caller passed them in.
    for field in ["mentions", "co_changed"] {
        for (key, want) in pairs(exp, field) {
            let n = row(p, &key)
                .get("why")
                .and_then(|w| w.get(field))
                .and_then(|m| m.as_arr())
                .map(<[Json]>::len)
                .unwrap_or(0);
            assert_eq!(
                n as i64,
                want.as_i64().expect("a count"),
                "{}: {} of {}",
                case,
                field,
                key
            );
        }
    }
    for (key, want) in pairs(exp, "elsewhere") {
        let scopes: Vec<&str> = row(p, &key)
            .get("why")
            .and_then(|w| w.get("decided_elsewhere"))
            .and_then(|e| e.as_arr())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.get("scope").and_then(|s| s.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        let want: Vec<&str> = want
            .as_arr()
            .expect("a list")
            .iter()
            .filter_map(|x| x.as_str())
            .collect();
        assert_eq!(scopes, want, "{}: where else {} is decided", case, key);
    }
    if let Some(rules) = exp.get("rules").and_then(|r| r.as_arr()) {
        let want: Vec<&str> = rules.iter().filter_map(|x| x.as_str()).collect();
        let got: Vec<&str> = p
            .get("rules")
            .and_then(|r| r.as_arr())
            .expect("rules")
            .iter()
            .filter_map(|r| r.get("id").and_then(|i| i.as_str()))
            .collect();
        assert_eq!(got, want, "{}: ranked rules", case);
    }
    // Whatever else it says, it says what it is.
    assert_eq!(
        p.get("kind").and_then(|k| k.as_str()),
        Some("suggestion"),
        "{}: a ranking declares itself a suggestion",
        case
    );
}

#[test]
fn the_relevance_battery_is_non_trivial() {
    let b = relevance_battery();
    let n = b
        .get("cases")
        .and_then(|c| c.as_arr())
        .map_or(0, |c| c.len());
    let min = b.get("min_cases").and_then(|m| m.as_i64()).unwrap_or(0) as usize;
    assert!(min > 0, "relevant.json must declare min_cases");
    assert!(
        n >= min,
        "battery has {} cases, fewer than the declared minimum {}",
        n,
        min
    );
}

#[test]
fn every_relevance_case_holds() {
    let b = relevance_battery();
    let cases = b.get("cases").and_then(|c| c.as_arr()).expect("cases");
    let mut seen: Vec<&str> = Vec::new();
    for c in cases {
        let name = want_str(c, "name").expect("case.name");
        assert!(!seen.contains(&name), "duplicate case name `{}`", name);
        seen.push(name);
        let l = loaded(want_str(c, "corpus").expect("case.corpus"));
        run_relevance(
            name,
            &l,
            c.get("query").expect("case.query"),
            c.get("expected").expect("case.expected"),
        );
    }
}

// --- traits (SEMANTICS section 2.5) -----------------------------------------

fn traits_battery() -> Json {
    let p = conformance_dir().join("traits.json");
    let src = std::fs::read_to_string(&p).expect("read traits.json");
    json::parse(&src).expect("traits.json parses")
}

fn obj_keys(v: &Json) -> Vec<String> {
    match v {
        Json::Obj(m) => m.keys().cloned().collect(),
        _ => panic!("expected an object, found {}", v),
    }
}

fn strings(v: &Json) -> Vec<String> {
    v.as_arr()
        .unwrap_or(&[])
        .iter()
        .map(|x| x.as_str().expect("a string").to_string())
        .collect()
}

#[test]
fn the_traits_battery_is_non_trivial() {
    let b = traits_battery();
    for (field, min) in [("cases", "min_cases"), ("globs", "min_globs")] {
        let n = b.get(field).and_then(|c| c.as_arr()).map_or(0, |c| c.len());
        let m = b.get(min).and_then(|m| m.as_i64()).unwrap_or(0) as usize;
        assert!(m > 0, "traits.json must declare {}", min);
        assert!(n >= m, "{}: {} entries, fewer than {}", field, n, m);
    }
}

#[test]
fn every_glob_case_holds() {
    use aval_core::glob;
    let b = traits_battery();
    for c in b.get("globs").and_then(|c| c.as_arr()).expect("globs") {
        let g = want_str(c, "glob").expect("glob");
        for k in obj_keys(c) {
            assert!(
                ["glob", "path", "matches", "valid"].contains(&k.as_str()),
                "unknown glob-case field `{}`",
                k
            );
        }
        match c.get("valid") {
            Some(Json::Bool(false)) => {
                assert!(glob::bad(g).is_some(), "`{}` must be refused", g);
                assert!(!glob::matches(g, "a/b"), "`{}` must match nothing", g);
            }
            Some(_) => panic!("`valid` is only ever false"),
            None => {
                assert!(glob::bad(g).is_none(), "`{}` must be accepted", g);
                let p = want_str(c, "path").expect("path");
                let want = matches!(c.get("matches"), Some(Json::Bool(true)));
                assert_eq!(glob::matches(g, p), want, "`{}` against `{}`", g, p);
            }
        }
    }
}

#[test]
fn every_applicability_case_holds() {
    use aval_core::applicability::{filter, Scope};
    let b = traits_battery();
    let mut seen: Vec<String> = Vec::new();
    for c in b.get("cases").and_then(|c| c.as_arr()).expect("cases") {
        let name = want_str(c, "name").expect("case.name").to_string();
        assert!(!seen.contains(&name), "duplicate case `{}`", name);
        seen.push(name.clone());
        let l = loaded(want_str(c, "corpus").expect("case.corpus"));
        let paths = strings(
            c.get("query")
                .and_then(|q| q.get("paths"))
                .expect("query.paths"),
        );
        let exp = c.get("expected").expect("expected");
        for k in obj_keys(exp) {
            assert!(
                ["kept", "omitted"].contains(&k.as_str()),
                "{}: unknown expected field `{}`",
                name,
                k
            );
        }
        let reg = l.graph.registry();
        let active: Vec<_> = l
            .graph
            .corpus()
            .rules
            .iter()
            .filter(|r| l.graph.rule_reason(r).is_none())
            .collect();
        let (kept, omitted) = filter(reg, &Scope::for_query(reg, &paths), active);
        let mut ids: Vec<String> = kept.iter().map(|r| r.id.clone()).collect();
        ids.sort();
        assert_eq!(
            ids,
            strings(exp.get("kept").expect("kept")),
            "{}: kept",
            name
        );
        match exp.get("omitted") {
            Some(Json::Null) | None => {
                assert!(omitted.is_none(), "{}: omitted must be absent", name)
            }
            Some(o) => {
                let got = omitted.unwrap_or_else(|| panic!("{}: omitted must be present", name));
                assert_eq!(
                    aval::render::omitted_json(&got).to_string(),
                    o.to_string(),
                    "{}: omitted",
                    name
                );
            }
        }
    }
}
