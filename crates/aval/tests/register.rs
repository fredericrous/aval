//! `aval mcp install|uninstall` and `aval hook uninstall`, run as the binary.
//! `claude` is a fake on PATH that logs its argv.

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

fn scratch(name: &str) -> PathBuf {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/register-tests")
        .join(name);
    let _ = fs::remove_dir_all(&directory); // a first run has no previous directory to remove
    fs::create_dir_all(&directory).expect("mkdir");
    directory
}

/// A `bin/` holding a `claude` that appends its argv to `argv.log` and answers
/// `mcp get` by the `state` file: `registered`, `missing` or anything else.
/// Like the real one, `mcp add` of a registered name fails and `mcp remove` of
/// a missing one fails; each success moves the state.
fn fake_claude(root: &Path, state: &str) -> PathBuf {
    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    fs::write(root.join("state"), state).unwrap();
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{log}'\n\
         if [ \"$1 $2\" = 'mcp get' ]; then\n\
           case \"$(cat '{state}')\" in\n\
             registered) echo 'aval: connected'; exit 0;;\n\
             missing) echo 'No MCP server found with name: aval'; exit 1;;\n\
             *) echo 'boom'; exit 7;;\n\
           esac\n\
         fi\n\
         if [ \"$1 $2\" = 'mcp add' ]; then\n\
           if [ \"$(cat '{state}')\" = registered ]; then echo 'MCP server aval already exists' >&2; exit 1; fi\n\
           echo registered > '{state}'\n\
         fi\n\
         if [ \"$1 $2\" = 'mcp remove' ]; then\n\
           if [ \"$(cat '{state}')\" != registered ]; then echo 'No MCP server found' >&2; exit 1; fi\n\
           echo missing > '{state}'\n\
         fi\nexit 0\n",
        log = root.join("argv.log").display(),
        state = root.join("state").display()
    );
    let path = bin_dir.join("claude");
    fs::write(&path, script).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    bin_dir
}

/// The fake's directory first, then the tools its script uses.
fn with_system_tools(fake_dir: &Path) -> PathBuf {
    PathBuf::from(format!("{}:/usr/bin:/bin", fake_dir.display()))
}

fn run_with_path(dir: &Path, path: &Path, args: &[&str]) -> Run {
    let output = Command::new(bin())
        .args(args)
        .current_dir(dir)
        .env("PATH", path)
        .output()
        .expect("spawn aval");
    Run {
        code: output.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&output.stdout).to_string(),
        err: String::from_utf8_lossy(&output.stderr).to_string(),
    }
}

fn log(root: &Path) -> String {
    fs::read_to_string(root.join("argv.log")).unwrap_or_default()
}

#[test]
fn mcp_install_previews_without_write_and_registers_with_it() {
    let scratch_root = scratch("mcp-install");
    let path = with_system_tools(&fake_claude(&scratch_root, "missing"));

    let preview = run_with_path(&scratch_root, &path, &["mcp", "install"]);
    assert_eq!(preview.code, 0, "{}{}", preview.out, preview.err);
    assert!(preview
        .out
        .contains("would run: claude mcp add --scope user aval -- "));
    assert_eq!(log(&scratch_root), "", "a preview runs nothing");

    let done = run_with_path(&scratch_root, &path, &["mcp", "install", "--write"]);
    assert_eq!(done.code, 0, "{}{}", done.out, done.err);
    let logged = log(&scratch_root);
    let line = logged.lines().nth(1).expect("claude add was run after get");
    assert_eq!(logged.lines().next(), Some("mcp get aval"));
    let prefix = "mcp add --scope user aval -- ";
    assert!(line.starts_with(prefix), "{line}");
    let exe = line[prefix.len()..]
        .strip_suffix(" mcp")
        .expect("ends in mcp");
    assert!(Path::new(exe).is_absolute(), "{exe}");
    assert_eq!(
        Path::new(exe).file_name().unwrap(),
        Path::new(bin()).file_name().unwrap()
    );
}

#[test]
fn mcp_uninstall_removes_at_user_scope() {
    let scratch_root = scratch("mcp-uninstall");
    let path = with_system_tools(&fake_claude(&scratch_root, "registered"));
    assert_eq!(
        run_with_path(&scratch_root, &path, &["mcp", "uninstall"]).code,
        0
    );
    assert_eq!(log(&scratch_root), "");
    let done = run_with_path(&scratch_root, &path, &["mcp", "uninstall", "--write"]);
    assert_eq!(done.code, 0, "{}{}", done.out, done.err);
    assert_eq!(
        log(&scratch_root),
        "mcp get aval\nmcp remove --scope user aval\n"
    );
}

#[test]
fn a_second_install_write_is_a_one_line_no_op() {
    let root = scratch("mcp-twice");
    let path = with_system_tools(&fake_claude(&root, "missing"));
    assert_eq!(
        run_with_path(&root, &path, &["mcp", "install", "--write"]).code,
        0
    );
    let again = run_with_path(&root, &path, &["mcp", "install", "--write"]);
    assert_eq!(again.code, 0, "{}{}", again.out, again.err);
    assert_eq!(again.out.lines().count(), 1, "{}", again.out);
    assert!(again.out.contains("already registered"), "{}", again.out);
    assert_eq!(again.err, "");
    assert_eq!(
        log(&root)
            .lines()
            .filter(|l| l.starts_with("mcp add"))
            .count(),
        1,
        "the second install did not add again"
    );
}

#[test]
fn uninstall_write_when_not_registered_is_a_one_line_no_op() {
    let root = scratch("mcp-uninstall-missing");
    let path = with_system_tools(&fake_claude(&root, "missing"));
    let done = run_with_path(&root, &path, &["mcp", "uninstall", "--write"]);
    assert_eq!(done.code, 0, "{}{}", done.out, done.err);
    assert_eq!(done.out.lines().count(), 1, "{}", done.out);
    assert!(done.out.contains("not registered"), "{}", done.out);
    assert_eq!(log(&root), "mcp get aval\n", "no remove was attempted");
}

#[test]
fn write_with_check_is_a_usage_error() {
    let root = scratch("write-check");
    let path = with_system_tools(&fake_claude(&root, "missing"));
    for args in [
        &["mcp", "install", "--write", "--check"][..],
        &["mcp", "uninstall", "--write", "--check"][..],
        &["hook", "uninstall", "--write", "--check"][..],
    ] {
        let got = run_with_path(&root, &path, args);
        assert_eq!(got.code, 2, "{args:?}: {}{}", got.out, got.err);
        assert!(got.err.contains("mutually exclusive"), "{}", got.err);
    }
    assert_eq!(log(&root), "", "claude was never asked");
}

#[test]
fn mcp_check_reports_registered_not_registered_and_unknown() {
    let scratch_root = scratch("mcp-check");
    let path = with_system_tools(&fake_claude(&scratch_root, "registered"));
    assert_eq!(
        run_with_path(&scratch_root, &path, &["mcp", "install", "--check"]).code,
        0
    );
    assert_eq!(
        run_with_path(&scratch_root, &path, &["mcp", "uninstall", "--check"]).code,
        1
    );
    assert_eq!(log(&scratch_root), "mcp get aval\nmcp get aval\n");

    fake_claude(&scratch_root, "missing");
    assert_eq!(
        run_with_path(&scratch_root, &path, &["mcp", "install", "--check"]).code,
        1
    );
    assert_eq!(
        run_with_path(&scratch_root, &path, &["mcp", "uninstall", "--check"]).code,
        0
    );

    fake_claude(&scratch_root, "broken");
    let unknown = run_with_path(&scratch_root, &path, &["mcp", "install", "--check"]);
    assert_eq!(unknown.code, 3, "{}{}", unknown.out, unknown.err);
    assert!(unknown.err.contains("boom"), "{}", unknown.err);
}

#[test]
fn an_absent_claude_is_a_reported_gap_not_a_failure() {
    let scratch_root = scratch("mcp-absent");
    let empty = scratch_root.join("empty");
    fs::create_dir_all(&empty).unwrap();
    for args in [
        &["mcp", "install", "--write"][..],
        &["mcp", "uninstall", "--write"][..],
        &["mcp", "install", "--check"][..],
    ] {
        let got = run_with_path(&scratch_root, &empty, args);
        assert_eq!(got.code, 0, "{args:?}: {}{}", got.out, got.err);
        assert_eq!(got.out.lines().count(), 1, "one line: {}", got.out);
        assert!(got.out.contains("`claude` is not on PATH"), "{}", got.out);
    }
}

fn hook_root(name: &str) -> PathBuf {
    let scratch_root = scratch(name);
    fs::write(
        scratch_root.join(".adr.yaml"),
        "dir: docs/adr\nscopes: []\nkeys:\n  a.b:\n",
    )
    .unwrap();
    scratch_root
}

fn aval(dir: &Path, args: &[&str]) -> Run {
    run_with_path(dir, Path::new("/usr/bin:/bin"), args)
}

fn settings(root: &Path) -> String {
    fs::read_to_string(root.join(".claude/settings.json")).unwrap()
}

#[test]
fn hook_install_then_uninstall_restores_settings_byte_for_byte() {
    let scratch_root = hook_root("roundtrip");
    fs::create_dir_all(scratch_root.join(".claude")).unwrap();
    let before = "{\n  \"hooks\": {\n    \"SessionStart\": [\n      {\n        \"hooks\": [\n          {\n            \"command\": \"echo hi\",\n            \"type\": \"command\"\n          }\n        ]\n      }\n    ]\n  },\n  \"model\": \"opus\"\n}\n";
    fs::write(scratch_root.join(".claude/settings.json"), before).unwrap();
    assert_eq!(aval(&scratch_root, &["hook", "install"]).code, 0);
    assert_ne!(settings(&scratch_root), before);

    let preview = aval(&scratch_root, &["hook", "uninstall"]);
    assert_eq!(preview.code, 0, "{}{}", preview.out, preview.err);
    assert!(
        scratch_root.join(".claude/hooks/aval-heads.sh").exists(),
        "a preview removes nothing"
    );

    assert_eq!(
        aval(&scratch_root, &["hook", "uninstall", "--check"]).code,
        1
    );
    let done = aval(&scratch_root, &["hook", "uninstall", "--write"]);
    assert_eq!(done.code, 0, "{}{}", done.out, done.err);
    assert_eq!(settings(&scratch_root), before);
    assert!(!scratch_root.join(".claude/hooks/aval-heads.sh").exists());
    assert_eq!(
        aval(&scratch_root, &["hook", "uninstall", "--check"]).code,
        0
    );
}

#[test]
fn uninstall_keeps_the_indentation_and_missing_final_newline() {
    let scratch_root = hook_root("indent");
    fs::create_dir_all(scratch_root.join(".claude")).unwrap();
    let ours = "{\n    \"hooks\": {\n        \"SessionStart\": [\n            {\n                \"hooks\": [\n                    {\n                        \"command\": \"sh .claude/hooks/aval-heads.sh\",\n                        \"type\": \"command\"\n                    }\n                ]\n            }\n        ]\n    },\n    \"model\": \"opus\"\n}";
    fs::write(scratch_root.join(".claude/settings.json"), ours).unwrap();
    assert_eq!(
        aval(&scratch_root, &["hook", "uninstall", "--write"]).code,
        0
    );
    assert_eq!(settings(&scratch_root), "{\n    \"model\": \"opus\"\n}");
}

#[test]
fn uninstall_after_a_fresh_install_leaves_no_trace() {
    let scratch_root = hook_root("fresh");
    assert_eq!(aval(&scratch_root, &["hook", "install"]).code, 0);
    assert_eq!(
        aval(&scratch_root, &["hook", "uninstall", "--write"]).code,
        0
    );
    assert!(!scratch_root.join(".claude").exists());
}

#[test]
fn a_script_without_the_marker_is_left_alone() {
    let scratch_root = hook_root("foreign");
    assert_eq!(aval(&scratch_root, &["hook", "install"]).code, 0);
    let script = scratch_root.join(".claude/hooks/aval-heads.sh");
    fs::write(&script, "#!/bin/sh\necho mine\n").unwrap();
    let done = aval(&scratch_root, &["hook", "uninstall", "--write"]);
    assert_eq!(done.code, 0, "{}{}", done.out, done.err);
    assert_eq!(
        fs::read_to_string(&script).unwrap(),
        "#!/bin/sh\necho mine\n"
    );
    assert!(done.out.contains("left"), "{}", done.out);
    assert!(!scratch_root.join(".claude/settings.json").exists());
}

#[test]
fn help_lists_the_new_commands() {
    let scratch_root = scratch("help");
    let got = aval(&scratch_root, &["--help"]);
    for line in [
        "aval mcp install",
        "aval mcp uninstall",
        "aval hook uninstall",
    ] {
        assert!(got.out.contains(line), "{line}");
    }
}
