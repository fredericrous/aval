//! Traits and areas, from the outside (SEMANTICS section 2.5): the filter on
//! `aval rules`, the checks, the trip through a pack, and `aval traits` in its
//! four modes.
//!
//! Two properties matter more than any single detector. A corpus without
//! `areas` cannot tell 1.7.0 from 1.6.0. And "could not look" never shares an
//! exit code with "found nothing".

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

fn run_env(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Run {
    let mut c = Command::new(bin());
    c.args(args).current_dir(dir);
    for (k, v) in env {
        c.env(k, v);
    }
    let o = c.output().expect("spawn aval");
    Run {
        code: o.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&o.stdout).to_string(),
        err: String::from_utf8_lossy(&o.stderr).to_string(),
    }
}

fn run(dir: &Path, args: &[&str]) -> Run {
    run_env(dir, args, &[])
}

fn scratch(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/traits-tests")
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

fn git(dir: &Path, args: &[&str]) {
    let o = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git");
    assert!(
        o.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&o.stderr)
    );
}

/// `git init` and track everything written so far.
fn track(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["add", "-A"]);
}

const ADR: &str = "---\nid: ADR-0001\nstatus: accepted\ndecisions:\n  \
                   - key: a.b\n    choice: One\n    first: true\n---\n# one\n";

const CLI_RULES: &str = "---\nadopts: ADR-0001\napplies: [cli]\n---\n# CLI\n\n\
                         ## cli.exit [constraint]\n\nUsage errors exit 2.\n\n\
                         ## cli.help [heuristic]\n\nHelp leads with examples.\n";

const UI_RULES: &str = "---\nadopts: ADR-0001\napplies: [ui]\n---\n# UI\n\n\
                        ## ui.answer [constraint]\n\nEvery action is answered.\n";

const ANY_RULES: &str = "---\nadopts: ADR-0001\n---\n# Any\n\n\
                         ## any.names [constraint]\n\nNames reveal intention.\n";

/// One record, three rule files, and whatever registry tail a test adds.
fn corpus(name: &str, tail: &str) -> PathBuf {
    let r = invalid_corpus(name, tail);
    let h = run(&r, &["heads", "--write"]);
    assert_eq!(h.code, 0, "{}{}", h.out, h.err);
    r
}

/// The same, for a registry the test expects to be refused.
fn invalid_corpus(name: &str, tail: &str) -> PathBuf {
    let r = scratch(name);
    write(
        &r,
        ".adr.yaml",
        &format!(
            "dir: docs/adr\ntraits: [cli, ui]\nrules:\n  - docs/p/cli.md\n  - docs/p/ui.md\n  \
             - docs/p/any.md\nscopes: [cloud]\nkeys:\n  a.b:\n{}",
            tail
        ),
    );
    write(&r, "docs/adr/0001-a.md", ADR);
    write(&r, "docs/p/cli.md", CLI_RULES);
    write(&r, "docs/p/ui.md", UI_RULES);
    write(&r, "docs/p/any.md", ANY_RULES);
    r
}

// --- the filter --------------------------------------------------------------

#[test]
fn without_areas_every_rule_is_listed_and_nothing_is_said() {
    let r = corpus("no-areas", "");
    let got = run(&r, &["rules"]);
    assert_eq!(got.code, 0);
    for id in ["cli.exit", "cli.help", "ui.answer", "any.names"] {
        assert!(got.out.contains(id), "{}", got.out);
    }
    assert!(got.err.is_empty(), "nothing to report: {}", got.err);
    let j = run(&r, &["rules", "--json"]);
    assert!(!j.out.contains("\"omitted\""), "{}", j.out);
    assert!(run(&r, &["traits", "--summary"]).out.is_empty());
}

#[test]
fn areas_hide_rules_about_other_traits_and_say_so_on_stderr() {
    let r = corpus("ui-only", "areas:\n  \"web/**\": [ui]\n");
    let got = run(&r, &["rules", "--level", "constraint"]);
    assert_eq!(got.code, 0);
    assert!(
        got.out.contains("ui.answer") && got.out.contains("any.names"),
        "{}",
        got.out
    );
    assert!(!got.out.contains("cli.exit"), "{}", got.out);
    // stdout is rule lines only: the hook counts them.
    assert_eq!(got.out.lines().count(), 2, "{}", got.out);
    assert!(
        got.err
            .contains("hidden by traits (ui): 1 constraint(s), 0 heuristic(s)"),
        "{}",
        got.err
    );

    let all = run(&r, &["rules", "--all-traits"]);
    assert!(
        all.out.contains("cli.exit") && all.out.contains("cli.help"),
        "{}",
        all.out
    );

    // Counted after the other filters: asking for heuristics counts heuristics.
    let h = run(&r, &["rules", "--level", "heuristic", "--json"]);
    assert!(
        h.out
            .contains("\"omitted\":{\"constraints\":0,\"heuristics\":1,\"traits\":[\"ui\"]}"),
        "{}",
        h.out
    );
    // A hidden rule is still active and still explained.
    let one = run(&r, &["rule", "cli.exit"]);
    assert_eq!(one.code, 0, "{}", one.err);
    assert!(one.out.contains("(active)"), "{}", one.out);
}

#[test]
fn the_summary_prints_even_when_nothing_is_hidden() {
    let r = corpus("both", "areas:\n  \"web/**\": [ui]\n  \"cmd/**\": [cli]\n");
    let got = run(&r, &["traits", "--summary"]);
    assert_eq!(
        got.out.trim(),
        "traits here: cli, ui — hidden by traits: 0 constraint(s), 0 heuristic(s) (aval rules --all-traits)"
    );
}

#[test]
fn a_catch_all_empty_area_is_visible_at_session_start() {
    let r = corpus("catch-all", "areas:\n  \"**\": []\n");
    let got = run(&r, &["traits", "--summary"]);
    assert!(
        got.out
            .contains("traits here: none — hidden by traits: 2 constraint(s), 1 heuristic(s)"),
        "{}",
        got.out
    );
}

#[test]
fn the_summary_needs_no_git_at_all() {
    let r = corpus("no-git", "areas:\n  \"web/**\": [ui]\n");
    // No repository, and no git on PATH.
    let got = run_env(&r, &["traits", "--summary"], &[("PATH", "/nonexistent")]);
    assert_eq!(got.code, 0, "{}", got.err);
    assert!(got.out.starts_with("traits here: ui"), "{}", got.out);
}

#[test]
fn applies_json_appears_only_when_targeted() {
    let r = corpus("json", "");
    let j = run(&r, &["rules", "--json"]).out;
    assert!(j.contains("\"applies\":[\"cli\"]"), "{}", j);
    let any = j.split("\"any.names\"").nth(1).unwrap_or("");
    let any_row = any.split('}').next().unwrap_or("");
    assert!(!any_row.contains("applies"), "{}", any_row);
}

// --- the checks ---------------------------------------------------------------

#[test]
fn a_trait_outside_the_vocabulary_is_refused_everywhere() {
    let r = invalid_corpus("undeclared", "areas:\n  \"web/**\": [uii]\n");
    let got = run(&r, &["check"]);
    assert_eq!(got.code, 3, "{}{}", got.out, got.err);
    assert!(
        got.err.contains("areas-declared") && got.err.contains("did you mean `ui`"),
        "{}",
        got.err
    );

    let r = scratch("undeclared-applies");
    write(
        &r,
        ".adr.yaml",
        "dir: docs/adr\nrules:\n  - docs/p/cli.md\nscopes: [cloud]\nkeys:\n  a.b:\n",
    );
    write(&r, "docs/adr/0001-a.md", ADR);
    write(&r, "docs/p/cli.md", CLI_RULES);
    let got = run(&r, &["check"]);
    assert_eq!(got.code, 3);
    assert!(got.err.contains("rule-applies-declared"), "{}", got.err);
}

#[test]
fn malformed_globs_are_areas_parse_errors() {
    for g in ["./web/**", "/web/**", "web/", "web/[ab]", "a/../b", "web**"] {
        let r = invalid_corpus("bad-glob", &format!("areas:\n  \"{}\": [ui]\n", g));
        let got = run(&r, &["check"]);
        assert_eq!(got.code, 3, "{}: {}", g, got.err);
        assert!(got.err.contains("areas-parse"), "{}: {}", g, got.err);
    }
    let r = invalid_corpus("not-a-list", "areas:\n  \"web/**\": ui\n");
    assert!(run(&r, &["check"]).err.contains("areas-parse"));
}

// --- packs --------------------------------------------------------------------

#[test]
fn traits_and_applies_travel_in_the_pack_and_areas_never_do() {
    let r = corpus("producer", "areas:\n  \"web/**\": [ui]\n");
    assert_eq!(run(&r, &["pack", "--write"]).code, 0);
    let pack = fs::read_to_string(r.join("aval.pack")).unwrap();
    assert!(
        pack.contains("traits:\n  - \"cli\"\n  - \"ui\"\n"),
        "{}",
        pack
    );
    assert!(pack.contains("    applies:\n      - \"cli\"\n"), "{}", pack);
    assert!(!pack.contains("areas"), "{}", pack);

    // A consumer reads it: the vocabulary arrives with the pack, so its own
    // areas can name `cli` without declaring it.
    let c = scratch("consumer");
    write(&c, ".adr/packs/fleet.pack", &pack);
    write(
        &c,
        ".adr.yaml",
        "packs:\n  - .adr/packs/fleet.pack\nkeys:\nareas:\n  \"cmd/**\": [cli]\n",
    );
    let got = run(&c, &["rules"]);
    assert_eq!(got.code, 0, "{}", got.err);
    assert!(
        got.out.contains("cli.exit") && !got.out.contains("ui.answer"),
        "{}",
        got.out
    );

    // And a pack carrying areas is refused whole.
    write(
        &c,
        ".adr/packs/fleet.pack",
        &format!("{}areas:\n  \"x/**\": [cli]\n", pack),
    );
    let bad = run(&c, &["check"]);
    assert_eq!(bad.code, 3);
    assert!(bad.err.contains("a pack carries no `areas`"), "{}", bad.err);
}

#[test]
fn a_pack_without_traits_is_what_it_was() {
    let r = scratch("plain-producer");
    write(
        &r,
        ".adr.yaml",
        "dir: docs/adr\nrules:\n  - docs/p/any.md\nscopes: [cloud]\nkeys:\n  a.b:\n",
    );
    write(&r, "docs/adr/0001-a.md", ADR);
    write(&r, "docs/p/any.md", ANY_RULES);
    assert_eq!(run(&r, &["pack", "--write"]).code, 0);
    let pack = fs::read_to_string(r.join("aval.pack")).unwrap();
    assert!(
        !pack.contains("traits") && !pack.contains("applies"),
        "{}",
        pack
    );
}

// --- detection ------------------------------------------------------------------

fn detect(r: &Path) -> Run {
    run(r, &["traits", "--detect", "--json"])
}

fn detected(r: &Path) -> Vec<(String, String)> {
    let out = detect(r).out;
    let j = aval_core::json::parse(&out).unwrap_or_else(|e| panic!("{}: {}", e, out));
    j.get("detected")
        .and_then(|d| d.as_arr())
        .unwrap_or(&[])
        .iter()
        .map(|d| {
            (
                d.get("anchor")
                    .and_then(|x| x.as_str())
                    .unwrap()
                    .to_string(),
                d.get("trait").and_then(|x| x.as_str()).unwrap().to_string(),
            )
        })
        .collect()
}

fn pair(a: &str, t: &str) -> (String, String) {
    (a.to_string(), t.to_string())
}

#[test]
fn a_rust_workspace_root_speaks_for_nothing_and_its_members_do() {
    let r = corpus("rust-ws", "");
    write(&r, "Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
    write(&r, "crates/tool/Cargo.toml", "[package]\nname = \"tool\"\n");
    write(&r, "crates/tool/src/main.rs", "fn main() {}\n");
    write(&r, "crates/lib/Cargo.toml", "[package]\nname = \"lib\"\n");
    write(&r, "crates/lib/src/lib.rs", "\n");
    track(&r);
    assert_eq!(detected(&r), vec![pair("crates/tool/Cargo.toml", "cli")]);
}

#[test]
fn exotic_cargo_shapes_are_scanned_not_failed() {
    let r = corpus("cargo-shapes", "");
    write(
        &r,
        "Cargo.toml",
        "[package]\nname = \"x\"\n\n[target.'cfg(unix)'.dependencies]\n\
         dependencies.axum = \"1\"\ntokio.workspace = true\n\
         serde = { version = \"1\",\n  features = [\"derive\"] }\n\n[[bin]] # the tool\nname = \"x\"\n",
    );
    track(&r);
    let got = detect(&r);
    assert_eq!(got.code, 0, "{}", got.err);
    assert_eq!(detected(&r), vec![pair("Cargo.toml", "cli")]);
}

#[test]
fn an_unrecognised_toml_shape_costs_a_detection_never_an_exit_3() {
    let r = corpus("toml-weird", "");
    write(&r, "pyproject.toml", "this is = = not [ toml\n[[[\n");
    track(&r);
    assert_eq!(detect(&r).code, 0);
    assert_eq!(run(&r, &["traits", "--check"]).code, 0);
}

#[test]
fn python_scripts_in_both_dialects() {
    let r = corpus("python", "");
    write(&r, "a/pyproject.toml", "[project]\ndependencies = [\"requests>=2; python_version>'3'\"]\n[project.scripts]\na = \"a:main\"\n");
    write(
        &r,
        "b/pyproject.toml",
        "[tool.poetry.dependencies]\npython = \"^3.12\"\n[tool.poetry.scripts]\nb = \"b:main\"\n",
    );
    write(
        &r,
        "c/setup.cfg",
        "[options.entry_points]\nconsole_scripts =\n    c = c:main\n",
    );
    track(&r);
    assert_eq!(
        detected(&r),
        vec![
            pair("a/pyproject.toml", "cli"),
            pair("b/pyproject.toml", "cli"),
            pair("c/setup.cfg", "cli")
        ]
    );
}

#[test]
fn a_component_library_is_ui_through_peer_dependencies() {
    let r = corpus("ds", "");
    write(
        &r,
        "package.json",
        "{\"name\":\"ds\",\"peerDependencies\":{\"react\":\"*\"},\"devDependencies\":{\"react\":\"19\"}}",
    );
    track(&r);
    assert_eq!(detected(&r), vec![pair("package.json", "ui")]);
}

#[test]
fn dev_only_frameworks_do_not_count_but_tsx_files_do() {
    let r = corpus("dev-only", "");
    write(
        &r,
        "app/package.json",
        "{\"devDependencies\":{\"react\":\"19\"}}",
    );
    write(&r, "app/src/story.tsx", "export {}\n");
    track(&r);
    // Not through the dev dependency — through the file.
    let got = detect(&r).out;
    assert!(got.contains("app/src/story.tsx"), "{}", got);
    assert_eq!(detected(&r), vec![pair("app/package.json", "ui")]);
}

#[test]
fn a_node_workspace_root_with_its_own_dependencies_speaks() {
    let r = corpus("expo-root", "");
    write(
        &r,
        "package.json",
        "{\"workspaces\":[\"packages/*\"],\"dependencies\":{\"react-native\":\"1\"}}",
    );
    write(
        &r,
        "packages/cli/package.json",
        "{\"bin\":{\"x\":\"x.js\"}}",
    );
    track(&r);
    assert_eq!(
        detected(&r),
        vec![
            pair("package.json", "ui"),
            pair("packages/cli/package.json", "cli")
        ]
    );

    let r = corpus("bare-root", "");
    write(
        &r,
        "package.json",
        "{\"workspaces\":[\"packages/*\"],\"devDependencies\":{\"turbo\":\"2\"}}",
    );
    write(
        &r,
        "packages/web/package.json",
        "{\"dependencies\":{\"vue\":\"3\"}}",
    );
    track(&r);
    assert_eq!(detected(&r), vec![pair("packages/web/package.json", "ui")]);
}

#[test]
fn go_commands_wherever_they_sit_and_not_when_build_ignored() {
    let r = corpus("go", "");
    write(&r, "go.mod", "module x\n");
    write(&r, "cmd/main.go", "// Command x.\npackage main\n");
    write(
        &r,
        "tools/gen/main.go",
        "//go:build ignore\n\npackage main\n",
    );
    write(&r, "internal/s/s.go", "package s\n");
    write(&r, "cmd/main_test.go", "package main\n");
    track(&r);
    assert_eq!(detected(&r), vec![pair("cmd/main.go", "cli")]);
}

// --- the check ----------------------------------------------------------------

#[test]
fn check_wants_local_coverage() {
    // `ui` is declared for web/, and the second package is ALSO ui. Declaring
    // it for one package does not cover the other.
    let r = corpus("local", "areas:\n  \"web/**\": [ui]\n");
    write(
        &r,
        "web/package.json",
        "{\"dependencies\":{\"react\":\"19\"}}",
    );
    write(
        &r,
        "admin/package.json",
        "{\"dependencies\":{\"react\":\"19\"}}",
    );
    track(&r);
    let got = run(&r, &["traits", "--check"]);
    assert_eq!(got.code, 1, "{}", got.out);
    assert!(
        got.out.contains("uncovered: `admin/package.json`"),
        "{}",
        got.out
    );
    assert!(!got.out.contains("`web/package.json`"), "{}", got.out);
}

#[test]
fn a_disclaim_dismisses_a_false_positive() {
    let r = corpus(
        "disclaim",
        "areas:\n  \"web/**\": [ui]\ndisclaims:\n  \"tools/**\": [cli]\n",
    );
    write(
        &r,
        "web/package.json",
        "{\"dependencies\":{\"react\":\"19\"}}",
    );
    write(&r, "tools/package.json", "{\"bin\":\"gen.js\"}");
    track(&r);
    let got = run(&r, &["traits", "--check"]);
    assert_eq!(got.code, 0, "{}", got.out);
    // Disclaims never filter rules: the summary is about areas alone.
    assert!(run(&r, &["traits", "--summary"])
        .out
        .starts_with("traits here: ui"));
}

#[test]
fn a_stale_glob_is_a_finding() {
    let r = corpus("stale", "areas:\n  \"gone/**\": [cli]\n");
    track(&r);
    let got = run(&r, &["traits", "--check"]);
    assert_eq!(got.code, 1);
    assert!(
        got.out
            .contains("stale-glob: `gone/**` under `areas` matches no tracked file"),
        "{}",
        got.out
    );
}

#[test]
fn a_catch_all_empty_area_swallowing_a_cli_is_reported() {
    let r = corpus("swallow", "areas:\n  \"**\": []\n");
    write(&r, "Cargo.toml", "[package]\nname = \"x\"\n");
    write(&r, "src/main.rs", "fn main() {}\n");
    track(&r);
    let got = run(&r, &["traits", "--check"]);
    assert_eq!(got.code, 1);
    assert!(
        got.out.contains("uncovered: `Cargo.toml` looks like `cli`"),
        "{}",
        got.out
    );
}

#[test]
fn could_not_inspect_is_exit_3_and_never_a_pass() {
    // Not a repository: git fails, and that is not "nothing to report".
    let r = corpus("no-repo", "");
    let got = run_env(&r, &["traits", "--check"], &[("GIT_DIR", "/nonexistent")]);
    assert_eq!(got.code, 3, "{}{}", got.out, got.err);
    assert!(
        got.err.contains("could not list tracked files"),
        "{}",
        got.err
    );

    // No git at all.
    let got = run_env(&r, &["traits", "--detect"], &[("PATH", "/nonexistent")]);
    assert_eq!(got.code, 3, "{}", got.err);

    // A package.json that is not JSON.
    let r = corpus("bad-json", "");
    write(&r, "package.json", "{ not json");
    track(&r);
    let got = run(&r, &["traits", "--check"]);
    assert_eq!(got.code, 3);
    assert!(
        got.err.contains("`package.json` is not JSON"),
        "{}",
        got.err
    );

    // A tracked manifest that is gone from the working tree.
    let r = corpus("unreadable", "");
    write(&r, "Cargo.toml", "[package]\n");
    track(&r);
    fs::remove_file(r.join("Cargo.toml")).unwrap();
    let got = run(&r, &["traits", "--check"]);
    assert_eq!(got.code, 3);
    assert!(
        got.err.contains("could not read `Cargo.toml`"),
        "{}",
        got.err
    );
}

#[test]
fn modes_are_exclusive() {
    let r = corpus("modes", "");
    assert_eq!(run(&r, &["traits", "--detect", "--check"]).code, 2);
    assert_eq!(run(&r, &["traits", "extra"]).code, 2);
}

/// prettier with `singleQuote: true` rewrites `"web/**"` as `'web/**'` at
/// pre-commit. 1.7.1 read that key with its quotes on and refused the glob;
/// a formatted registry must mean what the unformatted one meant.
#[test]
fn a_prettier_single_quoted_area_key_is_the_same_glob() {
    let r = corpus(
        "single-quoted",
        "areas:\n  'web/**': [ui]\ndisclaims:\n  'tools/**': [cli]\n",
    );
    let got = run(&r, &["rules", "--level", "constraint"]);
    assert_eq!(got.code, 0, "{}", got.err);
    assert!(!got.out.contains("cli.exit"), "{}", got.out);
    assert!(run(&r, &["traits", "--summary"])
        .out
        .starts_with("traits here: ui"));
}

/// `--detect` is how a repository learns what to declare, so it runs before
/// there is a registry: it reads the repository git names and proposes
/// against an empty vocabulary. Every other mode still needs a corpus.
#[test]
fn detect_runs_before_there_is_a_registry() {
    // Under the system temp dir: this repository has a registry of its own.
    let r = std::env::temp_dir().join(format!("aval-no-registry-{}", std::process::id()));
    let _ = fs::remove_dir_all(&r);
    fs::create_dir_all(&r).unwrap();
    write(&r, "Cargo.toml", "[package]\nname = \"x\"\n");
    write(&r, "src/main.rs", "fn main() {}\n");
    track(&r);
    let got = run(&r.join("src"), &["traits", "--detect"]);
    assert_eq!(got.code, 0, "{}", got.err);
    assert!(got.out.contains("\"**\": [cli]"), "{}", got.out);
    assert!(
        got.out.contains("not in the vocabulary: cli"),
        "{}",
        got.out
    );
    assert_eq!(run(&r, &["traits", "--check"]).code, 3);
}

/// `relevant --path web` names a directory, and `web/**` covers it. 1.7.x
/// compared the glob against the literal path `web`, found nothing, and
/// treated the directory as uncovered, so nothing was filtered.
#[test]
fn a_directory_path_is_covered_by_the_area_beneath_it() {
    let r = corpus("dir-path", "areas:\n  \"web/**\": [ui]\n");
    write(&r, "web/app.tsx", "export {}\n");
    let got = run(
        &r,
        &["relevant", "--path", "web", "--text", "exit", "--json"],
    );
    assert_eq!(got.code, 0, "{}", got.err);
    assert!(
        got.out
            .contains("\"omitted\":{\"constraints\":1,\"heuristics\":1,\"traits\":[\"ui\"]}"),
        "{}",
        got.out
    );
    // A directory no area reaches is still uncovered: nothing filtered.
    write(&r, "docs/readme.md", "x\n");
    let got = run(
        &r,
        &["relevant", "--path", "docs", "--text", "exit", "--json"],
    );
    assert!(
        got.out
            .contains("\"omitted\":{\"constraints\":0,\"heuristics\":0,\"traits\":[]}"),
        "{}",
        got.out
    );
}
