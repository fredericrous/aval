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
    /// `(basename, source)` for every ADR read, for Layer C.
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
    let mut entries: Vec<PathBuf> = match fs::read_dir(&adr_dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "md").unwrap_or(false))
            .collect(),
        Err(e) => {
            return Err(LoadError::Unreadable(format!(
                "{}: {}",
                adr_dir.display(),
                e
            )))
        }
    };
    // Sorted so findings and projections are stable across filesystems.
    entries.sort();

    let mut adrs = Vec::new();
    let mut files = Vec::new();
    let mut findings = Vec::new();
    for p in &entries {
        let name = p
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        // A README or any other prose file in the directory is not an ADR.
        // Requiring a numeric prefix is how they are told apart, and it is the
        // same rule `id-matches-filename` enforces.
        if !name
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            continue;
        }
        let src = match fs::read_to_string(p) {
            Ok(s) => s,
            Err(e) => {
                findings.push(
                    Finding::new(Layer::A, "frontmatter-parses", format!("unreadable: {}", e))
                        .in_file(name.clone()),
                );
                continue;
            }
        };
        match parse::adr(&name, &src) {
            Ok(a) => adrs.push(a),
            Err(mut f) => findings.append(&mut f),
        }
        files.push((name, src));
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
