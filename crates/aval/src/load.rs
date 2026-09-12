//! Reading a corpus off disk.
//!
//! All filesystem access lives here. `aval-core` stays pure, so everything
//! below hands it strings and gets findings back.

use aval_core::graph::Graph;
use aval_core::model::{Corpus, Finding, KeyDef, Layer};
use aval_core::pack::{self, Pack};
use aval_core::parse;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

pub const REGISTRY: &str = ".adr.yaml";
pub const HEADS: &str = "HEADS.md";

#[derive(Debug)]
pub struct Loaded {
    pub graph: Graph,
    /// Directory holding the registry.
    pub root: PathBuf,
    /// Directory holding the ADR files. Meaningless when the registry declares
    /// no `dir`; ask `registry().has_dir()` before joining onto it, or an empty
    /// `dir` turns this into the repository root and every caller starts
    /// reading files that are nothing to do with the corpus.
    pub adr_dir: PathBuf,
    /// `(repository-relative path, source)` for every record read, for Layer
    /// C. A path rather than a basename: two sources may hold the same
    /// filename, and the Layer C join would silently never match.
    ///
    /// Vendored records are deliberately absent. Layer C is about this
    /// repository — whether its citations resolve, whether its documents state
    /// a status twice — and a pack is another repository's business, already
    /// checked where it was written. Keeping it out of this list is what makes
    /// every Layer C check skip it without knowing packs exist.
    pub files: Vec<(String, String)>,
    /// Vendored packs, in registry order.
    pub packs: Vec<Pack>,
}

#[derive(Debug)]
pub enum LoadError {
    /// No registry anywhere above the working directory.
    NoRegistry(PathBuf),
    /// Could not read something that must be readable.
    Unreadable(String),
    /// The corpus is structurally invalid: Layer A.
    Invalid(Vec<Finding>),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::NoRegistry(p) => write!(
                f,
                "no {} in {} or any parent directory",
                REGISTRY,
                p.display()
            ),
            LoadError::Unreadable(m) => f.write_str(m),
            LoadError::Invalid(_) => f.write_str(
                "the corpus is structurally invalid; no question can be answered against it",
            ),
        }
    }
}

impl std::error::Error for LoadError {}

impl LoadError {
    /// The Layer A findings behind an `Invalid`; empty for the other variants.
    ///
    /// Paired with `Display` so a caller renders a load failure without
    /// matching the variants itself. Two callers did, in two places, and the
    /// copies were already drifting apart — which is what `lib.rs` exists to
    /// say about copies.
    pub fn findings(&self) -> &[Finding] {
        match self {
            LoadError::Invalid(f) => f,
            _ => &[],
        }
    }
}

/// A record found on disk, and which rule found it.
struct Found {
    /// Repository-relative, `/`-separated, for messages and for the Layer C join.
    rel: String,
    path: PathBuf,
    /// Found under `dir` by its numeric prefix, rather than named in `sources`.
    numbered: bool,
}

/// `a/b` from `a` and `b`, tolerating a trailing slash or a `.` directory.
fn join_rel(dir: &str, name: &str) -> String {
    let d = normalise(dir);
    if d.is_empty() || d == "." {
        name.to_string()
    } else {
        format!("{}/{}", d, name)
    }
}

/// Repository-relative, `/`-separated, without a leading `./` or trailing `/`.
/// Deliberately textual: `canonicalize` resolves symlinks and can hand back a
/// path outside the repository.
fn normalise(p: &str) -> String {
    p.replace('\\', "/")
        .trim_start_matches("./")
        .trim_end_matches('/')
        .to_string()
}

/// Walk up looking for the registry, the way git finds its own root.
pub fn find_root(from: &Path) -> Option<PathBuf> {
    let mut cur = Some(from);
    while let Some(d) = cur {
        if d.join(REGISTRY).is_file() {
            return Some(d.to_path_buf());
        }
        cur = d.parent();
    }
    None
}

pub fn load(from: &Path) -> Result<Loaded, LoadError> {
    let root = find_root(from).ok_or_else(|| LoadError::NoRegistry(from.to_path_buf()))?;
    let reg_path = root.join(REGISTRY);
    let reg_src = fs::read_to_string(&reg_path)
        .map_err(|e| LoadError::Unreadable(format!("{}: {}", reg_path.display(), e)))?;
    let mut registry = parse::registry(REGISTRY, &reg_src).map_err(LoadError::Invalid)?;

    let adr_dir = root.join(&registry.dir);

    // Packs first, because they contribute keys and scopes that the local
    // registry may then widen, and because a pack that cannot be read is a
    // reason to stop rather than to answer from a corpus missing a third of
    // its graph.
    let packs = read_packs(&root, &registry.packs)?;
    merge_declarations(&mut registry, &packs).map_err(LoadError::Invalid)?;

    // Two discovery rules, and which one found a file decides how strictly it
    // is judged (SEMANTICS section 2.2).
    let mut found: Vec<Found> = Vec::new();
    let mut findings = Vec::new();

    // 1. Numbered records under `dir`. Unchanged: a numeric prefix is what
    //    tells a record from a README, and frontmatter is mandatory.
    //
    //    Skipped entirely when there is no `dir`: joining an empty string onto
    //    the root would scan the whole repository root for numbered markdown.
    match dir_entries(&registry, &adr_dir) {
        Ok(rd) => {
            for p in rd
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "md").unwrap_or(false))
            {
                let name = p
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                // A README or any other prose file in the directory is not an
                // ADR. Requiring a numeric prefix is how they are told apart,
                // and it is the same rule `id-matches-filename` enforces.
                if !name
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
                {
                    continue;
                }
                found.push(Found {
                    rel: join_rel(&registry.dir, &name),
                    path: p,
                    numbered: true,
                });
            }
        }
        Err(e) if registry.sources.is_empty() && registry.packs.is_empty() => {
            return Err(LoadError::Unreadable(format!(
                "{}: {}",
                adr_dir.display(),
                e
            )))
        }
        // With `sources` or `packs` declared, a corpus need not have a `dir` on
        // disk yet. `heads --write` still needs one, and says so when it gets
        // there.
        Err(_) => {}
    }

    // 2. Records named outright. A listed file that is missing is an error:
    //    that is the whole reason these are literal paths rather than
    //    patterns, because a record must not be able to stop being one
    //    quietly. Its entries would leave the graph, whatever it superseded
    //    would return as a head, and `resolve` would answer `active` with a
    //    decision that was replaced.
    for src in &registry.sources {
        let path = root.join(src);
        if !path.is_file() {
            return Err(LoadError::Unreadable(format!(
                "{}: listed in `sources` and not a readable file",
                src
            )));
        }
        found.push(Found {
            rel: normalise(src),
            path,
            numbered: false,
        });
    }

    // One file may be reached both ways. Deduplicate before parsing, or it
    // becomes two records and `id-unique` fires against a file and itself.
    // The `dir` rule wins, so mandatory-frontmatter enforcement is never lost.
    found.sort_by(|a, b| {
        a.rel
            .to_lowercase()
            .cmp(&b.rel.to_lowercase())
            .then(b.numbered.cmp(&a.numbered))
    });
    found.dedup_by(|a, b| a.rel.to_lowercase() == b.rel.to_lowercase());

    let mut adrs = Vec::new();
    let mut files = Vec::new();
    for f in &found {
        let src = match fs::read_to_string(&f.path) {
            Ok(s) => s,
            Err(e) => {
                findings.push(
                    Finding::new(Layer::A, "frontmatter-parses", format!("unreadable: {}", e))
                        .in_file(f.rel.clone()),
                );
                continue;
            }
        };
        let origin = if f.numbered {
            parse::Origin::Numbered
        } else {
            parse::Origin::Listed
        };
        match parse::adr(&f.rel, &src, origin) {
            Ok(a) => adrs.push(a),
            Err(mut x) => findings.append(&mut x),
        }
        files.push((f.rel.clone(), src));
    }
    if !findings.is_empty() {
        return Err(LoadError::Invalid(findings));
    }

    // Vendored records join the graph as records, not as a second tier. That
    // is the point: a local record deciding a slot a pack already decides is
    // two heads for one slot, which is the contradiction `check` already
    // reports and `resolve` already exits 5 on. Nothing new had to be built to
    // stop a consumer quietly re-deciding a fleet key.
    for p in &packs {
        adrs.extend(p.adrs.iter().cloned());
    }

    let corpus = Corpus { registry, adrs };
    let graph = Graph::build(corpus).map_err(LoadError::Invalid)?;
    Ok(Loaded {
        graph,
        root,
        adr_dir,
        files,
        packs,
    })
}

/// The `dir` listing, or nothing at all when the registry declares no `dir`.
///
/// A registry that only vendors has `dir: ""`, and `root.join("")` is the
/// repository root. Reading it would treat every numbered markdown file in the
/// repository as a record.
fn dir_entries(reg: &aval_core::model::Registry, adr_dir: &Path) -> std::io::Result<fs::ReadDir> {
    if !reg.has_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no `dir`",
        ));
    }
    fs::read_dir(adr_dir)
}

/// The name a vendored pack is known by: its filename without the extension.
///
/// The path carries the name rather than a second registry field, because two
/// fields that must agree are two fields that can disagree. It is also the id
/// prefix, so renaming the file renames the records — which is visible in the
/// diff, and is the only way the name can change.
pub fn pack_name(rel: &str) -> &str {
    let base = rel.rsplit('/').next().unwrap_or(rel);
    base.rsplit_once('.').map_or(base, |(stem, _)| stem)
}

fn read_packs(root: &Path, list: &[String]) -> Result<Vec<Pack>, LoadError> {
    let mut out = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for rel in list {
        let name = pack_name(rel);
        if seen.contains(&name) {
            return Err(LoadError::Invalid(vec![Finding::new(
                Layer::A,
                "pack-parses",
                format!(
                    "two packs are named `{}`; the name is the id prefix, so it \
                     has to be unique — rename one file",
                    name
                ),
            )
            .in_file(REGISTRY)]));
        }
        seen.push(name);
        let path = root.join(rel);
        let src = fs::read_to_string(&path).map_err(|e| {
            LoadError::Unreadable(format!(
                "{}: listed in `packs` and not readable ({}); run `aval add` again",
                rel, e
            ))
        })?;
        out.push(pack::parse(rel, name, &src).map_err(LoadError::Invalid)?);
    }
    Ok(out)
}

/// Fold every pack's keys and scopes into the registry.
///
/// A consumer may **widen** a vendored key and may not narrow it. The list it
/// writes locally is the set of scopes it is *adding*, so narrowing is not
/// something the format can express rather than something a check has to catch.
/// The fleet's own slot stays exactly where the fleet put it, which is what
/// makes a vendored decision worth vendoring.
fn merge_declarations(
    reg: &mut aval_core::model::Registry,
    packs: &[Pack],
) -> Result<(), Vec<Finding>> {
    let mut out = Vec::new();
    for p in packs {
        for s in &p.scopes {
            if !reg.scopes.contains(s) {
                reg.scopes.push(s.clone());
            }
        }
        for pk in &p.keys {
            match reg.keys.iter_mut().find(|k| k.name == pk.name) {
                None => reg.keys.push(pk.clone()),
                Some(local) => merge_key(local, pk, &p.name, &mut out),
            }
        }
    }
    reg.scopes.sort();
    reg.scopes.dedup();
    if out.is_empty() {
        Ok(())
    } else {
        Err(out)
    }
}

fn merge_key(local: &mut KeyDef, packed: &KeyDef, pack: &str, out: &mut Vec<Finding>) {
    let f = |m: String| Finding::new(Layer::A, "pack-key-widens", m).in_file(REGISTRY);

    if local.description.is_some() && packed.description.is_some() {
        out.push(f(format!(
            "`{}` is described by the `{}` pack and again here; one key has one \
             description, and the pack's is the one everybody else reads",
            local.name, pack
        )));
    } else if local.description.is_none() {
        local.description = packed.description.clone();
    }

    match (&packed.scopes, &local.scopes) {
        // The pack restricts nothing, so there is nothing to widen and a local
        // list changes no answer. Silence here would let somebody believe they
        // had scoped a key that is answerable everywhere.
        (None, Some(_)) => {
            out.push(f(format!(
                "`{}` is declared by the `{}` pack without a scope restriction, \
                 so listing scopes here adds nothing; remove the list",
                local.name, pack
            )));
            local.scopes = None;
        }
        (None, None) => {}
        (Some(from_pack), local_added) => {
            let mut merged = from_pack.clone();
            for s in local_added.iter().flat_map(|v| v.iter()) {
                if !merged.contains(s) {
                    merged.push(s.clone());
                }
            }
            local.scopes = Some(merged);
        }
    }
}
