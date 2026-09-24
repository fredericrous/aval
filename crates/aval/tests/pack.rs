//! `aval pack` and `aval add`, run as the binary.
//!
//! No test here touches the network. A pack source is a git repository, so a
//! local path is a complete fixture — and exercising the local-path case is
//! not a compromise, it is the case an offline machine and a USB stick both
//! take.

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

impl Run {
    fn all(&self) -> String {
        format!("{}{}", self.out, self.err)
    }
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

fn git(dir: &Path, args: &[&str]) {
    let o = Command::new("git")
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

/// A fixture repository, with the machine's own git configuration kept out of
/// it. `init.templateDir` installs this developer's commit-msg hooks into every
/// new repository, and a fixture commit does not have to satisfy somebody's
/// subject-line policy to be a valid source for a pack.
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

fn scratch(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/pack-tests")
        .join(name);
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).expect("mkdir");
    p
}

fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, body).unwrap();
}

const FLEET_REGISTRY: &str = "\
dir: docs/adr
scopes: [effect-stack, duro-stack]
keys:
  stack.sql-layer:
    description: The SQL access layer
    scopes: [effect-stack]
  release.trigger:
    description: What starts a release
";

fn record(id: &str, key: &str, scope: Option<&str>, choice: &str, first_or: &str) -> String {
    let scope = scope
        .map(|s| format!("    scope: {}\n", s))
        .unwrap_or_default();
    format!(
        "---\nid: {}\nstatus: accepted\ndecisions:\n  - key: {}\n{}    choice: {}\n    {}\n---\n# {}\n",
        id, key, scope, choice, first_or, id
    )
}

/// A producing repository: a corpus, a committed `aval.pack`, one commit.
fn producer(name: &str) -> PathBuf {
    let p = scratch(name);
    write(&p, ".adr.yaml", FLEET_REGISTRY);
    write(
        &p,
        "docs/adr/0002-sql.md",
        &record(
            "ADR-0002",
            "stack.sql-layer",
            Some("effect-stack"),
            "\"@effect/sql\"",
            "first: true",
        ),
    );
    write(
        &p,
        "docs/adr/0008-tags.md",
        &record(
            "ADR-0008",
            "release.trigger",
            None,
            "a pushed version tag",
            "first: true",
        ),
    );
    assert_eq!(run(&p, &["heads", "--write"]).code, 0);
    assert_eq!(run(&p, &["pack", "--write"]).code, 0);
    git_init(&p);
    git(&p, &["add", "-A"]);
    git(&p, &["commit", "-qm", "corpus"]);
    p
}

/// A consuming repository that keeps no records of its own.
fn consumer_pack_only(name: &str) -> PathBuf {
    let p = scratch(name);
    write(&p, ".adr.yaml", "scopes: []\nkeys:\n");
    p
}

/// A consuming repository with a corpus of its own.
fn consumer_with_corpus(name: &str) -> PathBuf {
    let p = scratch(name);
    write(
        &p,
        ".adr.yaml",
        "dir: docs/adr\nscopes: []\nkeys:\n  app.router:\n",
    );
    write(
        &p,
        "docs/adr/0001-router.md",
        &record(
            "ADR-0001",
            "app.router",
            None,
            "React Router",
            "first: true",
        ),
    );
    assert_eq!(run(&p, &["heads", "--write"]).code, 0);
    p
}

fn src(p: &Path) -> String {
    p.display().to_string()
}

// --- producing -------------------------------------------------------------

#[test]
fn pack_write_is_idempotent_and_check_notices_drift() {
    let p = producer("produce");
    assert_eq!(run(&p, &["pack", "--check"]).code, 0);

    let got = run(&p, &["pack", "--write"]);
    assert_eq!(got.code, 0);
    assert!(got.out.contains("already current"), "{}", got.out);

    // A decision the published file does not carry.
    write(
        &p,
        "docs/adr/0009-gate.md",
        &record(
            "ADR-0009",
            "release.trigger",
            Some("duro-stack"),
            "a merge",
            "first: true",
        ),
    );
    let got = run(&p, &["pack", "--check"]);
    assert_eq!(got.code, 1, "{}", got.all());
    assert!(got.out.contains("pack-fresh"), "{}", got.out);
    // And `check` reports it too, because a stale published pack is a decision
    // handed to somebody else that nobody made.
    assert_eq!(run(&p, &["check"]).code, 1);
}

#[test]
fn a_corpus_that_publishes_nothing_is_not_held_to_publishing() {
    let p = scratch("no-pack");
    write(&p, ".adr.yaml", FLEET_REGISTRY);
    write(
        &p,
        "docs/adr/0002-sql.md",
        &record(
            "ADR-0002",
            "stack.sql-layer",
            Some("effect-stack"),
            "X",
            "first: true",
        ),
    );
    assert_eq!(run(&p, &["heads", "--write"]).code, 0);
    assert_eq!(run(&p, &["pack", "--check"]).code, 0);
    assert_eq!(
        run(&p, &["check"]).code,
        0,
        "a new check must not fail a clean corpus"
    );
}

// --- vendoring -------------------------------------------------------------

#[test]
fn a_pack_lands_with_its_commit_id_and_is_registered() {
    let fleet = producer("land-fleet");
    let c = consumer_pack_only("land-consumer");

    let got = run(&c, &["add", &src(&fleet)]);
    assert_eq!(got.code, 0, "{}", got.all());

    let vendored = fs::read_to_string(c.join(".adr/packs/land-fleet.pack")).expect("written");
    let head = String::from_utf8_lossy(
        &Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&fleet)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    assert!(
        vendored.contains(&format!("# commit: {}", head)),
        "the recorded id must be the commit that was read: {}",
        vendored
    );
    assert!(fs::read_to_string(c.join(".adr.yaml"))
        .unwrap()
        .contains("- .adr/packs/land-fleet.pack"));
}

/// The point of the whole exercise: a repository with no corpus of its own
/// answers fleet questions.
#[test]
fn a_consumer_with_no_corpus_resolves_the_fleet() {
    let fleet = producer("read-fleet");
    let c = consumer_pack_only("read-consumer");
    assert_eq!(run(&c, &["add", "--as", "decisions", &src(&fleet)]).code, 0);

    let got = run(
        &c,
        &["resolve", "stack.sql-layer", "--scope", "effect-stack"],
    );
    assert_eq!(got.code, 0, "{}", got.all());
    assert!(got.out.contains("@effect/sql"), "{}", got.out);
    assert!(got.out.contains("decisions:ADR-0002"), "{}", got.out);
    assert!(
        got.out.contains("change it there, not here"),
        "a borrowed answer must say so: {}",
        got.out
    );

    assert_eq!(run(&c, &["check"]).code, 0, "{}", run(&c, &["check"]).all());
    // The projection still prints, which is what the session hook reads.
    let heads = run(&c, &["heads"]);
    assert_eq!(heads.code, 0);
    assert!(heads.out.contains("decisions:ADR-0002"), "{}", heads.out);
    // But there is nowhere to write it, and saying so beats writing it at the
    // repository root.
    assert_eq!(run(&c, &["heads", "--write"]).code, 2);
}

#[test]
fn ids_are_namespaced_so_two_corpora_can_both_number_from_one() {
    let fleet = producer("ns-fleet");
    let c = consumer_with_corpus("ns-consumer");
    // The consumer's own ADR-0001 and the fleet's records coexist.
    write(
        &c,
        "docs/adr/0002-mine.md",
        &record(
            "ADR-0002",
            "app.router",
            Some("*"),
            "Ignored",
            "first: true",
        ),
    );
    let _ = fs::remove_file(c.join("docs/adr/0002-mine.md"));

    let add = run(&c, &["add", "--as", "fleet", &src(&fleet)]);
    assert_eq!(add.code, 0, "{}", add.all());
    // A borrowed decision is decided here, so the projection gains rows and
    // `add` says so rather than leaving the gate to.
    assert!(add.out.contains("aval heads --write"), "{}", add.out);
    assert_eq!(run(&c, &["heads", "--write"]).code, 0);

    let got = run(&c, &["check"]);
    assert_eq!(got.code, 0, "{}", got.all());
    assert!(fs::read_to_string(c.join("docs/adr/HEADS.md"))
        .unwrap()
        .contains("fleet:ADR-0002"));

    assert_eq!(run(&c, &["show", "fleet:ADR-0002"]).code, 0);
    assert_eq!(run(&c, &["show", "ADR-0001"]).code, 0);
    // The bare fleet id is not a record here. It names nothing rather than
    // silently meaning the local one.
    assert_eq!(run(&c, &["show", "ADR-0002"]).code, 7);
}

/// The enforcement that makes vendoring worth anything.
#[test]
fn a_local_record_re_deciding_a_fleet_slot_is_a_contradiction() {
    let fleet = producer("clash-fleet");
    let c = consumer_with_corpus("clash-consumer");
    assert_eq!(run(&c, &["add", "--as", "fleet", &src(&fleet)]).code, 0);

    // Same key, same scope, a different answer, declared `first`.
    write(
        &c,
        "docs/adr/0005-drizzle.md",
        &record(
            "ADR-0005",
            "stack.sql-layer",
            Some("effect-stack"),
            "Drizzle",
            "first: true",
        ),
    );
    let got = run(
        &c,
        &["resolve", "stack.sql-layer", "--scope", "effect-stack"],
    );
    assert_eq!(got.code, 5, "{}", got.all());
    assert_eq!(run(&c, &["check"]).code, 1);
}

/// And the divergence that is not a divergence: one repository can hold more
/// than one answer for a key, at scopes of its own, without touching the
/// fleet's slot.
#[test]
fn a_consumer_widens_a_vendored_key_and_the_fleet_head_does_not_move() {
    let fleet = producer("widen-fleet");
    let c = consumer_with_corpus("widen-consumer");
    assert_eq!(run(&c, &["add", "--as", "fleet", &src(&fleet)]).code, 0);

    // The local list is what this repository is *adding*, so narrowing the
    // fleet's own scope is not something the format can say.
    write(
        &c,
        ".adr.yaml",
        "dir: docs/adr\npacks:\n  - .adr/packs/fleet.pack\nscopes: [browser]\nkeys:\n  \
         app.router:\n  stack.sql-layer:\n    scopes: [browser]\n",
    );
    write(
        &c,
        "docs/adr/0005-opfs.md",
        &record(
            "ADR-0005",
            "stack.sql-layer",
            Some("browser"),
            "SQLite on OPFS",
            "first: true",
        ),
    );
    assert_eq!(run(&c, &["heads", "--write"]).code, 0);

    let got = run(&c, &["check"]);
    assert_eq!(got.code, 0, "{}", got.all());

    let local = run(&c, &["resolve", "stack.sql-layer", "--scope", "browser"]);
    assert_eq!(local.code, 0, "{}", local.all());
    assert!(local.out.contains("SQLite on OPFS"), "{}", local.out);
    assert!(!local.out.contains("vendored:"), "{}", local.out);

    let fleet_answer = run(
        &c,
        &["resolve", "stack.sql-layer", "--scope", "effect-stack"],
    );
    assert_eq!(fleet_answer.code, 0, "{}", fleet_answer.all());
    assert!(
        fleet_answer.out.contains("@effect/sql"),
        "{}",
        fleet_answer.out
    );
}

#[test]
fn a_local_description_for_a_vendored_key_is_refused() {
    let fleet = producer("desc-fleet");
    let c = consumer_with_corpus("desc-consumer");
    assert_eq!(run(&c, &["add", "--as", "fleet", &src(&fleet)]).code, 0);
    write(
        &c,
        ".adr.yaml",
        "dir: docs/adr\npacks:\n  - .adr/packs/fleet.pack\nscopes: []\nkeys:\n  \
         app.router:\n  stack.sql-layer:\n    description: my own words\n",
    );
    let got = run(&c, &["check"]);
    assert_eq!(got.code, 3, "{}", got.all());
    assert!(got.err.contains("pack-key-widens"), "{}", got.err);
}

// --- the failure paths -----------------------------------------------------

#[test]
fn a_broken_pack_is_refused_whole_and_writes_nothing() {
    let fleet = producer("bad-fleet");
    fs::write(
        fleet.join("aval.pack"),
        "aval: \"0.5.0\"\nscopes: []\nkeys: {}\nrecords:\n  - id: \"ADR-1\"\n    status: \"nonsense\"\n",
    )
    .unwrap();
    git(&fleet, &["add", "-A"]);
    git(&fleet, &["commit", "-qm", "break it"]);

    let c = consumer_pack_only("bad-consumer");
    let before = fs::read_to_string(c.join(".adr.yaml")).unwrap();
    let got = run(&c, &["add", &src(&fleet)]);
    assert_eq!(got.code, 1, "{}", got.all());
    assert!(got.err.contains("not readable"), "{}", got.err);
    assert_eq!(fs::read_to_string(c.join(".adr.yaml")).unwrap(), before);
    assert!(
        !c.join(".adr/packs").exists(),
        "nothing should have been written"
    );
}

#[test]
fn an_unresolvable_revision_fails_before_writing() {
    let fleet = producer("rev-fleet");
    let c = consumer_pack_only("rev-consumer");
    let before = fs::read_to_string(c.join(".adr.yaml")).unwrap();
    let got = run(&c, &["add", &format!("{}@no-such-tag", src(&fleet))]);
    assert_eq!(got.code, 1, "{}", got.all());
    assert_eq!(fs::read_to_string(c.join(".adr.yaml")).unwrap(), before);
    assert!(!c.join(".adr/packs").exists());
}

#[test]
fn a_source_without_a_pack_is_reported() {
    let fleet = producer("nopack-fleet");
    fs::remove_file(fleet.join("aval.pack")).unwrap();
    git(&fleet, &["add", "-A"]);
    git(&fleet, &["commit", "-qm", "drop the pack"]);
    let c = consumer_pack_only("nopack-consumer");
    let got = run(&c, &["add", &src(&fleet)]);
    assert_eq!(got.code, 1);
    assert!(got.err.contains("aval.pack"), "{}", got.err);
}

#[test]
fn dry_run_shows_everything_and_writes_nothing() {
    let fleet = producer("dry-fleet");
    let c = consumer_pack_only("dry-consumer");
    let before = fs::read_to_string(c.join(".adr.yaml")).unwrap();
    let got = run(&c, &["add", "--dry-run", &src(&fleet)]);
    assert_eq!(got.code, 0, "{}", got.all());
    assert!(got.out.contains("stack.sql-layer"), "{}", got.out);
    assert!(got.out.contains("nothing written"), "{}", got.out);
    assert_eq!(fs::read_to_string(c.join(".adr.yaml")).unwrap(), before);
    assert!(!c.join(".adr/packs").exists());
}

/// Being behind matters here in a way it does not for a vendored *command*:
/// a superseded decision still answering `active` is the failure the tool
/// exists to prevent. So there is a way to ask, even though nothing automatic
/// can.
#[test]
fn add_check_notices_a_pack_that_has_moved() {
    let fleet = producer("moved-fleet");
    let c = consumer_pack_only("moved-consumer");
    assert_eq!(run(&c, &["add", "--as", "fleet", &src(&fleet)]).code, 0);

    let got = run(&c, &["add", "--check"]);
    assert_eq!(got.code, 0, "{}", got.all());
    assert!(got.out.contains("current"), "{}", got.out);

    write(
        &fleet,
        "docs/adr/0009-more.md",
        &record(
            "ADR-0009",
            "release.trigger",
            Some("duro-stack"),
            "a merge",
            "first: true",
        ),
    );
    assert_eq!(run(&fleet, &["heads", "--write"]).code, 0);
    assert_eq!(run(&fleet, &["pack", "--write"]).code, 0);
    git(&fleet, &["add", "-A"]);
    git(&fleet, &["commit", "-qm", "another decision"]);

    let got = run(&c, &["add", "--check"]);
    assert_eq!(got.code, 1, "{}", got.all());
    assert!(got.out.contains("behind"), "{}", got.out);
    // Being behind is reported, never repaired: re-adding is a diff somebody
    // reads, not something a check does on their behalf.
    assert!(got.out.contains("aval add"), "{}", got.out);
}

/// `--quiet` is the hook's voice: nothing when every pack is current or
/// cannot be asked, and only the packs that need a person when one does.
#[test]
fn add_check_quiet_speaks_only_when_a_pack_needs_a_person() {
    let fleet = producer("quiet-fleet");
    let c = consumer_pack_only("quiet-consumer");
    assert_eq!(run(&c, &["add", "--as", "fleet", &src(&fleet)]).code, 0);

    let got = run(&c, &["add", "--check", "--quiet", "--budget", "10"]);
    assert_eq!(got.code, 0, "{}", got.all());
    assert_eq!(got.out.trim(), "", "current is silence: {}", got.out);

    write(
        &fleet,
        "docs/adr/0009-more.md",
        &record(
            "ADR-0009",
            "release.trigger",
            Some("duro-stack"),
            "a merge",
            "first: true",
        ),
    );
    assert_eq!(run(&fleet, &["heads", "--write"]).code, 0);
    assert_eq!(run(&fleet, &["pack", "--write"]).code, 0);
    git(&fleet, &["add", "-A"]);
    git(&fleet, &["commit", "-qm", "another decision"]);

    let got = run(&c, &["add", "--check", "--quiet"]);
    assert_eq!(got.code, 1, "{}", got.all());
    assert!(got.out.contains("behind"), "{}", got.out);
    assert!(got.out.contains("aval add"), "{}", got.out);
    assert!(!got.out.contains("current"), "{}", got.out);

    // A source that cannot be asked is `unknown`: exit 0, and quiet says
    // nothing about it.
    let moved = c.join(".adr/packs/fleet.pack");
    let text = fs::read_to_string(&moved).unwrap();
    let gone = fleet.with_file_name("quiet-fleet-gone");
    fs::write(&moved, text.replace(&src(&fleet), &gone.to_string_lossy())).unwrap();
    let got = run(&c, &["add", "--check", "--quiet", "--budget", "5"]);
    assert_eq!(got.code, 0, "{}", got.all());
    assert_eq!(
        got.out.trim(),
        "",
        "could not ask is not a notice: {}",
        got.out
    );
    let loud = run(&c, &["add", "--check"]);
    assert!(loud.out.contains("could not be checked"), "{}", loud.out);

    assert_eq!(
        run(&c, &["add", "--check", "--budget", "0"]).code,
        2,
        "usage"
    );
}

/// The session hook, end to end: a consumer whose fleet moved is told at
/// session start, once an hour, and a current one hears nothing about it.
#[test]
fn the_session_hook_says_when_the_vendored_decisions_are_behind() {
    let fleet = producer("hook-fleet");
    let c = consumer_pack_only("hook-consumer");
    assert_eq!(run(&c, &["add", "--as", "fleet", &src(&fleet)]).code, 0);
    assert_eq!(run(&c, &["hook", "install"]).code, 0);
    let cache = c.join("cache-home");
    let hook = |c: &Path| -> String {
        let o = Command::new("sh")
            .arg(".claude/hooks/aval-heads.sh")
            .current_dir(c)
            .env("XDG_CACHE_HOME", &cache)
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    Path::new(bin()).parent().unwrap().display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .output()
            .expect("run the hook");
        String::from_utf8_lossy(&o.stdout).to_string()
    };

    let quiet = hook(&c);
    assert!(quiet.contains("ARCHITECTURE DECISIONS"), "{quiet}");
    assert!(!quiet.contains("BEHIND THEIR SOURCE"), "{quiet}");

    write(
        &fleet,
        "docs/adr/0009-more.md",
        &record(
            "ADR-0009",
            "release.trigger",
            Some("duro-stack"),
            "a merge",
            "first: true",
        ),
    );
    assert_eq!(run(&fleet, &["heads", "--write"]).code, 0);
    assert_eq!(run(&fleet, &["pack", "--write"]).code, 0);
    git(&fleet, &["add", "-A"]);
    git(&fleet, &["commit", "-qm", "another decision"]);

    // The stamp from the first run is fresh: the hook does not ask again
    // within the hour, so the move is not seen yet…
    let stamped = hook(&c);
    assert!(!stamped.contains("BEHIND THEIR SOURCE"), "{stamped}");
    // …until the stamp ages out.
    let stamp = fs::read_dir(cache.join("aval"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "pack-check"))
        .expect("the hook left a stamp under the cache directory");
    fs::remove_file(&stamp).unwrap();
    let told = hook(&c);
    assert!(told.contains("BEHIND THEIR SOURCE"), "{told}");
    assert!(told.contains("behind"), "{told}");
    assert!(told.contains("aval add"), "{told}");
    assert!(
        told.find("BEHIND THEIR SOURCE") < told.find("ARCHITECTURE DECISIONS"),
        "the notice comes before the heads it qualifies: {told}"
    );
    assert!(
        !c.join(".claude").join(".aval-pack-check").exists()
            && fs::read_dir(&c)
                .unwrap()
                .flatten()
                .all(|e| { !e.file_name().to_string_lossy().contains("pack-check") }),
        "nothing is written into the repository"
    );
}

#[test]
fn add_check_is_quiet_where_nothing_is_vendored() {
    let c = consumer_with_corpus("nocheck-consumer");
    let got = run(&c, &["add", "--check"]);
    assert_eq!(got.code, 0, "{}", got.all());
    assert!(got.out.contains("vendors no packs"), "{}", got.out);
}

/// What every consumer of 1.2 has on disk: the pack as `<name>.yaml`, listed
/// under that path. `add` moves both, and nothing else in the registry moves.
#[test]
fn a_pack_vendored_before_1_3_is_migrated_with_its_registry_line() {
    let fleet = producer("migrate-fleet");
    let c = consumer_pack_only("migrate-consumer");
    assert_eq!(run(&c, &["add", "--as", "fleet", &src(&fleet)]).code, 0);

    // Wind the clock back to what the previous version wrote.
    fs::rename(
        c.join(".adr/packs/fleet.pack"),
        c.join(".adr/packs/fleet.yaml"),
    )
    .unwrap();
    let old_reg = fs::read_to_string(c.join(".adr.yaml"))
        .unwrap()
        .replace("fleet.pack", "fleet.yaml")
        + "# a comment of my own\n";
    fs::write(c.join(".adr.yaml"), &old_reg).unwrap();
    // It still loads, because a `packs:` entry is a literal path and no check
    // reads the extension.
    assert_eq!(run(&c, &["check"]).code, 0);

    let dry = run(&c, &["add", "--dry-run", "--as", "fleet", &src(&fleet)]);
    assert_eq!(dry.code, 0, "{}", dry.all());
    assert!(dry.out.contains("migrated"), "{}", dry.out);
    assert!(
        c.join(".adr/packs/fleet.yaml").exists() && !c.join(".adr/packs/fleet.pack").exists(),
        "--dry-run reports the move without making it"
    );
    assert_eq!(fs::read_to_string(c.join(".adr.yaml")).unwrap(), old_reg);

    let got = run(&c, &["add", "--as", "fleet", &src(&fleet)]);
    assert_eq!(got.code, 0, "{}", got.all());
    assert!(
        got.out
            .contains("migrated  fleet  .adr/packs/fleet.yaml → .adr/packs/fleet.pack"),
        "{}",
        got.out
    );
    assert!(c.join(".adr/packs/fleet.pack").is_file());
    assert!(
        !c.join(".adr/packs/fleet.yaml").exists(),
        "leaving it would vendor the same decisions twice, under two names"
    );
    assert_eq!(
        fs::read_to_string(c.join(".adr.yaml")).unwrap(),
        old_reg.replace("fleet.yaml", "fleet.pack"),
        "one line moves; every other byte is the one the person wrote"
    );
    assert_eq!(run(&c, &["check"]).code, 0);

    // And the second run has nothing left to do.
    let again = run(&c, &["add", "--as", "fleet", &src(&fleet)]);
    assert!(again.out.contains("unchanged"), "{}", again.out);
}

/// The reason for the extension, and for comparing declarations: a formatter
/// went over the vendored file. It declares the same decisions, so it is
/// current, and `add` does not fight it by rewriting it.
#[test]
fn a_formatted_pack_is_unchanged_and_is_left_exactly_as_it_is() {
    let fleet = producer("format-fleet");
    let c = consumer_pack_only("format-consumer");
    assert_eq!(run(&c, &["add", "--as", "fleet", &src(&fleet)]).code, 0);

    let path = c.join(".adr/packs/fleet.pack");
    let formatted = fs::read_to_string(&path)
        .unwrap()
        .replace(
            "\nrecords:",
            "\n\n# tidied by somebody's pre-commit hook\nrecords:",
        )
        .replace("status: \"accepted\"", "status: 'accepted'");
    fs::write(&path, &formatted).unwrap();

    let got = run(&c, &["add", "--as", "fleet", &src(&fleet)]);
    assert_eq!(got.code, 0, "{}", got.all());
    assert!(got.out.contains("unchanged"), "{}", got.out);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        formatted,
        "a file that declares the same thing is not rewritten"
    );
    // And nobody edited anything, so nothing is reported as edited.
    let check = run(&c, &["add", "--check"]);
    assert_eq!(check.code, 0, "{}", check.all());
    assert!(check.out.contains("current"), "{}", check.out);
}

/// The other half: the declarations themselves were changed here. The revision
/// still names the recorded commit, so re-resolving alone would call it
/// current — which is how a decision nobody made survives in a consumer.
#[test]
fn add_check_reports_a_pack_whose_declarations_were_changed_here() {
    let fleet = producer("edit-fleet");
    let c = consumer_pack_only("edit-consumer");
    assert_eq!(run(&c, &["add", "--as", "fleet", &src(&fleet)]).code, 0);

    let path = c.join(".adr/packs/fleet.pack");
    let edited = fs::read_to_string(&path)
        .unwrap()
        .replace("\"@effect/sql\"", "\"Kysely\"");
    fs::write(&path, edited).unwrap();

    let got = run(&c, &["add", "--check"]);
    assert_eq!(got.code, 1, "{}", got.all());
    assert!(got.out.contains("edited"), "{}", got.out);
    assert!(
        got.out.contains("are not what"),
        "it has to say what is wrong with it: {}",
        got.out
    );
    assert!(got.out.contains("aval add"), "{}", got.out);

    // Re-running `add` restores what the source published, and says so.
    let fixed = run(&c, &["add", "--as", "fleet", &src(&fleet)]);
    assert_eq!(fixed.code, 0, "{}", fixed.all());
    assert!(fixed.out.contains("edited here"), "{}", fixed.out);
    assert!(fs::read_to_string(&path).unwrap().contains("@effect/sql"));
    assert_eq!(run(&c, &["add", "--check"]).code, 0);
}

#[test]
fn re_adding_replaces_rather_than_appending() {
    let fleet = producer("again-fleet");
    let c = consumer_pack_only("again-consumer");
    assert_eq!(run(&c, &["add", "--as", "fleet", &src(&fleet)]).code, 0);

    write(
        &fleet,
        "docs/adr/0009-more.md",
        &record(
            "ADR-0009",
            "release.trigger",
            Some("duro-stack"),
            "a merge",
            "first: true",
        ),
    );
    assert_eq!(run(&fleet, &["pack", "--write"]).code, 0);
    git(&fleet, &["add", "-A"]);
    git(&fleet, &["commit", "-qm", "another decision"]);

    assert_eq!(run(&c, &["add", "--as", "fleet", &src(&fleet)]).code, 0);
    let reg = fs::read_to_string(c.join(".adr.yaml")).unwrap();
    assert_eq!(
        reg.matches(".adr/packs/fleet.pack").count(),
        1,
        "one entry, not two: {}",
        reg
    );
    let vendored = fs::read_to_string(c.join(".adr/packs/fleet.pack")).unwrap();
    assert_eq!(vendored.matches("# commit:").count(), 1, "{}", vendored);
    assert!(vendored.contains("ADR-0009"), "{}", vendored);
    assert_eq!(
        run(&c, &["resolve", "release.trigger", "--scope", "duro-stack"]).code,
        0
    );
}

#[test]
fn a_second_source_may_not_quietly_take_an_occupied_name() {
    let a = producer("collide-a");
    let b = producer("collide-b");
    let c = consumer_pack_only("collide-consumer");
    assert_eq!(run(&c, &["add", "--as", "fleet", &src(&a)]).code, 0);
    let vendored = fs::read_to_string(c.join(".adr/packs/fleet.pack")).unwrap();

    let got = run(&c, &["add", "--as", "fleet", &src(&b)]);
    assert_eq!(got.code, 1, "{}", got.all());
    assert!(got.err.contains("--as"), "{}", got.err);
    assert_eq!(
        fs::read_to_string(c.join(".adr/packs/fleet.pack")).unwrap(),
        vendored,
        "the occupant must be untouched"
    );
}

// --- transport -------------------------------------------------------------

#[test]
fn an_annotated_tag_pins_end_to_end() {
    let fleet = producer("tag-fleet");
    git(&fleet, &["tag", "-a", "v1", "-m", "one"]);
    let c = consumer_pack_only("tag-consumer");
    let got = run(&c, &["add", &format!("{}@v1", src(&fleet))]);
    assert_eq!(got.code, 0, "{}", got.all());
    assert!(fs::read_to_string(c.join(".adr/packs/tag-fleet.pack"))
        .unwrap()
        .contains("# rev:    v1"));
}

/// A plain clone advertises `HEAD` and `refs/remotes/origin/HEAD` together.
/// Reading that as two answers refused the most natural offline source there
/// is: a checkout somebody already has.
#[test]
fn a_plain_clone_is_not_ambiguous() {
    let fleet = producer("clone-fleet");
    let clone = scratch("clone-copy");
    let _ = fs::remove_dir_all(&clone);
    let o = Command::new("git")
        .args(["clone", "-q", &src(&fleet), &src(&clone)])
        .output()
        .expect("clone");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));

    let c = consumer_pack_only("clone-consumer");
    let got = run(&c, &["add", &src(&clone)]);
    assert_eq!(got.code, 0, "{}", got.all());
}

#[test]
fn a_name_on_two_diverged_refs_is_ambiguous_and_names_both() {
    let fleet = producer("amb-fleet");
    git(&fleet, &["tag", "same"]);
    write(
        &fleet,
        "docs/adr/0010-x.md",
        &record(
            "ADR-0010",
            "release.trigger",
            Some("duro-stack"),
            "X",
            "first: true",
        ),
    );
    assert_eq!(run(&fleet, &["pack", "--write"]).code, 0);
    git(&fleet, &["add", "-A"]);
    git(&fleet, &["commit", "-qm", "diverge"]);
    git(&fleet, &["branch", "same"]);

    let c = consumer_pack_only("amb-consumer");
    let got = run(&c, &["add", &format!("{}@same", src(&fleet))]);
    assert_eq!(got.code, 1, "{}", got.all());
    assert!(got.err.contains("ambiguous"), "{}", got.err);
    assert!(got.err.contains("refs/heads/same"), "{}", got.err);
    assert!(got.err.contains("refs/tags/same"), "{}", got.err);
}

#[test]
fn usage_errors_exit_two() {
    let c = consumer_pack_only("usage-consumer");
    assert_eq!(run(&c, &["add"]).code, 2);
    assert_eq!(run(&c, &["add", "x", "--nope"]).code, 2);
    assert_eq!(run(&c, &["pack", "--write", "--check"]).code, 2);
}

/// A pack is data. Nothing in it is a command, nothing in it is run, and this
/// is the test that would fail if that ever stopped being true.
#[test]
fn a_pack_never_re_exports_what_it_borrowed() {
    let fleet = producer("chain-fleet");
    let middle = consumer_with_corpus("chain-middle");
    assert_eq!(
        run(&middle, &["add", "--as", "fleet", &src(&fleet)]).code,
        0
    );

    let published = run(&middle, &["pack"]);
    assert_eq!(published.code, 0, "{}", published.all());
    assert!(
        published.out.contains("app.router"),
        "its own decisions belong in it: {}",
        published.out
    );
    assert!(
        !published.out.contains("stack.sql-layer") && !published.out.contains("ADR-0002"),
        "a borrowed decision must not travel on: {}",
        published.out
    );
}

/// A consumer that also publishes publishes its own scopes only. The fleet's
/// `effect-stack` arrived with the vendored pack and is in the vocabulary in
/// force, but re-exporting it would hand this consumer's consumers a scope
/// from a pack they never vendored (SEMANTICS section 2.3).
#[test]
fn a_published_pack_does_not_re_export_vendored_scopes() {
    let p = producer("rexport-producer");
    let c = consumer_with_corpus("rexport-consumer");
    assert_eq!(run(&c, &["add", &src(&p)]).code, 0);
    assert_eq!(run(&c, &["heads", "--write"]).code, 0);
    assert_eq!(run(&c, &["pack", "--write"]).code, 0);
    let published = fs::read_to_string(c.join("aval.pack")).unwrap();
    let scopes = published.split("keys:").next().unwrap_or("");
    assert!(!scopes.contains("effect-stack"), "{}", published);

    // …unless this corpus itself decides at it: then the scope is part of
    // what it publishes, or its own record would name an undeclared scope.
    write(
        &c,
        "docs/adr/0002-sql-here.md",
        &record(
            "ADR-0002",
            "app.router",
            Some("effect-stack"),
            "React Router, here too",
            "first: true",
        ),
    );
    assert_eq!(run(&c, &["heads", "--write"]).code, 0);
    assert_eq!(run(&c, &["pack", "--write"]).code, 0);
    let published = fs::read_to_string(c.join("aval.pack")).unwrap();
    let scopes = published.split("keys:").next().unwrap_or("");
    assert!(scopes.contains("effect-stack"), "{}", published);
}

/// Adopting the fleet is `aval add` in a repository that has no registry yet.
/// It used to refuse; it now writes a starter registry at the git top level —
/// not in the subdirectory it was run from — and lists the pack in it, so
/// the corpus resolves straight away.
#[test]
fn add_creates_the_registry_it_vendors_into() {
    let p = producer("starter-producer");
    // Under the system temp dir, not `target/`: this repository has a
    // registry of its own, and walking up from `target/` would find it.
    let c = std::env::temp_dir().join(format!("aval-starter-consumer-{}", std::process::id()));
    let _ = fs::remove_dir_all(&c);
    fs::create_dir_all(c.join("src/deep")).unwrap();
    git_init(&c);

    let dry = run(&c.join("src/deep"), &["add", &src(&p), "--dry-run"]);
    assert_eq!(dry.code, 0, "{}", dry.all());
    assert!(dry.all().contains("would be created"), "{}", dry.all());
    assert!(!c.join(".adr.yaml").exists());

    let got = run(&c.join("src/deep"), &["add", &src(&p)]);
    assert_eq!(got.code, 0, "{}", got.all());
    assert!(got.all().contains("created"), "{}", got.all());
    assert!(c.join(".adr.yaml").is_file());
    assert!(!c.join("src/deep/.adr.yaml").exists());
    let reg = fs::read_to_string(c.join(".adr.yaml")).unwrap();
    assert!(reg.contains(".adr/packs/"), "{}", reg);
    assert_eq!(run(&c, &["check"]).code, 0);
    assert_eq!(run(&c, &["resolve", "release.trigger"]).code, 0);
}
