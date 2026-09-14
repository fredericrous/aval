//! Rules, from the outside: the two commands, the three checks, and the trip
//! through a pack.
//!
//! The property every one of these is really about is that a corpus with no
//! `rules:` in its registry cannot tell this release from the last one. That is
//! what makes 1.2.0 minor, and it is asserted here rather than argued for.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_aval")
}

struct Run {
    code: i32,
    out: String,
    err: String,
}

fn run(dir: &Path, args: &[&str]) -> Run {
    let o = Command::new(bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn aval");
    Run {
        code: o.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&o.stdout).to_string(),
        err: String::from_utf8_lossy(&o.stderr).to_string(),
    }
}

fn scratch(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/rules-tests")
        .join(name);
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(p.join("docs/adr")).expect("mkdir");
    p
}

fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    if let Some(d) = p.parent() {
        fs::create_dir_all(d).expect("mkdir");
    }
    fs::write(p, body).expect("write");
}

const ADR: &str = "---\nid: ADR-0001\nstatus: accepted\ndecisions:\n  \
                   - key: a.b\n    choice: One\n    first: true\n---\n# one\n";

const RULES: &str = "---\nadopts: ADR-0001\nsource: A Book (2008)\n---\n\
                     # Restated\n\n\
                     ## names.reveal-intent [constraint]\n\n\
                     Names reveal intention.\n\n\
                     Because an abbreviation is a private vocabulary.\n\n\
                     ## functions.few-arguments [heuristic]\n\n\
                     A function takes no more inputs than it uses.\n";

/// A corpus with one record and two rules.
fn corpus(name: &str) -> PathBuf {
    let r = scratch(name);
    write(
        &r,
        ".adr.yaml",
        // A declared scope nothing uses: a corpus with an EMPTY `scopes` list
        // publishes a pack no consumer can read (`scopes:` with no items reads
        // back as empty, not as a list), which is a defect of its own and not
        // this test's subject.
        "dir: docs/adr\nrules:\n  - docs/principles/book.md\nscopes: [cloud]\nkeys:\n  a.b:\n",
    );
    write(&r, "docs/adr/0001-a.md", ADR);
    write(&r, "docs/principles/book.md", RULES);
    assert_eq!(run(&r, &["heads", "--write"]).code, 0);
    r
}

// --- the two commands ------------------------------------------------------

#[test]
fn rules_lists_constraints_first_then_heuristics() {
    let r = corpus("listing");
    let got = run(&r, &["rules"]);
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    let lines: Vec<&str> = got.out.lines().collect();
    assert_eq!(lines.len(), 2, "{}", got.out);
    assert!(
        lines[0].starts_with("constraint names.reveal-intent"),
        "{}",
        got.out
    );
    assert!(
        lines[1].starts_with("heuristic  functions.few-arguments"),
        "{}",
        got.out
    );
    // The statement, not the body: this list is what there is, not why.
    assert!(lines[0].ends_with("Names reveal intention."), "{}", got.out);
    assert!(!got.out.contains("private vocabulary"), "{}", got.out);
}

#[test]
fn rules_filters_by_level_and_by_adopting_record() {
    let r = corpus("filters");
    let one = run(&r, &["rules", "--level", "heuristic"]);
    assert_eq!(one.out.lines().count(), 1, "{}", one.out);
    assert!(one.out.contains("functions.few-arguments"));

    assert_eq!(
        run(&r, &["rules", "--adopted-by", "ADR-0001"])
            .out
            .lines()
            .count(),
        2
    );
    // A record that adopts nothing is an empty list and exit 0: an empty list
    // is an answer.
    let none = run(&r, &["rules", "--adopted-by", "ADR-9999"]);
    assert_eq!(none.code, 0);
    assert_eq!(none.out, "");

    assert_eq!(run(&r, &["rules", "--level", "advisory"]).code, 2);
}

/// Activity is not a check: a rule whose record was superseded is inactive,
/// listed under `--all` with the reason, and `check` says nothing about it.
#[test]
fn an_inactive_rule_is_shown_with_its_reason_and_is_not_a_finding() {
    let r = scratch("inactive");
    write(
        &r,
        ".adr.yaml",
        "dir: docs/adr\nrules:\n  - docs/p/early.md\n  - docs/p/draft.md\nscopes: []\nkeys:\n  a.b:\n  c.d:\n",
    );
    write(&r, "docs/adr/0001-a.md", ADR);
    write(
        &r,
        "docs/adr/0002-b.md",
        "---\nid: ADR-0002\nstatus: accepted\ndecisions:\n  \
         - key: a.b\n    choice: Two\n    replaces: [ADR-0001]\n---\n# two\n",
    );
    write(
        &r,
        "docs/adr/0003-c.md",
        "---\nid: ADR-0003\nstatus: draft\ndecisions:\n  \
         - key: c.d\n    choice: Proposed\n    first: true\n---\n# three\n",
    );
    write(
        &r,
        "docs/p/early.md",
        "---\nadopts: ADR-0001\n---\n## a.early [constraint]\n\nEarly.\n",
    );
    write(
        &r,
        "docs/p/draft.md",
        "---\nadopts: ADR-0003\n---\n## a.proposed [constraint]\n\nProposed.\n",
    );
    assert_eq!(run(&r, &["heads", "--write"]).code, 0);

    let active = run(&r, &["rules"]);
    assert_eq!(active.code, 0, "{}{}", active.out, active.err);
    assert_eq!(active.out, "", "neither rule's record holds");

    let all = run(&r, &["rules", "--all"]);
    assert!(
        all.out
            .contains("(inactive: adopting record ADR-0001 is superseded)"),
        "{}",
        all.out
    );
    assert!(
        all.out
            .contains("(inactive: adopting record ADR-0003 is a draft)"),
        "{}",
        all.out
    );

    let check = run(&r, &["check"]);
    assert_eq!(check.code, 0, "{}{}", check.out, check.err);
}

#[test]
fn rule_prints_the_body_and_exits_seven_for_an_unknown_id() {
    let r = corpus("one-rule");
    let got = run(&r, &["rule", "names.reveal-intent"]);
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    assert!(
        got.out.contains("adopts: ADR-0001   (active)"),
        "{}",
        got.out
    );
    assert!(got.out.contains("source: A Book (2008)"), "{}", got.out);
    assert!(got.out.contains("private vocabulary"), "{}", got.out);

    let miss = run(&r, &["rule", "names.reveal-intnet"]);
    assert_eq!(miss.code, 7);
    assert!(
        miss.err.contains("did you mean `names.reveal-intent`"),
        "{}",
        miss.err
    );
    // Section 12: the text of a miss goes to stderr, so stdout stays a
    // machine's business.
    assert_eq!(miss.out, "");

    let json = run(&r, &["rule", "names.reveal-intnet", "--json"]);
    assert_eq!(json.code, 7);
    assert!(json.out.contains(r#""state":"unknown""#), "{}", json.out);
    assert!(json.out.contains(r#""exit":7"#), "{}", json.out);
    assert!(
        json.out.contains(r#""rule":"names.reveal-intnet""#),
        "{}",
        json.out
    );

    // A rule id is corpus-local, exactly as a record id is.
    assert_eq!(
        run(&r, &["rule", "names.reveal-intent", "--all-repos"]).code,
        2
    );
}

#[test]
fn the_json_shapes_carry_every_field() {
    let r = corpus("json");
    let list = run(&r, &["rules", "--json"]);
    assert_eq!(list.code, 0);
    assert!(list.out.starts_with(r#"{"rules":["#), "{}", list.out);
    assert!(list.out.contains(r#""active":true"#), "{}", list.out);
    assert!(list.out.contains(r#""level":"constraint""#), "{}", list.out);
    assert!(list.out.contains(r#""adopts":"ADR-0001""#), "{}", list.out);
    assert!(
        list.out.contains(r#""file":"docs/principles/book.md""#),
        "{}",
        list.out
    );
    assert!(
        list.out.contains(r#""source":"A Book (2008)""#),
        "{}",
        list.out
    );
    // No bodies in the list, and no pack where nothing was vendored.
    assert!(!list.out.contains(r#""body""#), "{}", list.out);
    assert!(!list.out.contains(r#""pack""#), "{}", list.out);
    assert!(!list.out.contains("inactive_reason"), "{}", list.out);

    let one = run(&r, &["rule", "names.reveal-intent", "--json"]);
    assert!(
        one.out.contains(r#""body":"Because an abbreviation"#),
        "{}",
        one.out
    );
    assert!(one.out.contains(r#""active":true"#), "{}", one.out);
}

// --- the checks ------------------------------------------------------------

#[test]
fn a_rules_file_that_is_listed_and_missing_is_an_error() {
    let r = corpus("missing");
    fs::remove_file(r.join("docs/principles/book.md")).unwrap();
    let got = run(&r, &["rules"]);
    assert_eq!(got.code, 3, "{}{}", got.out, got.err);
    assert!(got.err.contains("listed in `rules`"), "{}", got.err);
}

#[test]
fn a_near_miss_heading_fails_the_load_rather_than_becoming_body_text() {
    let r = corpus("near-miss");
    write(
        &r,
        "docs/principles/book.md",
        "---\nadopts: ADR-0001\n---\n## a.b [constraint]\n\nOne.\n\n## c.d [constrain]\n\nTwo.\n",
    );
    let got = run(&r, &["check"]);
    assert_eq!(got.code, 3, "{}{}", got.out, got.err);
    assert!(got.err.contains("rules-parse"), "{}", got.err);
    assert!(got.err.contains("is not a rule heading"), "{}", got.err);
}

#[test]
fn a_rule_must_adopt_a_record_this_corpus_carries() {
    let r = corpus("adopts");
    write(
        &r,
        "docs/principles/book.md",
        "---\nadopts: ADR-0404\n---\n## a.b [constraint]\n\nOne.\n",
    );
    let got = run(&r, &["check"]);
    assert_eq!(got.code, 3, "{}{}", got.out, got.err);
    assert!(got.err.contains("rule-adopts-resolves"), "{}", got.err);
}

/// A rule file is a document of this repository, so its citations are checked
/// like a record's — Layer C, which never stops an answer.
#[test]
fn links_resolve_runs_over_a_rule_body() {
    let r = corpus("links");
    // Its own repository, because the scratch tree lives under `target/`, which
    // THIS repository ignores — and `links-resolve` skips ignored paths, so
    // without this the check would answer "ignored" and the test would assert
    // nothing.
    assert!(Command::new("git")
        .args(["init", "-q"])
        .current_dir(&r)
        .status()
        .expect("git init")
        .success());
    write(
        &r,
        "docs/principles/book.md",
        "---\nadopts: ADR-0001\n---\n## a.b [constraint]\n\nOne.\n\n\
         See `docs/adr/0404-gone.md` for the case it does not cover.\n",
    );
    let got = run(&r, &["check"]);
    assert_eq!(got.code, 1, "{}{}", got.out, got.err);
    assert!(got.out.contains("links-resolve"), "{}", got.out);
    assert!(got.out.contains("docs/principles/book.md"), "{}", got.out);
    // Layer C never stops an answer.
    assert_eq!(run(&r, &["rules"]).code, 0);
}

// --- a corpus with no rules ------------------------------------------------

/// The property that makes this release minor.
#[test]
fn a_corpus_without_rules_is_unchanged() {
    let r = scratch("no-rules");
    write(
        &r,
        ".adr.yaml",
        "dir: docs/adr\nscopes: []\nkeys:\n  a.b:\n",
    );
    write(&r, "docs/adr/0001-a.md", ADR);
    assert_eq!(run(&r, &["heads", "--write"]).code, 0);

    let check = run(&r, &["check"]);
    assert_eq!(check.code, 0, "{}{}", check.out, check.err);

    // The commands still answer, with the empty answer.
    let rules = run(&r, &["rules", "--all"]);
    assert_eq!(rules.code, 0);
    assert_eq!(rules.out, "");
    assert_eq!(run(&r, &["rules", "--json"]).out.trim(), r#"{"rules":[]}"#);
    assert_eq!(run(&r, &["rule", "a.b"]).code, 7);

    // And a pack published from it carries no rules section at all.
    assert_eq!(run(&r, &["pack", "--write"]).code, 0);
    let pack = fs::read_to_string(r.join("aval.pack")).unwrap();
    assert!(!pack.contains("rules:"), "{}", pack);
}

// --- through a pack --------------------------------------------------------

/// The whole trip: a producer publishes rules, a consumer vendors them, and the
/// consumer's `rules` lists them with their bodies intact.
#[test]
fn a_consumer_vendors_the_rules_with_the_decisions() {
    let producer = corpus("producer");
    assert_eq!(run(&producer, &["pack", "--write"]).code, 0);

    let c = scratch("consumer");
    write(
        &c,
        ".adr.yaml",
        "dir: docs/adr\nscopes: []\nkeys:\n  x.y:\n",
    );
    write(
        &c,
        "docs/adr/0001-x.md",
        "---\nid: ADR-0001\nstatus: accepted\ndecisions:\n  \
         - key: x.y\n    choice: Mine\n    first: true\n---\n# mine\n",
    );
    // Vendored by hand rather than by `aval add`, which resolves a revision
    // over git: the trip being tested is the pack's, and `add`'s fetching has
    // its own tests.
    write(
        &c,
        ".adr/packs/fleet.yaml",
        &fs::read_to_string(producer.join("aval.pack")).unwrap(),
    );
    write(
        &c,
        ".adr.yaml",
        "dir: docs/adr\npacks:\n  - .adr/packs/fleet.yaml\nscopes: []\nkeys:\n  x.y:\n",
    );
    assert_eq!(run(&c, &["heads", "--write"]).code, 0);

    let got = run(&c, &["rules"]);
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    assert!(got.out.contains("names.reveal-intent"), "{}", got.out);

    let one = run(&c, &["rule", "names.reveal-intent", "--json"]);
    assert!(one.out.contains(r#""pack":"fleet""#), "{}", one.out);
    // Qualified on the way in, like every other reference into a pack.
    assert!(
        one.out.contains(r#""adopts":"fleet:ADR-0001""#),
        "{}",
        one.out
    );
    // The body survived the block scalar, paragraph break and all.
    assert!(one.out.contains("private vocabulary"), "{}", one.out);

    // A consumer that re-declares a vendored rule has two statements of one
    // practice, and nothing keeps them in step.
    write(
        &c,
        ".adr.yaml",
        &format!(
            "{}rules:\n  - docs/p/mine.md\n",
            fs::read_to_string(c.join(".adr.yaml")).unwrap()
        ),
    );
    write(
        &c,
        "docs/p/mine.md",
        "---\nadopts: ADR-0001\n---\n## names.reveal-intent [constraint]\n\nMine.\n",
    );
    let clash = run(&c, &["check"]);
    assert_eq!(clash.code, 3, "{}{}", clash.out, clash.err);
    assert!(clash.err.contains("rule-id-unique"), "{}", clash.err);
    assert!(
        clash.err.contains("vendored from the `fleet` pack"),
        "{}",
        clash.err
    );

    // And a local rule may not adopt a vendored record: another repository's
    // rules arrive with its pack.
    write(
        &c,
        "docs/p/mine.md",
        "---\nadopts: fleet:ADR-0001\n---\n## mine.own [constraint]\n\nMine.\n",
    );
    let vendored = run(&c, &["check"]);
    assert_eq!(vendored.code, 3, "{}{}", vendored.out, vendored.err);
    assert!(
        vendored.err.contains("is a vendored record"),
        "{}",
        vendored.err
    );

    // A pack never re-exports what it vendored.
    write(
        &c,
        "docs/p/mine.md",
        "---\nadopts: ADR-0001\n---\n## mine.own [constraint]\n\nMine.\n",
    );
    assert_eq!(run(&c, &["pack", "--write"]).code, 0);
    let pack = fs::read_to_string(c.join("aval.pack")).unwrap();
    assert!(pack.contains("mine.own"), "{}", pack);
    assert!(!pack.contains("names.reveal-intent"), "{}", pack);
}
