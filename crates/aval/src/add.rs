//! `aval add` — vendor another repository's declarations into this one.
//!
//! Everything is resolved, fetched and validated before anything is written.
//! A half-applied add leaves a registry pointing at a pack that is not there,
//! or a pack whose records are partly missing — and a missing record is the
//! worst failure this tool has, because whatever it superseded comes back as a
//! head and `resolve` answers `active` with a decision that was replaced.
//!
//! There is no confirmation prompt and no per-machine consent step. That is
//! the deliberate difference from `amont add`, which vendors commands and
//! therefore has to take consent again on every machine before any of them can
//! run. Nothing in a pack executes, ever. What it can do is change an answer,
//! and the place to catch that is the pull request that adds the file — which
//! is where a change of architectural direction belongs.
//!
//! "Inert" scopes to EXECUTION. A pack's text reaches an agent's context by way
//! of the hook and `aval mcp`, and text in a context is the one input that
//! acts. SEMANTICS section 2.3 says what holds that down: the surfaces name the
//! text as data, and section 3.7 refuses a value whose printed form could
//! differ from its reviewed form.
//!
//! The commit id recorded in the banner is provenance, not authority. It says
//! the bytes are the ones that repository published. It says nothing about
//! whether the decisions are good ones, and nothing reads it back except
//! `--check`.

use crate::fetch::{self, Scratch, Source};
use crate::load::{self, pack_name};
use aval_core::{pack, yaml};
use std::path::{Path, PathBuf};

/// Where vendored packs live. One directory, so a reader can see every
/// borrowed decision at once.
pub const DIR: &str = ".adr/packs";

/// The extension a vendored pack is written with, matching the `aval.pack` the
/// producer publishes.
///
/// Not `.yaml`, and the reason is not taste. A consumer's pre-commit hook runs
/// a formatter over every YAML file it can name, and one that requoted a
/// vendored pack turned a generated file into a hand-edited one in three
/// repositories in an afternoon. 1.2.1 answered by emitting the quoting
/// prettier would choose, which is the wrong layer: a second formatter would
/// disagree. An extension no formatter claims settles it for all of them.
///
/// A `.yaml` written before 1.3 still loads — a `packs:` entry is a literal
/// path and no check reads the extension — and `aval add` migrates it.
pub const EXT: &str = "pack";

#[derive(Debug)]
pub struct Vendored {
    pub name: String,
    pub rel: String,
    pub source: Source,
    pub id: String,
    pub body: String,
    /// Records the pack declares, for the preview.
    pub records: usize,
    pub keys: Vec<String>,
}

/// The provenance header. Comments, so the parser skips them and the file is
/// still exactly the pack the producer published plus a note saying where it
/// came from.
fn banner(s: &Source, id: &str) -> String {
    format!(
        "# Vendored by `aval add`. Do not edit — re-run the command to update.\n\
         # source: {}\n# rev:    {}\n# commit: {}\n",
        s.label, s.rev, id
    )
}

/// Read back what a vendored file says about its own origin.
fn provenance(text: &str) -> Option<(String, String, String)> {
    let field = |k: &str| -> Option<String> {
        text.lines()
            .take_while(|l| l.starts_with('#'))
            .find_map(|l| {
                l.trim_start_matches('#')
                    .trim()
                    .strip_prefix(k)?
                    .trim()
                    .to_string()
                    .into()
            })
    };
    Some((field("source:")?, field("rev:")?, field("commit:")?))
}

pub fn resolve_one(spec: &str, as_name: Option<&str>) -> Result<Vendored, String> {
    let source = fetch::parse_source(spec)?;
    let id = fetch::resolve(&source)?;

    let name = match as_name {
        Some(n) => n.to_string(),
        // The last segment of the source, which is the repository's own name
        // and is what a person would call it. `--as` exists for the case where
        // two forges hold a `decisions` each.
        None => source
            .label
            .rsplit(['/', ':'])
            .next()
            .unwrap_or("pack")
            .trim_end_matches(".git")
            .to_string(),
    };
    if let Some(m) = pack::bad_name(&name) {
        return Err(format!("{} — name it with `--as <name>`", m));
    }

    let scratch = Scratch::new(&name);
    let body = fetch::fetch(&source, &id, &scratch.0, pack::FILE)?;

    let rel = format!("{}/{}.{}", DIR, name, EXT);
    // Parse before writing, with the parser the loader itself uses. A pack
    // that read here and not there would be a corpus nobody intended.
    let parsed = pack::parse(&rel, &name, &body).map_err(|f| {
        let mut m = format!(
            "{}: the pack at {} is not readable",
            source.label,
            fetch::short(&id)
        );
        for x in f.iter().take(5) {
            m.push_str(&format!("\n  {}", x));
        }
        m
    })?;

    Ok(Vendored {
        name,
        rel,
        records: parsed.adrs.len(),
        keys: parsed.keys.iter().map(|k| k.name.clone()).collect(),
        source,
        id,
        body,
    })
}

/// Two of these that would land on one path, if any: the earlier and the later.
///
/// Checked before planning, because `plan` asks the disk and the disk does not
/// yet hold what an earlier source in the same command is about to write.
pub fn same_destination(vs: &[Vendored]) -> Option<(&Vendored, &Vendored)> {
    vs.iter().enumerate().find_map(|(i, later)| {
        vs[..i]
            .iter()
            .find(|earlier| earlier.rel == later.rel)
            .map(|earlier| (earlier, later))
    })
}

/// What writing this one would do, without doing it.
#[derive(Debug)]
pub enum Plan {
    New,
    /// Same source, and the file already declares what the producer published.
    Unchanged,
    /// Same source, different declarations. Carries the commit that was there.
    Update(String),
    /// A pack vendored by an earlier version, as `<name>.yaml`, from this same
    /// source. The file moves to `<name>.pack` and its registry line with it.
    Migrate {
        /// The `.yaml` path, which is deleted and unlisted.
        from: String,
        /// The commit that file records, which the move does not change.
        had: String,
    },
    /// Something else already holds this name.
    Collision {
        /// The file that holds it, which may be the `.yaml` of an earlier
        /// version — naming the `.pack` we were about to write would send the
        /// reader to open a file that is not there.
        rel: String,
        /// Where it came from, or that nothing says.
        other: String,
    },
}

/// Whether two pack texts **declare** the same thing.
///
/// Not a byte comparison, and that is the whole of §2.3's revision: a vendored
/// pack sits in repositories that run a formatter at pre-commit, and a file
/// requoted by one still says exactly what the producer published. Comparing
/// bytes made such a file "changed", so `add` rewrote it on every run and
/// `--check` would have called it edited by nobody.
///
/// A text that will not parse is not equal to anything. `resolve_one` has
/// already parsed the fetched side, so this only ever fails on the vendored
/// one — a file somebody broke by hand, which is exactly the file that should
/// be rewritten rather than left alone.
fn declares_the_same(a: &str, b: &str) -> bool {
    match (yaml::parse(a), yaml::parse(b)) {
        (Ok(x), Ok(y)) => yaml::same_values(&x, &y),
        _ => false,
    }
}

pub fn plan(root: &Path, v: &Vendored) -> std::io::Result<Plan> {
    if let Ok(have) = std::fs::read_to_string(root.join(&v.rel)) {
        return Ok(match provenance(&have) {
            Some((src, _, _)) if src != v.source.label => Plan::Collision {
                rel: v.rel.clone(),
                other: src,
            },
            Some((_, _, id)) if id == v.id && declares_the_same(&have, &v.body) => Plan::Unchanged,
            Some((_, _, id)) => Plan::Update(id),
            // No banner: somebody's own file, or one written by hand. Treat it
            // as a collision rather than replacing it, for the same reason an
            // unreadable settings file stops `hook install`.
            None => Plan::Collision {
                rel: v.rel.clone(),
                other: "an unmarked file".into(),
            },
        });
    }

    // No `.pack`. A `.yaml` under the same name is what every consumer of 1.2
    // and earlier has, and it is this pack rather than a second one — so it is
    // moved, not duplicated. Leaving it would vendor the same decisions twice
    // under two names, which is two heads for one slot.
    let from = format!("{}/{}.yaml", DIR, v.name);
    let Ok(old) = std::fs::read_to_string(root.join(&from)) else {
        return Ok(Plan::New);
    };
    Ok(match provenance(&old) {
        Some((src, _, _)) if src != v.source.label => Plan::Collision {
            rel: from,
            other: src,
        },
        Some((_, _, id)) => Plan::Migrate { from, had: id },
        None => Plan::Collision {
            rel: from,
            other: "an unmarked file".into(),
        },
    })
}

pub fn body_with_banner(v: &Vendored) -> String {
    format!("{}{}", banner(&v.source, &v.id), v.body)
}

/// What writing one pack did to the registry.
#[derive(Debug, PartialEq, Eq)]
pub enum Listed {
    /// The path was already listed.
    Already,
    /// A line was added to `packs:`.
    Added,
    /// The line naming the old path now names the new one.
    Relisted,
}

/// Write the pack and register it. Callers have already planned every source.
///
/// The plan is passed in rather than recomputed: a migration deletes a file and
/// rewrites a registry line, and a second look at the disk between planning and
/// writing is a second answer this command could get.
pub fn write(root: &Path, v: &Vendored, p: &Plan) -> Result<Listed, String> {
    let path: PathBuf = root.join(&v.rel);
    // An unchanged pack is left exactly as it is. It already declares what the
    // producer published, and rewriting it would undo the formatter that ran
    // over it — every run, forever, which is the loop this release ends.
    if !matches!(p, Plan::Unchanged) {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {}", d.display(), e))?;
        }
        std::fs::write(&path, body_with_banner(v))
            .map_err(|e| format!("{}: {}", path.display(), e))?;
    }
    let Plan::Migrate { from, .. } = p else {
        return register(root, &v.rel);
    };
    // The new file first, the old one only once it is safely there: the failure
    // to avoid is a registry naming a pack that no longer exists.
    let old = root.join(from);
    std::fs::remove_file(&old).map_err(|e| format!("{}: {}", old.display(), e))?;
    relist(root, from, &v.rel)
}

/// The `packs:` entry of a registry, as the text a person wrote it.
///
/// Two shapes are legal YAML and both occur: a block list, one `- path` per
/// line, and an inline list, `packs: [a, b]` or the `packs: []` a registry
/// starts with. An edit that assumed the block shape once appended an
/// indented item under `packs: []`, which `add` accepted and every later read
/// refused with "unexpected indentation" — exit 3, from a command that had
/// exited 0.
enum PacksShape {
    /// No `packs:` at all.
    Absent,
    /// `packs:` heading a block list.
    Block,
    /// `packs: [ ... ]` on one line: the index, the items, and everything after
    /// the closing bracket (a trailing comment, usually).
    Inline {
        line: usize,
        items: Vec<String>,
        suffix: String,
    },
}

/// Where the value of a `key:` line ends and its comment begins.
fn before_comment(rest: &str) -> &str {
    let mut prev = ' ';
    for (i, c) in rest.char_indices() {
        if c == '#' && prev.is_whitespace() {
            return &rest[..i];
        }
        prev = c;
    }
    rest
}

fn packs_shape(src: &str) -> Result<PacksShape, String> {
    for (i, line) in src.lines().enumerate() {
        let Some(rest) = line.strip_prefix("packs:") else {
            continue;
        };
        let value = before_comment(rest).trim();
        if value.is_empty() {
            return Ok(PacksShape::Block);
        }
        let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) else {
            return Err(format!(
                "{}: `packs:` holds `{}`, which is not a list",
                load::REGISTRY,
                value
            ));
        };
        let items = inner
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_matches(|c| c == '"' || c == '\'').to_string())
            .collect();
        let close = rest.rfind(']').map_or(rest.len(), |j| j + 1);
        return Ok(PacksShape::Inline {
            line: i,
            items,
            suffix: rest[close..].to_string(),
        });
    }
    Ok(PacksShape::Absent)
}

/// One line of the registry, changed; every other byte as it was.
fn replace_line(src: &str, index: usize, with: &str) -> String {
    let mut out = String::new();
    for (i, line) in src.lines().enumerate() {
        out.push_str(if i == index { with } else { line });
        out.push('\n');
    }
    out
}

fn inline_line(items: &[String], suffix: &str) -> String {
    format!("packs: [{}]{}", items.join(", "), suffix)
}

/// Point the registry's `packs:` line at the migrated path.
///
/// Textual, like `register`, and for the same reason. Only the one line moves;
/// every other byte of the file a person wrote is left as they wrote it.
fn relist(root: &Path, from: &str, to: &str) -> Result<Listed, String> {
    let path = root.join(load::REGISTRY);
    let src = std::fs::read_to_string(&path).map_err(|e| format!("{}: {}", load::REGISTRY, e))?;
    let item = |l: &str, rel: &str| l.trim_start().starts_with('-') && l.contains(rel);

    if let PacksShape::Inline {
        line,
        mut items,
        suffix,
    } = packs_shape(&src)?
    {
        if !items.iter().any(|i| i == from) {
            return register(root, to);
        }
        // Drop rather than rename when the destination is already listed:
        // a pack listed twice is a name collision at load.
        items.retain(|i| i != from);
        if !items.iter().any(|i| i == to) {
            items.push(to.to_string());
        }
        let out = replace_line(&src, line, &inline_line(&items, &suffix));
        std::fs::write(&path, out).map_err(|e| format!("{}: {}", load::REGISTRY, e))?;
        return Ok(Listed::Relisted);
    }

    if !src.lines().any(|l| item(l, from)) {
        // Nothing named the old path — a registry somebody edited by hand.
        // Registering the new one is still the right end state.
        return register(root, to);
    }
    // A registry that already names the destination would otherwise end up
    // naming it twice, and a pack listed twice is a name collision at load.
    let already = src.lines().any(|l| item(l, to));
    let mut out = String::new();
    for line in src.lines() {
        if item(line, from) {
            if already {
                continue;
            }
            out.push_str(&line.replace(from, to));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    std::fs::write(&path, out).map_err(|e| format!("{}: {}", load::REGISTRY, e))?;
    Ok(Listed::Relisted)
}

/// Add the pack to the registry's `packs:` list, if it is not already there.
///
/// A textual edit rather than a re-serialisation, because `.adr.yaml` is a file
/// a person wrote and rewriting it would lose their comments and their
/// ordering to make room for one line. An inline list stays inline and a block
/// list stays a block: the shape is theirs too.
/// What `aval add` writes when a repository has no registry yet. Everything
/// else is the repository's to declare; `add` then lists the pack below it.
pub const STARTER_REGISTRY: &str = "\
# The decision registry. `aval add` created it to vendor a pack of decisions
# made elsewhere; this repository keeps no records of its own yet.
#
# Declare `keys` (and a `dir` for records) the day it has a question the pack
# does not answer, and `areas:` for which parts of it are a command line or a
# UI — `aval traits --detect` proposes them.
scopes: []
keys:
";

fn register(root: &Path, rel: &str) -> Result<Listed, String> {
    let path = root.join(load::REGISTRY);
    let src = std::fs::read_to_string(&path).map_err(|e| format!("{}: {}", load::REGISTRY, e))?;

    let shape = packs_shape(&src)?;
    if let PacksShape::Inline {
        line,
        mut items,
        suffix,
    } = shape
    {
        if items.iter().any(|i| i == rel) {
            return Ok(Listed::Already);
        }
        items.push(rel.to_string());
        let out = replace_line(&src, line, &inline_line(&items, &suffix));
        std::fs::write(&path, out).map_err(|e| format!("{}: {}", load::REGISTRY, e))?;
        return Ok(Listed::Added);
    }

    if src
        .lines()
        .any(|l| l.trim_start().starts_with('-') && l.contains(rel))
    {
        return Ok(Listed::Already);
    }

    let mut out = String::new();
    let mut placed = false;
    let mut in_packs = false;
    for line in src.lines() {
        // The end of an existing `packs:` block: anything that is not one of
        // its items.
        if in_packs && !line.trim_start().starts_with('-') && !line.trim().is_empty() {
            out.push_str(&format!("  - {}\n", rel));
            placed = true;
            in_packs = false;
        }
        out.push_str(line);
        out.push('\n');
        if line.starts_with("packs:") {
            in_packs = true;
        }
    }
    if in_packs {
        out.push_str(&format!("  - {}\n", rel));
        placed = true;
    }
    if !placed {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&format!("packs:\n  - {}\n", rel));
    }
    std::fs::write(&path, out).map_err(|e| format!("{}: {}", load::REGISTRY, e))?;
    Ok(Listed::Added)
}

/// What a vendored pack's banner says about where it came from.
#[derive(Debug)]
pub struct Origin {
    pub name: String,
    pub rel: String,
    pub source: String,
    pub rev: String,
    pub commit: String,
}

/// Whether a vendored pack is still the one its revision names.
#[derive(Debug)]
pub enum Standing {
    Current,
    /// The revision now names a different commit.
    Behind(String),
    /// The revision still names the recorded commit, but the file no longer
    /// declares what that commit published.
    Edited,
    /// The revision could not be resolved: offline, moved, or access lost.
    Unknown(String),
    /// No banner, so nothing says where it came from.
    Unmarked,
}

pub fn origins(root: &Path, rels: &[String]) -> Vec<Result<Origin, String>> {
    rels.iter()
        .map(|rel| {
            let text =
                std::fs::read_to_string(root.join(rel)).map_err(|e| format!("{}: {}", rel, e))?;
            let (source, rev, commit) = provenance(&text).ok_or_else(|| {
                format!(
                    "{}: no `# source:` banner, so nothing says where it came from",
                    rel
                )
            })?;
            Ok(Origin {
                name: pack_name(rel).to_string(),
                rel: rel.clone(),
                source,
                rev,
                commit,
            })
        })
        .collect()
}

/// Re-resolve one pack's revision and say whether it still names what was
/// vendored.
///
/// Human-run, and never on a hook or a gate path. It reaches the network, and
/// a check that needed the network would fail on a plane, in a CI job with no
/// credential for the source, and in every repository whose forge cannot see
/// the other one — which is the situation this whole mechanism exists for.
pub fn standing(root: &Path, o: &Origin) -> Standing {
    let spec = format!("{}@{}", o.source, o.rev);
    let Ok(source) = fetch::parse_source(&spec) else {
        return Standing::Unmarked;
    };
    match fetch::resolve(&source) {
        Ok(id) if id == o.commit => unedited(root, o, &source),
        Ok(id) => Standing::Behind(id),
        Err(e) => Standing::Unknown(e),
    }
}

/// The revision still names the commit that was vendored. Whether the file
/// still says what that commit said is the second question, and the one a
/// re-resolution alone never asked.
///
/// Only asked of a `Current` pack, on purpose. A `Behind` one is being replaced
/// whatever its content says, and an `Unknown` one cannot be fetched to compare
/// against — reporting both would be two lines about one re-vendoring.
///
/// Declarations, never bytes: the vendored file lives where formatters run, and
/// one that reindented it has edited nothing. What `edited` means is that
/// somebody changed what this repository believes another repository decided.
fn unedited(root: &Path, o: &Origin, source: &Source) -> Standing {
    let scratch = Scratch::new(&o.name);
    let published = match fetch::fetch(source, &o.commit, &scratch.0, pack::FILE) {
        Ok(p) => p,
        Err(e) => return Standing::Unknown(e),
    };
    match std::fs::read_to_string(root.join(&o.rel)) {
        Ok(have) if declares_the_same(&have, &published) => Standing::Current,
        Ok(_) => Standing::Edited,
        // `origins` read this file a moment ago, so this is a race or a
        // permission change. Not an answer, and not reported as one.
        Err(e) => Standing::Unknown(format!("{}: {}", o.rel, e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_banner_round_trips() {
        let s = fetch::parse_source("github:acme/decisions@v1").unwrap();
        let text = format!("{}aval: \"0.5.0\"\n", banner(&s, "abc123"));
        let (src, rev, id) = provenance(&text).expect("reads back");
        assert_eq!(src, "github:acme/decisions");
        assert_eq!(rev, "v1");
        assert_eq!(id, "abc123");
    }

    #[test]
    fn a_file_with_no_banner_reads_as_unmarked() {
        assert!(provenance("aval: \"0.5.0\"\n").is_none());
    }

    // --- planning against what is on disk -----------------------------------

    const BODY: &str = "\
aval: \"1.3.0\"
scopes: []
keys:
  \"stack.sql-layer\":
    description: \"The SQL access layer\"
records:
  - id: \"ADR-0002\"
    status: \"accepted\"
    decisions:
      - key: \"stack.sql-layer\"
        choice: \"@effect/sql\"
        first: true
";

    /// The registry a person wrote: a comment, a trailing comment on the very
    /// line a migration rewrites, and a second pack that must not move.
    const REGISTRY: &str = "\
# ours
dir: docs/adr
packs:
  - .adr/packs/fleet.yaml # the fleet's
  - .adr/packs/other.pack
scopes: []
keys:
";

    /// A scratch root under the system temp directory. Never under this
    /// worktree: the repository is itself a corpus, and a fixture registry
    /// inside it is one a corpus walk could reach.
    fn root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("aval-add-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(p.join(DIR)).expect("mkdir");
        std::fs::write(p.join(load::REGISTRY), REGISTRY).expect("registry");
        p
    }

    fn vendored(spec: &str) -> Vendored {
        let source = fetch::parse_source(spec).expect("a source");
        Vendored {
            name: "fleet".into(),
            rel: format!("{}/fleet.{}", DIR, EXT),
            source,
            id: "c0ffee1234567890".into(),
            body: BODY.to_string(),
            records: 1,
            keys: vec!["stack.sql-layer".into()],
        }
    }

    /// What 1.2 left behind: the same pack, from the same source, as `.yaml`.
    fn old_yaml(root: &Path, v: &Vendored, id: &str) {
        std::fs::write(
            root.join(format!("{}/fleet.yaml", DIR)),
            format!("{}{}", banner(&v.source, id), v.body),
        )
        .expect("write");
    }

    #[test]
    fn a_pack_the_last_version_wrote_is_migrated_not_duplicated() {
        let r = root("migrate");
        let v = vendored("github:acme/decisions");
        old_yaml(&r, &v, &v.id);

        let Plan::Migrate { from, had } = plan(&r, &v).expect("planned") else {
            panic!("a same-source .yaml is the file to move, not a second pack");
        };
        assert_eq!(from, ".adr/packs/fleet.yaml");
        assert_eq!(had, v.id);

        let p = Plan::Migrate { from, had };
        assert_eq!(write(&r, &v, &p).expect("written"), Listed::Relisted);
        assert!(r.join(&v.rel).is_file());
        assert!(
            !r.join(".adr/packs/fleet.yaml").exists(),
            "leaving it would vendor the same decisions twice, under two names"
        );
        assert_eq!(
            std::fs::read_to_string(r.join(load::REGISTRY)).unwrap(),
            REGISTRY.replace("fleet.yaml", "fleet.pack"),
            "only the one path moves; the comment and the other entry are the \
             bytes the person wrote"
        );
        // And the migrated file is now what any later run compares against.
        assert!(matches!(plan(&r, &v).expect("planned"), Plan::Unchanged));
    }

    #[test]
    fn a_yaml_from_somewhere_else_is_still_a_collision() {
        let r = root("collide");
        let v = vendored("github:acme/decisions");
        let other = vendored("github:other/decisions");
        old_yaml(&r, &other, "0123456789abcdef");
        let Plan::Collision { rel, other } = plan(&r, &v).expect("planned") else {
            panic!("another source's file is not this pack's to move");
        };
        assert_eq!(rel, ".adr/packs/fleet.yaml", "the file the reader opens");
        assert_eq!(other, "github:other/decisions");
    }

    #[test]
    fn an_unmarked_yaml_is_a_collision_rather_than_something_to_move() {
        let r = root("unmarked");
        let v = vendored("github:acme/decisions");
        std::fs::write(r.join(".adr/packs/fleet.yaml"), BODY).expect("write");
        let Plan::Collision { rel, other } = plan(&r, &v).expect("planned") else {
            panic!("nothing says where it came from, so nothing says it is ours");
        };
        assert_eq!(rel, ".adr/packs/fleet.yaml");
        assert_eq!(other, "an unmarked file");
    }

    #[test]
    fn a_formatter_that_went_over_the_file_changed_nothing() {
        let r = root("formatted");
        let v = vendored("github:acme/decisions");
        // A formatter's output for this file: requoted values, blank lines
        // between blocks, and a note of its own. Every declaration is the one
        // the producer published.
        let formatted = format!(
            "{}{}",
            banner(&v.source, &v.id),
            "# formatted\n\naval: '1.3.0'\nscopes: []\n\nkeys:\n  \"stack.sql-layer\":\n    \
             description: 'The SQL access layer'\n\nrecords:\n  - id: 'ADR-0002'\n    \
             status: 'accepted'\n    decisions:\n      - key: 'stack.sql-layer'\n        \
             choice: '@effect/sql'\n        first: true\n"
        );
        std::fs::write(r.join(&v.rel), &formatted).expect("write");

        assert!(matches!(plan(&r, &v).expect("planned"), Plan::Unchanged));
        assert_eq!(
            write(&r, &v, &Plan::Unchanged).expect("written"),
            Listed::Added
        );
        assert_eq!(
            std::fs::read_to_string(r.join(&v.rel)).unwrap(),
            formatted,
            "an unchanged pack is left alone; rewriting it would fight the \
             formatter on every run"
        );
    }

    #[test]
    fn a_changed_declaration_is_an_update_even_at_the_same_commit() {
        let r = root("edited");
        let v = vendored("github:acme/decisions");
        std::fs::write(
            r.join(&v.rel),
            format!(
                "{}{}",
                banner(&v.source, &v.id),
                BODY.replace("@effect/sql", "Kysely")
            ),
        )
        .expect("write");
        let Plan::Update(had) = plan(&r, &v).expect("planned") else {
            panic!("a decision nobody published is not unchanged");
        };
        assert_eq!(had, v.id, "the commit is the same; the file is not");
    }

    /// The registry a fresh consumer starts with, and the shape `register`
    /// once corrupted by appending a block item under it.
    fn root_with(tag: &str, registry: &str) -> PathBuf {
        let r = root(tag);
        std::fs::write(r.join(load::REGISTRY), registry).expect("registry");
        r
    }

    fn registry_text(r: &Path) -> String {
        std::fs::read_to_string(r.join(load::REGISTRY)).unwrap()
    }

    #[test]
    fn an_inline_packs_list_stays_inline_and_stays_valid() {
        let r = root_with("inline-empty", "dir: docs/adr\npacks: []\nkeys:\n");
        let v = vendored("github:acme/decisions");
        assert_eq!(write(&r, &v, &Plan::New).expect("written"), Listed::Added);
        let text = registry_text(&r);
        assert_eq!(
            text,
            "dir: docs/adr\npacks: [.adr/packs/fleet.pack]\nkeys:\n"
        );
        // What was written must read back: this is the exact shape that used
        // to come back as "unexpected indentation" on the next command.
        aval_core::parse::registry(load::REGISTRY, &text).expect("a registry that reads back");
        assert_eq!(register(&r, &v.rel).expect("again"), Listed::Already);
    }

    #[test]
    fn an_inline_list_with_items_and_a_comment_keeps_both() {
        let r = root_with(
            "inline-items",
            "dir: docs/adr\npacks: [.adr/packs/other.pack]  # theirs\nkeys:\n",
        );
        let v = vendored("github:acme/decisions");
        register(&r, &v.rel).expect("registered");
        assert_eq!(
            registry_text(&r),
            "dir: docs/adr\npacks: [.adr/packs/other.pack, .adr/packs/fleet.pack]  # theirs\nkeys:\n"
        );
    }

    #[test]
    fn a_migration_moves_an_inline_entry_too() {
        let r = root_with(
            "inline-migrate",
            "dir: docs/adr\npacks: [.adr/packs/fleet.yaml, .adr/packs/other.pack]\nkeys:\n",
        );
        let v = vendored("github:acme/decisions");
        old_yaml(&r, &v, &v.id);
        let p = plan(&r, &v).expect("planned");
        assert!(matches!(p, Plan::Migrate { .. }));
        assert_eq!(write(&r, &v, &p).expect("written"), Listed::Relisted);
        assert_eq!(
            registry_text(&r),
            "dir: docs/adr\npacks: [.adr/packs/other.pack, .adr/packs/fleet.pack]\nkeys:\n"
        );
        assert!(!r.join(format!("{}/fleet.yaml", DIR)).exists());
    }

    #[test]
    fn a_packs_scalar_is_refused_not_edited() {
        let r = root_with("scalar", "dir: docs/adr\npacks: nope\nkeys:\n");
        let v = vendored("github:acme/decisions");
        let err = register(&r, &v.rel).expect_err("refused");
        assert!(err.contains("not a list"), "{}", err);
        assert_eq!(registry_text(&r), "dir: docs/adr\npacks: nope\nkeys:\n");
    }

    #[test]
    fn two_sources_landing_on_one_path_are_caught_before_planning() {
        let a = vendored("github:acme/fleet");
        let b = vendored("github:other/fleet");
        assert!(same_destination(&[a]).is_none());
        let both = [vendored("github:acme/fleet"), b];
        let (first, second) = same_destination(&both).expect("a collision");
        assert_eq!(first.source.label, "github:acme/fleet");
        assert_eq!(second.source.label, "github:other/fleet");
    }

    #[test]
    fn a_pack_that_is_not_there_yet_is_new() {
        let r = root("new");
        let v = vendored("github:acme/decisions");
        assert!(matches!(plan(&r, &v).expect("planned"), Plan::New));
        assert_eq!(write(&r, &v, &Plan::New).expect("written"), Listed::Added);
        assert!(std::fs::read_to_string(r.join(load::REGISTRY))
            .unwrap()
            .contains("- .adr/packs/fleet.pack"));
    }
}
