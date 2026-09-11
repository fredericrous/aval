//! Reading a corpus off disk.
//!
//! All filesystem access lives here. `aval-core` stays pure, so everything
//! below hands it strings and gets findings back.

use aval_core::graph::Graph;
use aval_core::model::{Corpus, Finding, Layer};
use aval_core::parse;
use std::fs;
use std::path::{Path, PathBuf};

pub const REGISTRY: &str = ".adr.yaml";
pub const HEADS: &str = "HEADS.md";

pub struct Loaded {
    pub graph: Graph,
    /// Directory holding the registry.
    pub root: PathBuf,
    /// Directory holding the ADR files.
    pub adr_dir: PathBuf,
    /// `(repository-relative path, source)` for every record read, for Layer
    /// C. A path rather than a basename: two sources may hold the same
    /// filename, and the Layer C join would silently never match.
    pub files: Vec<(String, String)>,
}

pub enum LoadError {
    /// No registry anywhere above the working directory.
    NoRegistry(PathBuf),
    /// Could not read something that must be readable.
    Unreadable(String),
    /// The corpus is structurally invalid: Layer A.
    Invalid(Vec<Finding>),
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
    let registry = parse::registry(REGISTRY, &reg_src).map_err(LoadError::Invalid)?;

    let adr_dir = root.join(&registry.dir);

    // Two discovery rules, and which one found a file decides how strictly it
    // is judged (SEMANTICS section 2.2).
    let mut found: Vec<Found> = Vec::new();
    let mut findings = Vec::new();

    // 1. Numbered records under `dir`. Unchanged: a numeric prefix is what
    //    tells a record from a README, and frontmatter is mandatory.
    match fs::read_dir(&adr_dir) {
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
        Err(e) if registry.sources.is_empty() => {
            return Err(LoadError::Unreadable(format!(
                "{}: {}",
                adr_dir.display(),
                e
            )))
        }
        // With `sources` declared, a corpus need not have a `dir` on disk yet.
        // `heads --write` still needs it, and says so when it gets there.
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

    let corpus = Corpus { registry, adrs };
    let graph = Graph::build(corpus).map_err(LoadError::Invalid)?;
    Ok(Loaded {
        graph,
        root,
        adr_dir,
        files,
    })
}
