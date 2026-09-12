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
//! run. A pack is inert: nothing in it executes, ever. What it can do is change
//! an answer, and the place to catch that is the pull request that adds the
//! file — which is where a change of architectural direction belongs.
//!
//! The commit id recorded in the banner is provenance, not authority. It says
//! the bytes are the ones that repository published. It says nothing about
//! whether the decisions are good ones, and nothing reads it back except
//! `--check`.

use crate::fetch::{self, Scratch, Source};
use crate::load::{self, pack_name};
use aval_core::pack;
use std::path::{Path, PathBuf};

/// Where vendored packs live. One directory, so a reader can see every
/// borrowed decision at once.
pub const DIR: &str = ".adr/packs";

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

    let rel = format!("{}/{}.yaml", DIR, name);
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

/// What writing this one would do, without doing it.
#[derive(Debug)]
pub enum Plan {
    New,
    /// Same source, same bytes.
    Unchanged,
    /// Same source, different bytes.
    Update(String),
    /// A different source already holds this name.
    Collision(String),
}

pub fn plan(root: &Path, v: &Vendored) -> std::io::Result<Plan> {
    let path = root.join(&v.rel);
    let Ok(have) = std::fs::read_to_string(&path) else {
        return Ok(Plan::New);
    };
    match provenance(&have) {
        Some((src, _, _)) if src != v.source.label => Ok(Plan::Collision(src)),
        Some((_, _, id)) if id == v.id && have == body_with_banner(v) => Ok(Plan::Unchanged),
        Some((_, _, id)) => Ok(Plan::Update(id)),
        // No banner: somebody's own file, or one written by hand. Treat it as a
        // collision rather than replacing it, for the same reason an unreadable
        // settings file stops `hook install`.
        None => Ok(Plan::Collision("an unmarked file".into())),
    }
}

pub fn body_with_banner(v: &Vendored) -> String {
    format!("{}{}", banner(&v.source, &v.id), v.body)
}

/// Write the pack and register it. Callers have already planned every source.
pub fn write(root: &Path, v: &Vendored) -> Result<bool, String> {
    let path: PathBuf = root.join(&v.rel);
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {}", d.display(), e))?;
    }
    std::fs::write(&path, body_with_banner(v)).map_err(|e| format!("{}: {}", path.display(), e))?;
    register(root, &v.rel)
}

/// Add the pack to the registry's `packs:` list, if it is not already there.
///
/// A textual edit rather than a re-serialisation, because `.adr.yaml` is a file
/// a person wrote and rewriting it would lose their comments and their
/// ordering to make room for one line.
fn register(root: &Path, rel: &str) -> Result<bool, String> {
    let path = root.join(load::REGISTRY);
    let src = std::fs::read_to_string(&path).map_err(|e| format!("{}: {}", load::REGISTRY, e))?;
    if src
        .lines()
        .any(|l| l.trim_start().starts_with('-') && l.contains(rel))
    {
        return Ok(false);
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
        if line.trim_start() == "packs:" || line.trim_start().starts_with("packs:") {
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
    Ok(true)
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
pub fn standing(o: &Origin) -> Standing {
    let spec = format!("{}@{}", o.source, o.rev);
    let Ok(source) = fetch::parse_source(&spec) else {
        return Standing::Unmarked;
    };
    match fetch::resolve(&source) {
        Ok(id) if id == o.commit => Standing::Current,
        Ok(id) => Standing::Behind(id),
        Err(e) => Standing::Unknown(e),
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
}
