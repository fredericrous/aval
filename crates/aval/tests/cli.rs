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

/// A directory that cannot be read is not a directory that is not there. The
/// allowance for a missing `dir` beside `sources` once swallowed a permissions
/// error whole: every numbered record vanished from the graph, whatever they
/// superseded came back as heads, and `resolve` answered from a corpus missing
/// half of itself — exit 0.
#[cfg(unix)]
#[test]
fn an_unreadable_records_directory_is_an_error_not_an_absence() {
    use std::os::unix::fs::PermissionsExt;
    let r = scratch("unreadable-dir");
    write(
        &r,
        ".adr.yaml",
        "dir: docs/adr\nsources: [docs/spec.md]\nscopes: [cloud]\nkeys:\n  a.b:\n  c.d:\n",
    );
    write(&r, "docs/adr/0001-one.md", NUMBERED);
    write(&r, "docs/spec.md", LISTED);
    let before = run(&r, &["resolve", "a.b"]);
    assert_eq!(before.code, 0, "{}", before.err);

    let dir = r.join("docs/adr");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o000)).unwrap();
    // Root reads anything; the property under test is unobservable there.
    let unreadable = fs::read_dir(&dir).is_err();
    let got = run(&r, &["resolve", "a.b"]);
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    if !unreadable {
        return;
    }
    assert_eq!(got.code, 3, "out={} err={}", got.out, got.err);
    assert!(got.err.contains("docs/adr"), "{}", got.err);
    assert!(!got.out.contains("undecided"), "{}", got.out);
}

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

#[test]
fn a_value_that_would_read_differently_to_a_model_is_refused() {
    // This text is printed into an agent's context by the session-start hook
    // and by `aval mcp`. A bidi override or a control character makes what a
    // reviewer reads in the pull request differ from what the model receives,
    // and the pull request is the gate section 2.3 relies on.
    let r = scratch("printable-values");
    write(&r, ".adr.yaml", REGISTRY);
    write(
        &r,
        "docs/adr/0001-one.md",
        "---\nid: ADR-0001\nstatus: accepted\ndecisions:\n  \
         - key: a.b\n    choice: \"One\u{202E}ereher\"\n    first: true\n---\n# one\n",
    );
    let got = run(&r, &["check"]);
    assert_eq!(got.code, 3, "{}{}", got.out, got.err);
    assert!(got.err.contains("U+202E"), "{}", got.err);

    // Ordinary text with punctuation, accents and symbols stays valid.
    write(
        &r,
        "docs/adr/0001-one.md",
        "---\nid: ADR-0001\nstatus: accepted\ndecisions:\n  \
         - key: a.b\n    choice: \"Ceph RGW — S3, façade «x» 100%\"\n    first: true\n---\n# one\n",
    );
    assert_eq!(run(&r, &["heads", "--write"]).code, 0);
    let ok = run(&r, &["check"]);
    assert_eq!(ok.code, 0, "{}{}", ok.out, ok.err);
}

#[test]
fn keys_names_omits_where_each_is_decided() {
    let r = scratch("keys-names");
    write(&r, ".adr.yaml", REGISTRY);
    write(&r, "docs/adr/0001-one.md", NUMBERED);

    let full = run(&r, &["keys", "--json"]);
    assert!(full.out.contains(r#""decided""#), "{}", full.out);
    let names = run(&r, &["keys", "--names", "--json"]);
    assert_eq!(names.code, 0, "{}{}", names.out, names.err);
    assert!(!names.out.contains(r#""decided""#), "{}", names.out);
    assert!(names.out.contains(r#""key":"a.b""#), "{}", names.out);
    assert!(names.out.len() < full.out.len());
}

// --- repos and --all-repos --------------------------------------------------
//
// Workspace fixtures live under the system temp dir, not target/: this
// repository is a corpus, and a registry-less directory beneath it would walk
// up and find aval's own `.adr.yaml`.

fn ws_scratch(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join("aval-cli-tests").join(name);
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn corpus_at(root: &Path, choice: &str) {
    write(
        root,
        ".adr.yaml",
        "dir: docs/adr\nscopes: [cloud]\nkeys:\n  a.b:\n",
    );
    write(
        root,
        "docs/adr/0001-one.md",
        &format!(
            "---\nid: ADR-0001\nstatus: accepted\ndecisions:\n  \
             - key: a.b\n    choice: {}\n    first: true\n---\n# one\n",
            choice
        ),
    );
}

fn workspace(name: &str) -> PathBuf {
    let ws = ws_scratch(name);
    corpus_at(&ws.join("alpha"), "Alpha");
    corpus_at(&ws.join("beta"), "Beta");
    ws
}

fn git(dir: &Path, args: &[&str]) {
    let o = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn git");
    assert!(
        o.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&o.stderr)
    );
}

/// A fixture repository with the machine's own git configuration kept out.
fn git_init(dir: &Path) {
    git(
        dir,
        &["-c", "init.templateDir=", "init", "-q", "-b", "main"],
    );
    git(dir, &["config", "core.hooksPath", "/nonexistent"]);
    git(dir, &["config", "user.email", "t@example.com"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
}

#[test]
fn repos_json_lists_name_root_worktree_and_shadowed() {
    let ws = workspace("repos-json");
    corpus_at(&ws.join("alpha").join("sub"), "Sub");
    let gamma = ws.join("gamma");
    corpus_at(&gamma, "Gamma");
    write(
        &gamma,
        ".git",
        &format!("gitdir: {}/alpha/.git/worktrees/gamma\n", ws.display()),
    );

    let got = run(&ws, &["repos", "--json"]);
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    assert!(got.out.contains(r#""mode":"workspace""#), "{}", got.out);
    let canon = fs::canonicalize(ws.join("alpha"))
        .unwrap()
        .display()
        .to_string();
    assert!(
        got.out.contains(&format!(r#""root":"{}""#, canon)),
        "{}",
        got.out
    );
    assert!(got.out.contains(r#""name":"gamma""#), "{}", got.out);
    assert!(got.out.contains(r#""parent":"alpha""#), "{}", got.out);
    // Siblings are not the corpus's business; its own children are.
    let inside = run(&ws.join("alpha"), &["repos", "--json"]);
    assert!(inside.out.contains(r#""mode":"corpus""#), "{}", inside.out);
    assert!(
        inside.out.contains(r#""name":"sub","root""#),
        "{}",
        inside.out
    );
    assert!(inside.out.contains(r#""shadowed":true"#), "{}", inside.out);
    assert!(!inside.out.contains(r#""name":"beta""#), "{}", inside.out);

    let text = run(&ws, &["repos"]);
    assert!(
        text.out.starts_with("workspace: 3 repositories"),
        "{}",
        text.out
    );
    assert!(text.out.contains("worktree of alpha"), "{}", text.out);
}

#[cfg(unix)]
#[test]
fn repos_is_sorted_and_deduped() {
    let ws = ws_scratch("repos-order");
    // Created out of order; listed in order, whatever `read_dir` says.
    for n in ["zeta", "alpha", "mid"] {
        corpus_at(&ws.join(n), n);
    }
    std::os::unix::fs::symlink(ws.join("mid"), ws.join("mid-again")).unwrap();
    let got = run(&ws, &["repos", "--json"]);
    let a = got.out.find(r#""name":"alpha""#).unwrap();
    let m = got.out.find(r#""name":"mid""#).unwrap();
    let z = got.out.find(r#""name":"zeta""#).unwrap();
    assert!(a < m && m < z, "{}", got.out);
    assert!(
        got.out
            .contains(r#""name":"mid-again","reason":"the same directory as `mid`""#),
        "{}",
        got.out
    );
}

#[test]
fn all_repos_exit_is_a_report_not_a_verdict() {
    // 0: every member loaded, none contradicts — `unknown` members included,
    // since a key one repository never declared is an answer there.
    let ws = workspace("exit-clean");
    write(
        &ws.join("beta"),
        ".adr.yaml",
        "dir: docs/adr\nscopes: [cloud]\nkeys:\n  c.d:\n",
    );
    write(&ws.join("beta"), "docs/adr/0001-one.md", "---\nid: ADR-0001\nstatus: accepted\ndecisions:\n  - key: c.d\n    choice: X\n    first: true\n---\n# one\n");
    let got = run(&ws, &["resolve", "a.b", "--all-repos", "--json"]);
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    assert!(got.out.contains(r#""beta":{"exit":7"#), "{}", got.out);

    // 1: a member contradicts.
    let ws = workspace("exit-contradiction");
    write(&ws.join("beta"), "docs/adr/0002-two.md", "---\nid: ADR-0002\nstatus: accepted\ndecisions:\n  - key: a.b\n    choice: Other\n    first: true\n---\n# two\n");
    let got = run(&ws, &["resolve", "a.b", "--all-repos", "--json"]);
    assert_eq!(got.code, 1, "{}{}", got.out, got.err);
    assert!(
        got.out.contains(r#""state":"contradiction""#),
        "{}",
        got.out
    );

    // 1: a member would not load — its error object stands in its place.
    let ws = workspace("exit-unloadable");
    write(
        &ws.join("beta"),
        ".adr.yaml",
        "dir: docs/adr\nscopes: [\nkeys:\n  a.b:\n",
    );
    let got = run(&ws, &["resolve", "a.b", "--all-repos", "--json"]);
    assert_eq!(got.code, 1, "{}{}", got.out, got.err);
    assert!(got.out.contains(r#""beta":{"error":"#), "{}", got.out);
    assert!(
        got.out.contains(r#""alpha":{"adr":"ADR-0001""#),
        "{}",
        got.out
    );

    // 3: no member loaded.
    let ws = workspace("exit-none");
    for n in ["alpha", "beta"] {
        write(
            &ws.join(n),
            ".adr.yaml",
            "dir: docs/adr\nscopes: [\nkeys:\n  a.b:\n",
        );
    }
    assert_eq!(
        run(&ws, &["resolve", "a.b", "--all-repos", "--json"]).code,
        3
    );

    // 2: a report has no file to write or check, and one record in one corpus
    // is not a fleet-wide question.
    let ws = workspace("exit-usage");
    assert_eq!(run(&ws, &["heads", "--all-repos", "--write"]).code, 2);
    assert_eq!(run(&ws, &["heads", "--all-repos", "--check"]).code, 2);
    assert_eq!(run(&ws, &["show", "ADR-0001", "--all-repos"]).code, 2);

    // Inside a single corpus the shape is the same: a one-member map, so a
    // script never branches on how many there were.
    let got = run(
        &ws.join("alpha"),
        &["resolve", "a.b", "--all-repos", "--json"],
    );
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    assert!(got.out.starts_with(r#"{"repos":{"alpha":{"#), "{}", got.out);

    // Text form: a heading per member, a member's message where it failed.
    let ws = workspace("exit-text");
    write(
        &ws.join("beta"),
        ".adr.yaml",
        "dir: docs/adr\nscopes: [\nkeys:\n  a.b:\n",
    );
    let got = run(&ws, &["resolve", "a.b", "--all-repos"]);
    assert!(
        got.out.contains("== alpha ==\nactive   ADR-0001   Alpha\n"),
        "{}",
        got.out
    );
    assert!(
        got.out
            .contains("== beta ==\naval: the corpus is structurally invalid"),
        "{}",
        got.out
    );
}

#[test]
fn a_relative_dash_c_reports_committed_provenance() {
    // Verified broken before this change: `aval -C relative/path resolve
    // <contradicted key> --json` reported every head `unavailable` while the
    // same path spelled absolutely reported `committed`. The pathspec handed
    // to `git -C root blame` was relative to the wrong directory.
    let parent = scratch("provenance-parent");
    let r = parent.join("repo");
    fs::create_dir_all(&r).unwrap();
    git_init(&r);
    write(
        &r,
        ".adr.yaml",
        "dir: docs/adr\nscopes: []\nkeys:\n  a.b:\n",
    );
    write(&r, "docs/adr/0001-one.md", "---\nid: ADR-0001\nstatus: accepted\ndecisions:\n  - key: a.b\n    choice: One\n    first: true\n---\n# one\n");
    write(&r, "docs/adr/0002-two.md", "---\nid: ADR-0002\nstatus: accepted\ndecisions:\n  - key: a.b\n    choice: Two\n    first: true\n---\n# two\n");
    git(&r, &["add", "-A"]);
    git(&r, &["commit", "-q", "-m", "two heads"]);
    fs::create_dir_all(parent.join("sibling")).unwrap();

    let absolute = run(
        &parent,
        &["-C", r.to_str().unwrap(), "resolve", "a.b", "--json"],
    );
    let relative = run(&parent, &["-C", "repo", "resolve", "a.b", "--json"]);
    let dotdot = run(
        &parent.join("sibling"),
        &["-C", "../repo", "resolve", "a.b", "--json"],
    );
    for (label, got) in [
        ("absolute", &absolute),
        ("relative", &relative),
        ("../", &dotdot),
    ] {
        assert_eq!(got.code, 5, "{}: {}{}", label, got.out, got.err);
        assert!(
            got.out.contains(r#""state":"committed""#),
            "{}: {}",
            label,
            got.out
        );
        assert!(!got.out.contains("unavailable"), "{}: {}", label, got.out);
    }
    assert_eq!(absolute.out, relative.out);
    assert_eq!(absolute.out, dotdot.out);

    // And through the server, started with the relative path.
    let o = std::process::Command::new(bin())
        .args(["-C", "repo", "mcp"])
        .current_dir(&parent)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write as _;
            c.stdin.take().unwrap().write_all(
                br#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"aval_resolve","arguments":{"key":"a.b"}}}
"#,
            )?;
            c.wait_with_output()
        })
        .expect("mcp");
    let reply = String::from_utf8_lossy(&o.stdout);
    let inner = reply
        .split(r#""text":""#)
        .nth(1)
        .map(|s| s.replace(r#"\""#, "\""))
        .unwrap_or_default();
    assert!(inner.starts_with(absolute.out.trim()), "mcp: {}", reply);
}

/// A reader that stops early must not be answered with a panic. The output
/// has to outgrow the pipe buffer, or the write lands before the reader
/// leaves and nothing is observed; three thousand keys do it.
#[cfg(unix)]
#[test]
fn a_closed_stdout_ends_the_process_quietly() {
    use std::io::Read;
    use std::process::Stdio;
    let r = scratch("closed-stdout");
    let mut registry = String::from("dir: docs/adr\nscopes: [cloud]\nkeys:\n");
    for i in 0..3000 {
        registry.push_str(&format!("  k.n{}:\n    description: key number {}\n", i, i));
    }
    write(&r, ".adr.yaml", &registry);
    let mut child = Command::new(bin())
        .args(["keys"])
        .current_dir(&r)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn aval");
    // Take the reader and drop it without reading: the pipe closes with the
    // writer still holding most of its output.
    drop(child.stdout.take());
    let mut err = String::new();
    child
        .stderr
        .take()
        .expect("stderr")
        .read_to_string(&mut err)
        .expect("read stderr");
    let status = child.wait().expect("wait");
    assert!(!err.contains("panicked"), "{}", err);
    assert!(!err.contains("Broken pipe"), "{}", err);
    // Ended by the signal, not by a successful exit it did not have.
    assert!(!status.success(), "{:?}", status);
}

// --- aval relevant ---------------------------------------------------------
//
// Retrieval at the input edge. Every test here is about the wall between it
// and resolution: what it exits, what it says about itself, and that it never
// invents an answer for a key nobody decided.

/// A corpus with one decided key, one undecided key, and a record whose body
/// names a directory — so all three path-free and path-bearing signals have
/// something to bite on.
fn ranked(name: &str) -> PathBuf {
    let r = scratch(name);
    write(
        &r,
        ".adr.yaml",
        "dir: docs/adr\nscopes: [cloud]\nkeys:\n  storage.object-store:\n    \
         description: Canonical S3-compatible object store\n  api.gateway:\n",
    );
    write(
        &r,
        "docs/adr/0001-object-store.md",
        "---\nid: ADR-0001\nstatus: accepted\ndecisions:\n  \
         - key: storage.object-store\n    choice: Ceph RGW\n    first: true\n\
         ---\n# 0001 — Ceph RGW for object storage\n\nIt serves \
         `kubernetes/data-storage/ceph`, and nothing else.\n",
    );
    r
}

#[test]
fn relevant_ranks_a_key_and_exits_zero() {
    let r = ranked("relevant-text");
    let got = run(&r, &["relevant", "--text", "where do objects get stored"]);
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    assert!(got.out.contains("storage.object-store"), "{}", got.out);
    assert!(got.out.contains("active"), "{}", got.out);
    // The obligation is on every rendering, not only the JSON one.
    assert!(got.out.contains("aval resolve <key>"), "{}", got.out);
}

#[test]
fn relevant_exits_zero_even_when_nothing_matches() {
    // A thin answer is not a failure. Exit 0 is what keeps "I ranked and found
    // little" out of the range that means "I could not look".
    let r = ranked("relevant-nothing");
    let got = run(&r, &["relevant", "--text", "quarterly expiry reversal"]);
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    assert!(
        got.out.contains("nothing in this corpus matched"),
        "{}",
        got.out
    );
}

#[test]
fn relevant_needs_something_to_rank_against() {
    let r = ranked("relevant-usage");
    assert_eq!(run(&r, &["relevant"]).code, 2);
    assert_eq!(run(&r, &["relevant", "--text", "  "]).code, 2);
    // A positional would be mistaken for a key, which this command does not take.
    assert_eq!(run(&r, &["relevant", "storage.object-store"]).code, 2);
    assert_eq!(run(&r, &["relevant", "--text", "x", "--top", "0"]).code, 2);
    assert_eq!(run(&r, &["relevant", "--text", "x", "--all-repos"]).code, 2);
}

#[test]
fn an_undeclared_scope_is_a_usage_error_not_a_thin_ranking() {
    let r = ranked("relevant-scope");
    let got = run(&r, &["relevant", "--text", "storage", "--scope", "clodu"]);
    assert_eq!(got.code, 2, "{}{}", got.out, got.err);
    assert!(got.err.contains("did you mean `cloud`"), "{}", got.err);
}

#[test]
fn a_path_a_record_names_outranks_a_word_it_shares() {
    let r = ranked("relevant-mention");
    let got = run(
        &r,
        &[
            "relevant",
            "--path",
            "kubernetes/data-storage/ceph/values.yaml",
            "--json",
        ],
    );
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    let j = aval_core::json::parse(&got.out).expect("json");
    let keys = j.get("keys").and_then(|k| k.as_arr()).expect("keys");
    assert_eq!(
        keys[0].get("key").and_then(|k| k.as_str()),
        Some("storage.object-store"),
        "{}",
        got.out
    );
    let mentions = keys[0]
        .get("why")
        .and_then(|w| w.get("mentions"))
        .and_then(|m| m.as_arr())
        .expect("why.mentions");
    assert_eq!(mentions.len(), 1, "{}", got.out);
}

#[test]
fn the_json_carries_a_dependency_row_per_ranked_key() {
    let r = ranked("relevant-deps");
    let got = run(&r, &["relevant", "--text", "object gateway", "--json"]);
    let j = aval_core::json::parse(&got.out).expect("json");
    assert_eq!(j.get("kind").and_then(|k| k.as_str()), Some("suggestion"));
    let keys = j.get("keys").and_then(|k| k.as_arr()).expect("keys").len();
    let deps = j
        .get("dependencies")
        .and_then(|d| d.as_arr())
        .expect("dependencies");
    assert_eq!(deps.len(), keys, "one row per ranked key: {}", got.out);
    // A router reads `unresolved` and nothing else to know it must stop.
    assert!(
        deps.iter()
            .any(|d| d.get("unresolved") == Some(&aval_core::json::Json::Bool(true))),
        "{}",
        got.out
    );
}

#[test]
fn the_same_question_gives_the_same_order_twice() {
    // SEMANTICS section 12: the ranking is a pure function of the corpus and
    // the query. Two runs over an untouched tree must not differ at all.
    let r = ranked("relevant-deterministic");
    let args = ["relevant", "--text", "object storage gateway", "--json"];
    assert_eq!(run(&r, &args).out, run(&r, &args).out);
}

#[test]
fn relevant_reads_the_working_tree_when_asked() {
    let r = ranked("relevant-changed");
    // Not a git repository: `--changed` finds nothing and says so by ranking
    // from nothing rather than by failing.
    let got = run(&r, &["relevant", "--changed", "--text", "object"]);
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    assert!(got.out.contains("storage.object-store"), "{}", got.out);
}
