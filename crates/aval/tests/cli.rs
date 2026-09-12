//! The binary, run as a binary.
//!
//! Discovery, the freshness gate and the write path are the parts of this tool
//! that touch a filesystem, and until now nothing spawned the command at all —
//! `heads --check` exit codes, `--write` idempotence and `sources` resolution
//! had no coverage. The conformance battery cannot reach them: it drives the
//! graph, not the CLI.

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

/// A scratch corpus. Uses the target directory rather than a system temp dir
/// so a failed run leaves something inspectable.
fn scratch(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/cli-tests")
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

const REGISTRY: &str = "dir: docs/adr\nscopes: [cloud]\nkeys:\n  a.b:\n  c.d:\n";

const NUMBERED: &str = "---\nid: ADR-0001\nstatus: accepted\ndecisions:\n  \
                        - key: a.b\n    choice: One\n    first: true\n---\n# one\n";

const LISTED: &str = "---\nid: a-specification\nstatus: accepted\ndecisions:\n  \
                      - key: c.d\n    choice: Two\n    first: true\n---\n# spec\n";

// --- discovery -------------------------------------------------------------

#[test]
fn a_specification_named_in_sources_is_a_record() {
    let r = scratch("sources-basic");
    write(
        &r,
        ".adr.yaml",
        &format!("{}sources:\n  - docs/a-specification.md\n", REGISTRY),
    );
    write(&r, "docs/adr/0001-one.md", NUMBERED);
    write(&r, "docs/a-specification.md", LISTED);

    assert_eq!(run(&r, &["heads", "--write"]).code, 0);
    let got = run(&r, &["resolve", "c.d"]);
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    assert!(got.out.contains("a-specification"), "{}", got.out);
}

/// The property that makes literal paths safe where a pattern is not: a record
/// must never be able to stop being one quietly. A pattern that stops matching
/// drops the record, its entries leave the graph, whatever it superseded comes
/// back as a head, and `resolve` answers `active` with a replaced decision.
#[test]
fn a_listed_file_that_is_missing_is_an_error() {
    let r = scratch("sources-missing");
    write(
        &r,
        ".adr.yaml",
        &format!("{}sources:\n  - docs/not-here.md\n", REGISTRY),
    );
    write(&r, "docs/adr/0001-one.md", NUMBERED);

    let got = run(&r, &["check"]);
    assert_eq!(got.code, 3, "{}{}", got.out, got.err);
    assert!(got.err.contains("not-here.md"), "{}", got.err);
}

#[test]
fn a_listed_file_without_frontmatter_is_an_error() {
    let r = scratch("sources-no-frontmatter");
    write(
        &r,
        ".adr.yaml",
        &format!("{}sources:\n  - docs/prose.md\n", REGISTRY),
    );
    write(&r, "docs/adr/0001-one.md", NUMBERED);
    write(&r, "docs/prose.md", "# just prose\n");

    let got = run(&r, &["check"]);
    assert_eq!(got.code, 3, "{}{}", got.out, got.err);
    assert!(got.err.contains("frontmatter"), "{}", got.err);
}

/// Listing a file that `dir` already found must not turn it into two records
/// and report `id-unique` against a file and itself.
#[test]
fn a_file_reached_by_both_rules_is_read_once() {
    let r = scratch("sources-overlap");
    write(
        &r,
        ".adr.yaml",
        &format!("{}sources:\n  - docs/adr/0001-one.md\n", REGISTRY),
    );
    write(&r, "docs/adr/0001-one.md", NUMBERED);
    assert_eq!(run(&r, &["heads", "--write"]).code, 0);

    let got = run(&r, &["check"]);
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    assert!(
        !got.err.contains("id-unique"),
        "a file read twice would collide with itself: {}",
        got.err
    );
}

/// The `dir` rule wins the deduplication, so mandatory-frontmatter enforcement
/// is never traded away by also listing the file.
#[test]
fn the_numbered_rule_wins_the_overlap() {
    let r = scratch("sources-overlap-id");
    write(
        &r,
        ".adr.yaml",
        &format!("{}sources:\n  - docs/adr/0001-one.md\n", REGISTRY),
    );
    // Wrong id for its filename: only the numbered rule objects.
    write(
        &r,
        "docs/adr/0001-one.md",
        &NUMBERED.replace("ADR-0001", "ADR-0009"),
    );

    let got = run(&r, &["check"]);
    assert_eq!(got.code, 3, "{}{}", got.out, got.err);
    assert!(got.err.contains("id-matches-filename"), "{}", got.err);
}

#[test]
fn a_pattern_in_sources_is_refused_with_the_reason() {
    let r = scratch("sources-pattern");
    write(
        &r,
        ".adr.yaml",
        &format!("{}sources:\n  - 'docs/*.md'\n", REGISTRY),
    );
    write(&r, "docs/adr/0001-one.md", NUMBERED);

    let got = run(&r, &["check"]);
    assert_eq!(got.code, 3, "{}{}", got.out, got.err);
    assert!(got.err.contains("list each file"), "{}", got.err);
}

#[test]
fn a_listed_record_may_not_impersonate_a_numbered_one() {
    let r = scratch("sources-fake-adr");
    write(
        &r,
        ".adr.yaml",
        &format!("{}sources:\n  - docs/spec.md\n", REGISTRY),
    );
    write(&r, "docs/adr/0001-one.md", NUMBERED);
    write(
        &r,
        "docs/spec.md",
        &LISTED.replace("a-specification", "ADR-0042"),
    );

    let got = run(&r, &["check"]);
    assert_eq!(got.code, 3, "{}{}", got.out, got.err);
    assert!(got.err.contains("reserved"), "{}", got.err);
}

/// Two records with the same basename in different directories. The Layer C
/// join is by path for exactly this reason; by basename it would match the
/// wrong document, or both.
#[test]
fn two_records_may_share_a_basename() {
    let r = scratch("sources-same-basename");
    write(
        &r,
        ".adr.yaml",
        &format!("{}sources:\n  - docs/specs/notes.md\n", REGISTRY),
    );
    write(&r, "docs/adr/0001-one.md", NUMBERED);
    write(&r, "docs/specs/notes.md", LISTED);
    // A decoy with the same basename that is not a record.
    write(&r, "docs/notes.md", "# unrelated\n");
    assert_eq!(run(&r, &["heads", "--write"]).code, 0);

    let got = run(&r, &["check"]);
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
}

// --- freshness and the write path -----------------------------------------

fn corpus_with_heads(name: &str) -> PathBuf {
    let r = scratch(name);
    write(&r, ".adr.yaml", REGISTRY);
    write(&r, "docs/adr/0001-one.md", NUMBERED);
    assert_eq!(run(&r, &["heads", "--write"]).code, 0);
    r
}

#[test]
fn a_formatted_projection_stays_current_and_keeps_its_bytes() {
    let r = corpus_with_heads("heads-formatted");
    let p = r.join("docs/adr/HEADS.md");
    let canonical = fs::read_to_string(&p).unwrap();

    // Prettier's shape: padded cells, a rewritten delimiter row.
    let formatted = canonical
        .replace(
            "| Key | Scope | Record | Decision |\n|---|---|---|---|",
            "| Key | Scope | Record   | Decision |\n| --- | ----- | -------- | -------- |",
        )
        .replace(
            "| a.b | — | ADR-0001 | One |",
            "| a.b | —     | ADR-0001 | One      |",
        );
    assert_ne!(formatted, canonical, "the fixture must actually differ");
    fs::write(&p, &formatted).unwrap();

    let got = run(&r, &["heads", "--check"]);
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);

    assert_eq!(run(&r, &["heads", "--write"]).code, 0);
    assert_eq!(
        fs::read_to_string(&p).unwrap(),
        formatted,
        "`--write` must leave a current file alone, or the formatter and the \
         tool undo each other on every commit"
    );
}

#[test]
fn a_stale_projection_names_the_row_and_is_repaired() {
    let r = corpus_with_heads("heads-stale");
    let p = r.join("docs/adr/HEADS.md");
    let canonical = fs::read_to_string(&p).unwrap();
    fs::write(&p, canonical.replace("ADR-0001", "ADR-0099")).unwrap();

    let got = run(&r, &["heads", "--check"]);
    assert_eq!(got.code, 1, "{}{}", got.out, got.err);
    assert!(got.err.contains("ADR-0099"), "{}", got.err);

    assert_eq!(run(&r, &["heads", "--write"]).code, 0);
    assert_eq!(fs::read_to_string(&p).unwrap(), canonical);
    assert_eq!(run(&r, &["heads", "--check"]).code, 0);
}

/// Section 9: a `HEADS.md` that cannot be read as the projection must still be
/// repairable by `--write`, or the idempotence above would create a state with
/// no way out.
#[test]
fn an_unreadable_projection_is_overwritten() {
    let r = corpus_with_heads("heads-garbage");
    let p = r.join("docs/adr/HEADS.md");
    let canonical = fs::read_to_string(&p).unwrap();
    fs::write(&p, "not a projection at all\n").unwrap();

    assert_eq!(run(&r, &["heads", "--check"]).code, 1);
    assert_eq!(run(&r, &["heads", "--write"]).code, 0);
    assert_eq!(fs::read_to_string(&p).unwrap(), canonical);
}

#[test]
fn a_missing_projection_is_reported_then_written() {
    let r = corpus_with_heads("heads-missing");
    let p = r.join("docs/adr/HEADS.md");
    fs::remove_file(&p).unwrap();

    let got = run(&r, &["heads", "--check"]);
    assert_eq!(got.code, 1, "{}{}", got.out, got.err);
    assert!(got.err.contains("missing"), "{}", got.err);

    assert_eq!(run(&r, &["heads", "--write"]).code, 0);
    assert!(p.is_file());
}

/// Section 12: `--json` writes the result object to stdout and nothing else.
/// All three of these printed nothing at all under `--json`, and `--check`
/// sent its findings to stderr, so the row-level detail this release adds was
/// unreachable from a machine caller.
#[test]
fn heads_speaks_json_on_stdout() {
    let r = corpus_with_heads("heads-json");
    let p = r.join("docs/adr/HEADS.md");

    let got = run(&r, &["heads", "--check", "--json"]);
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    assert!(got.out.contains("\"state\":\"current\""), "{}", got.out);
    assert!(got.err.is_empty(), "stderr must stay empty: {}", got.err);

    let canonical = fs::read_to_string(&p).unwrap();
    fs::write(&p, canonical.replace("ADR-0001", "ADR-0099")).unwrap();

    let got = run(&r, &["heads", "--check", "--json"]);
    assert_eq!(got.code, 1, "{}{}", got.out, got.err);
    assert!(got.out.contains("\"state\":\"stale\""), "{}", got.out);
    assert!(
        got.out.contains("ADR-0099"),
        "the finding must reach stdout: {}",
        got.out
    );
    assert!(got.err.is_empty(), "stderr must stay empty: {}", got.err);

    let got = run(&r, &["heads", "--write", "--json"]);
    assert_eq!(got.code, 0);
    assert!(got.out.contains("\"state\":\"written\""), "{}", got.out);
    let got = run(&r, &["heads", "--write", "--json"]);
    assert!(got.out.contains("\"state\":\"unchanged\""), "{}", got.out);
}

// --- keys ------------------------------------------------------------------
//
// Discovery, and the distinctions it must not flatten.

#[test]
fn keys_lists_the_vocabulary_and_where_each_is_decided() {
    let r = scratch("keys-basic");
    write(&r, ".adr.yaml", REGISTRY);
    write(&r, "docs/adr/0001-one.md", NUMBERED);

    let got = run(&r, &["keys"]);
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    assert!(got.out.contains("a.b"), "{}", got.out);
    assert!(got.out.contains("c.d"), "{}", got.out);
    // a.b is decided; c.d is declared and undecided, and must still be listed —
    // a key nothing has answered is exactly what a caller needs to discover.
    assert!(got.out.contains("active"), "{}", got.out);
}

#[test]
fn keys_distinguishes_an_absent_scope_list_from_an_empty_one() {
    // `null` admits every declared scope; `[]` admits only the default one.
    // Collapsing either into the other silently changes which questions the
    // registry says are answerable.
    let r = scratch("keys-scopes");
    write(
        &r,
        ".adr.yaml",
        "dir: docs/adr\nscopes: [cloud]\nkeys:\n  \
         open.key:\n  fleet.key:\n    scopes: []\n  narrow.key:\n    scopes: [cloud]\n",
    );
    let got = run(&r, &["keys", "--json"]);
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    assert!(
        got.out.contains(r#""key":"open.key","scopes":null"#),
        "{}",
        got.out
    );
    assert!(
        got.out.contains(r#""key":"fleet.key","scopes":[]"#),
        "{}",
        got.out
    );
    assert!(
        got.out.contains(r#""key":"narrow.key","scopes":["cloud"]"#),
        "{}",
        got.out
    );
}

#[test]
fn keys_reports_competing_heads_as_a_list() {
    // A contradiction has several records. Joining them into one string would
    // make a caller split it back out, and `adrs` is where they belong.
    let r = scratch("keys-contradiction");
    write(
        &r,
        ".adr.yaml",
        "dir: docs/adr\nscopes: []\nkeys:\n  a.b:\n",
    );
    write(
        &r,
        "docs/adr/0001-one.md",
        "---\nid: ADR-0001\nstatus: accepted\ndecisions:\n  \
         - key: a.b\n    choice: One\n    first: true\n---\n# one\n",
    );
    write(
        &r,
        "docs/adr/0002-two.md",
        "---\nid: ADR-0002\nstatus: accepted\ndecisions:\n  \
         - key: a.b\n    choice: Two\n    first: true\n---\n# two\n",
    );
    let got = run(&r, &["keys", "--json"]);
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    assert!(
        got.out.contains(r#""state":"contradiction""#),
        "{}",
        got.out
    );
    assert!(
        got.out.contains(r#""adrs":["ADR-0001","ADR-0002"]"#),
        "{}",
        got.out
    );
}

#[test]
fn keys_takes_no_arguments() {
    let r = scratch("keys-usage");
    write(&r, ".adr.yaml", REGISTRY);
    assert_eq!(run(&r, &["keys", "a.b"]).code, 2);
}

// --- heads --json ----------------------------------------------------------

#[test]
fn heads_json_reports_a_contradiction_the_projection_omits() {
    // project::head_slots keeps only slots with exactly one head, so HEADS.md
    // renders a contradicted corpus as empty: "Active: None", and nothing to
    // say two records are fighting. That is tolerable in a document a person
    // reads beside the corpus and not in an answer to a caller, which would
    // read "None" as settled.
    let r = scratch("heads-json-contradiction");
    write(
        &r,
        ".adr.yaml",
        "dir: docs/adr\nscopes: []\nkeys:\n  a.b:\n",
    );
    write(
        &r,
        "docs/adr/0001-one.md",
        "---\nid: ADR-0001\nstatus: accepted\ndecisions:\n  \
         - key: a.b\n    choice: One\n    first: true\n---\n# one\n",
    );
    write(
        &r,
        "docs/adr/0002-two.md",
        "---\nid: ADR-0002\nstatus: accepted\ndecisions:\n  \
         - key: a.b\n    choice: Two\n    first: true\n---\n# two\n",
    );

    let md = run(&r, &["heads"]);
    assert_eq!(md.code, 0, "{}{}", md.out, md.err);
    assert!(
        md.out.contains("None."),
        "projection should be empty: {}",
        md.out
    );

    let got = run(&r, &["heads", "--json"]);
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    assert!(
        got.out.contains(r#""state":"contradiction""#),
        "{}",
        got.out
    );
    assert!(
        got.out.contains(r#""adrs":["ADR-0001","ADR-0002"]"#),
        "{}",
        got.out
    );
}

#[test]
fn heads_json_writes_the_object_and_nothing_else() {
    // Bare `heads` ignored --json and printed the markdown table, against
    // section 12's "--json writes the result object to stdout and nothing
    // else".
    let r = scratch("heads-json-only");
    write(&r, ".adr.yaml", REGISTRY);
    write(&r, "docs/adr/0001-one.md", NUMBERED);

    let got = run(&r, &["heads", "--json"]);
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    assert!(got.out.starts_with('{'), "{}", got.out);
    assert!(!got.out.contains("# Architecture"), "{}", got.out);
    assert!(got.out.contains(r#""choice":"One""#), "{}", got.out);
    assert!(got.err.is_empty(), "stderr: {}", got.err);
}

// --- history's failure paths ------------------------------------------------

#[test]
fn history_json_answers_an_unknown_key_instead_of_staying_silent() {
    // Both rejections printed to stderr and returned 7 with nothing on stdout,
    // so `--json` gave a machine caller an exit code and silence — against
    // section 12's "writes the result object to stdout and nothing else".
    let r = scratch("history-unknown");
    write(&r, ".adr.yaml", REGISTRY);
    write(&r, "docs/adr/0001-one.md", NUMBERED);

    let got = run(&r, &["history", "a.d", "--json"]);
    assert_eq!(got.code, 7, "{}{}", got.out, got.err);
    assert!(got.out.contains(r#""state":"unknown""#), "{}", got.out);
    assert!(got.out.contains(r#""unknown":"key""#), "{}", got.out);
    // Advisory, as everywhere else: named, never substituted.
    assert!(got.out.contains(r#""suggestion":"a.b""#), "{}", got.out);

    let scope = run(&r, &["history", "a.b", "--scope", "nope", "--json"]);
    assert_eq!(scope.code, 7, "{}{}", scope.out, scope.err);
    assert!(scope.out.contains(r#""unknown":"scope""#), "{}", scope.out);
}
