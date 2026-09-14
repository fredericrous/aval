//! `aval hook install`, run as the binary.
//!
//! The cases here are the ones that cost something when they go wrong: a
//! settings file clobbered, a second copy of the same hook appended on every
//! run, or a hook that fails a session because a tool is missing.

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

/// A corpus with one record, so `aval heads` has something to print.
fn scratch(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/hook-tests")
        .join(name);
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(p.join("docs/adr")).expect("mkdir");
    fs::write(
        p.join(".adr.yaml"),
        "dir: docs/adr\nscopes: []\nkeys:\n  a.b:\n",
    )
    .unwrap();
    fs::write(
        p.join("docs/adr/0001-a.md"),
        "---\nid: ADR-0001\nstatus: accepted\ndecisions:\n  \
         - key: a.b\n    choice: One\n    first: true\n---\n# one\n",
    )
    .unwrap();
    assert_eq!(run(&p, &["heads", "--write"]).code, 0);
    p
}

fn settings(root: &Path) -> String {
    fs::read_to_string(root.join(".claude/settings.json")).expect("settings")
}

#[test]
fn install_writes_the_script_and_wires_the_hook() {
    let r = scratch("fresh");
    let got = run(&r, &["hook", "install"]);
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);

    let script = fs::read_to_string(r.join(".claude/hooks/aval-heads.sh")).expect("script");
    assert!(script.starts_with("#!/bin/sh"), "{}", script);
    assert!(settings(&r).contains("sh .claude/hooks/aval-heads.sh"));
}

#[test]
fn installing_twice_changes_nothing() {
    let r = scratch("idempotent");
    assert_eq!(run(&r, &["hook", "install"]).code, 0);
    let before = settings(&r);

    let got = run(&r, &["hook", "install"]);
    assert_eq!(got.code, 0);
    assert!(got.out.contains("already up to date"), "{}", got.out);
    assert_eq!(
        settings(&r),
        before,
        "the second run must not rewrite bytes"
    );
    assert_eq!(run(&r, &["hook", "install", "--check"]).code, 0);
}

#[test]
fn check_reports_an_unwired_repository_and_writes_nothing() {
    let r = scratch("check-unwired");
    let got = run(&r, &["hook", "install", "--check"]);
    assert_eq!(got.code, 1, "{}{}", got.out, got.err);
    assert!(got.out.contains("out of date"), "{}", got.out);
    assert!(
        !r.join(".claude/hooks/aval-heads.sh").exists(),
        "--check must not install"
    );
}

#[test]
fn a_hand_edited_script_is_stale_and_then_restored() {
    let r = scratch("drift");
    assert_eq!(run(&r, &["hook", "install"]).code, 0);
    let p = r.join(".claude/hooks/aval-heads.sh");
    let original = fs::read_to_string(&p).unwrap();
    fs::write(&p, "#!/bin/sh\necho tampered\n").unwrap();

    assert_eq!(run(&r, &["hook", "install", "--check"]).code, 1);
    assert_eq!(run(&r, &["hook", "install"]).code, 0);
    assert_eq!(fs::read_to_string(&p).unwrap(), original);
}

/// The expensive mistake: rewriting a settings file and losing what else it
/// said, or dropping another tool's hook.
#[test]
fn unrelated_settings_and_another_tools_hook_survive() {
    let r = scratch("preserve");
    fs::create_dir_all(r.join(".claude")).unwrap();
    fs::write(
        r.join(".claude/settings.json"),
        r#"{
  "permissions": { "allow": ["Bash(pnpm test)"] },
  "hooks": {
    "SessionStart": [
      { "hooks": [{ "type": "command", "command": "sh .claude/hooks/duro-catalog.sh" }] }
    ]
  }
}
"#,
    )
    .unwrap();

    assert_eq!(run(&r, &["hook", "install"]).code, 0);
    let s = settings(&r);
    assert!(s.contains("Bash(pnpm test)"), "unrelated key lost: {}", s);
    assert!(s.contains("duro-catalog.sh"), "other hook lost: {}", s);
    assert!(s.contains("aval-heads.sh"), "ours missing: {}", s);
}

/// A variant spelling must be corrected, not joined by a second entry that
/// does the same thing on every session start.
#[test]
fn a_variant_command_is_rewritten_rather_than_duplicated() {
    let r = scratch("migrate");
    fs::create_dir_all(r.join(".claude")).unwrap();
    fs::write(
        r.join(".claude/settings.json"),
        r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"bash ./.claude/hooks/aval-heads.sh"}]}]}}"#,
    )
    .unwrap();

    assert_eq!(run(&r, &["hook", "install"]).code, 0);
    let s = settings(&r);
    assert_eq!(
        s.matches("aval-heads.sh").count(),
        1,
        "one entry, not two: {}",
        s
    );
    assert!(s.contains("sh .claude/hooks/aval-heads.sh"), "{}", s);
}

/// A settings file this cannot read is one somebody wrote. Stop; do not
/// replace it with one the tool invented.
#[test]
fn an_unparseable_settings_file_stops_the_install_untouched() {
    let r = scratch("bad-json");
    fs::create_dir_all(r.join(".claude")).unwrap();
    let original = "{ not json";
    fs::write(r.join(".claude/settings.json"), original).unwrap();

    let got = run(&r, &["hook", "install"]);
    assert_eq!(got.code, 3, "{}{}", got.out, got.err);
    assert!(got.err.contains("not valid JSON"), "{}", got.err);
    assert_eq!(settings(&r), original, "the file must be untouched");
    assert!(
        !r.join(".claude/hooks/aval-heads.sh").exists(),
        "nothing should have been written"
    );
}

// --- what the generated script does ---------------------------------------

/// `/bin/sh` by absolute path, so a test can empty `PATH` to hide `aval`
/// without also hiding the shell.
fn sh(dir: &Path, path_override: Option<&str>) -> Run {
    let mut c = Command::new("/bin/sh");
    c.arg(".claude/hooks/aval-heads.sh").current_dir(dir);
    if let Some(p) = path_override {
        c.env("PATH", p);
    }
    let o = c.output().expect("spawn sh");
    Run {
        code: o.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&o.stdout).to_string(),
        err: String::from_utf8_lossy(&o.stderr).to_string(),
    }
}

#[test]
fn the_hook_prints_the_heads_and_the_preamble() {
    let r = scratch("runs");
    assert_eq!(run(&r, &["hook", "install"]).code, 0);

    let dir = Path::new(bin()).parent().unwrap().display().to_string();
    let got = sh(
        &r,
        Some(&format!("{}:{}", dir, std::env::var("PATH").unwrap())),
    );
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    assert!(got.out.contains("ARCHITECTURE DECISIONS"), "{}", got.out);
    assert!(got.out.contains("| a.b |"), "{}", got.out);
    assert!(got.out.contains("aval resolve"), "{}", got.out);
    // The paragraph that exists because of a real mistake: two live
    // implementations read as two decisions when one had replaced the other.
    assert!(
        got.out.contains("What is deployed is not what was decided"),
        "{}",
        got.out
    );
}

/// The rules block: printed under the heads when the corpus has constraints,
/// absent when it has none, and carrying the precedence a caller needs to read
/// a rule against a decision.
#[test]
fn the_hook_prints_the_constraints_and_counts_the_heuristics() {
    let r = scratch("rules");
    fs::write(
        r.join(".adr.yaml"),
        "dir: docs/adr\nrules:\n  - docs/p/book.md\nscopes: []\nkeys:\n  a.b:\n",
    )
    .unwrap();
    fs::create_dir_all(r.join("docs/p")).unwrap();
    fs::write(
        r.join("docs/p/book.md"),
        "---\nadopts: ADR-0001\n---\n\
         ## names.reveal-intent [constraint]\n\nNames reveal intention.\n\n\
         ## functions.few-arguments [heuristic]\n\nFew arguments.\n",
    )
    .unwrap();
    assert_eq!(run(&r, &["hook", "install"]).code, 0);

    let dir = Path::new(bin()).parent().unwrap().display().to_string();
    let got = sh(
        &r,
        Some(&format!("{}:{}", dir, std::env::var("PATH").unwrap())),
    );
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    assert!(
        got.out.contains("RULES — each adopted by a decision"),
        "{}",
        got.out
    );
    assert!(
        got.out.contains("does not outrank a rule here"),
        "{}",
        got.out
    );
    assert!(got.out.contains("names.reveal-intent"), "{}", got.out);
    // The heuristic is counted, not printed: it is fetched on demand.
    assert!(!got.out.contains("Few arguments."), "{}", got.out);
    assert!(
        got.out.contains("(1 heuristics beside these)"),
        "{}",
        got.out
    );
    // The heads still come first, and the preamble is untouched.
    assert!(
        got.out.find("ARCHITECTURE DECISIONS") < got.out.find("RULES —"),
        "{}",
        got.out
    );
}

/// A corpus with no rules prints exactly what it printed before rules existed.
#[test]
fn the_hook_says_nothing_about_rules_when_there_are_none() {
    let r = scratch("no-rules");
    assert_eq!(run(&r, &["hook", "install"]).code, 0);
    let dir = Path::new(bin()).parent().unwrap().display().to_string();
    let got = sh(
        &r,
        Some(&format!("{}:{}", dir, std::env::var("PATH").unwrap())),
    );
    assert_eq!(got.code, 0, "{}{}", got.out, got.err);
    assert!(got.out.contains("| a.b |"), "{}", got.out);
    assert!(!got.out.contains("RULES"), "{}", got.out);
}

/// The script's bytes are its version. A consumer holding the 1.1.0 script is
/// told it is stale, which is the only mechanism there is for shipping a change
/// to it.
#[test]
fn the_1_1_0_script_reads_as_stale() {
    let r = scratch("stale-script");
    assert_eq!(run(&r, &["hook", "install"]).code, 0);

    let current = aval::hook::SCRIPT;
    let start = current
        .find("# The rules the decisions")
        .expect("the rules block");
    let end = current
        .find("if [ -s \"$notes\" ]")
        .expect("the notes block");
    let previous = format!("{}{}", &current[..start], &current[end..]);
    assert_ne!(
        previous, current,
        "the fixture must differ, or this asserts nothing"
    );
    fs::write(r.join(".claude/hooks/aval-heads.sh"), &previous).unwrap();

    let got = run(&r, &["hook", "install", "--check"]);
    assert_eq!(got.code, 1, "{}{}", got.out, got.err);
    assert!(got.out.contains("stale"), "{}", got.out);
    assert!(got.out.contains("out of date"), "{}", got.out);

    assert_eq!(run(&r, &["hook", "install"]).code, 0);
    assert_eq!(run(&r, &["hook", "install", "--check"]).code, 0);
}

/// The property that makes this safe to commit: a session must not fail, or
/// even complain, because the person who started it has not installed aval.
#[test]
fn the_hook_is_silent_when_aval_is_absent() {
    let r = scratch("no-binary");
    assert_eq!(run(&r, &["hook", "install"]).code, 0);

    // A PATH with no aval on it. Empty rather than bogus: the machine running
    // these tests has aval installed, so inheriting PATH would not test this.
    let empty = r.join("empty-path");
    fs::create_dir_all(&empty).unwrap();
    let got = sh(&r, Some(&empty.display().to_string()));
    assert_eq!(got.code, 0, "must not fail a session");
    assert_eq!(got.out, "", "must say nothing: {}", got.out);
}

/// Installed in a repository that has no corpus, or whose corpus does not
/// resolve, the hook says nothing rather than guessing or complaining.
/// Reporting a broken corpus is the gate's job.
#[test]
fn the_hook_is_silent_without_a_corpus() {
    let r = scratch("no-corpus");
    assert_eq!(run(&r, &["hook", "install"]).code, 0);
    fs::remove_file(r.join(".adr.yaml")).unwrap();

    let dir = Path::new(bin()).parent().unwrap().display().to_string();
    let got = sh(
        &r,
        Some(&format!("{}:{}", dir, std::env::var("PATH").unwrap())),
    );
    assert_eq!(got.code, 0);
    assert_eq!(got.out, "", "{}", got.out);
}

#[test]
fn the_notes_file_is_appended_and_never_written_by_install() {
    let r = scratch("notes");
    assert_eq!(run(&r, &["hook", "install"]).code, 0);
    assert!(
        !r.join(".claude/aval-hook.local.md").exists(),
        "install must not create it"
    );

    let note = "Local caveat: the cloud scope is not wired up here yet.\n";
    fs::write(r.join(".claude/aval-hook.local.md"), note).unwrap();
    assert_eq!(run(&r, &["hook", "install"]).code, 0);
    assert_eq!(
        fs::read_to_string(r.join(".claude/aval-hook.local.md")).unwrap(),
        note,
        "regeneration must leave it alone"
    );

    let dir = Path::new(bin()).parent().unwrap().display().to_string();
    let got = sh(
        &r,
        Some(&format!("{}:{}", dir, std::env::var("PATH").unwrap())),
    );
    assert!(got.out.contains("Local caveat"), "{}", got.out);
}

/// A repository that excludes `.claude` drops the files silently, and the hook
/// then works for whoever ran the command and for nobody else.
#[test]
fn an_ignored_claude_directory_is_reported() {
    let r = scratch("gitignored");
    assert!(Command::new("git")
        .args(["init", "-q"])
        .current_dir(&r)
        .status()
        .expect("git init")
        .success());
    fs::write(r.join(".gitignore"), ".claude/\n").unwrap();

    let got = run(&r, &["hook", "install"]);
    assert_eq!(got.code, 0, "the warning is advisory, not a failure");
    assert!(got.out.contains("ship to nobody"), "{}", got.out);
    assert!(
        got.out.contains("!.claude/hooks/aval-heads.sh"),
        "{}",
        got.out
    );
}
