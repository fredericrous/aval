//! `aval traits`: what this repository says it is, and what its files suggest
//! (SEMANTICS section 2.5).
//!
//! Three questions, kept apart because they fail differently:
//!
//! - **summary** reads the corpus and nothing else — no git, no subprocess —
//!   because it runs at every session start;
//! - **detect** proposes areas from the files git tracks, and is advisory;
//! - **check** compares that proposal with the declaration.
//!
//! Detection errs toward reporting. A false positive costs a reviewer one
//! `disclaims` line; a false negative hides a constraint from the agent the
//! hook exists for. Manifests are scanned line by line, never parsed: a shape
//! the scanner does not know can cost a detection, never a failure. The one
//! parse is `package.json`, which is JSON or broken.
//!
//! Unlike `relevant`'s git helper, which turns a failure into a missing signal,
//! the runner here reports it: "I found nothing" and "I could not look" must
//! not share an exit code (section 14).

use crate::load::Loaded;
use crate::render;
use aval_core::applicability::{self, Scope};
use aval_core::glob;
use aval_core::json::{self, Json};
use aval_core::model::{Area, Registry};
use std::path::Path;
use std::process::{Command, Stdio};

/// Why inspection could not happen. Every variant is exit 3.
#[derive(Debug)]
pub enum Uninspectable {
    Git(String),
    Unreadable(String),
    NotJson(String),
}

impl std::fmt::Display for Uninspectable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Uninspectable::Git(m) => write!(f, "could not list tracked files: {}", m),
            Uninspectable::Unreadable(m) => write!(f, "could not read {}", m),
            Uninspectable::NotJson(m) => write!(f, "{}", m),
        }
    }
}

/// One detected trait, anchored at the file that is its evidence. Coverage is
/// judged at the anchor: an area or disclaim matching it must name the trait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    pub anchor: String,
    pub name: String,
    pub evidence: String,
}

/// Where a repository without a registry is rooted: git's top level, or the
/// directory asked when git has none to name.
pub fn repository_root(from: &Path) -> std::path::PathBuf {
    Command::new("git")
        .arg("-C")
        .arg(from)
        .args(["rev-parse", "--show-toplevel"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| std::path::PathBuf::from(String::from_utf8_lossy(&o.stdout).trim().to_string()))
        .unwrap_or_else(|| from.to_path_buf())
}

/// Every tracked path under `root`, relative to it.
///
/// NUL-delimited so a path with a newline or a quote is one path, and with
/// `core.quotePath` irrelevant. A failure is reported, never an empty list.
pub fn tracked(root: &Path) -> Result<Vec<String>, Uninspectable> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", "--cached"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| Uninspectable::Git(format!("git did not run ({})", e)))?;
    if !out.status.success() {
        return Err(Uninspectable::Git(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    let mut v: Vec<String> = out
        .stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    v.sort();
    Ok(v)
}

const MANIFESTS: &[&str] = &[
    "Cargo.toml",
    "go.mod",
    "package.json",
    "pyproject.toml",
    "setup.cfg",
];

const UI_PACKAGES: &[&str] = &[
    "react",
    "react-dom",
    "react-native",
    "preact",
    "vue",
    "svelte",
    "solid-js",
    "@angular/core",
    "lit",
];

const UI_EXTENSIONS: &[&str] = &[".tsx", ".jsx", ".vue", ".svelte"];

fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

fn base(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, b)| b)
}

fn under(dir: &str, path: &str) -> bool {
    dir.is_empty() || path.starts_with(&format!("{}/", dir))
}

fn join(dir: &str, rel: &str) -> String {
    if dir.is_empty() {
        rel.to_string()
    } else {
        format!("{}/{}", dir, rel)
    }
}

/// The package directory that owns a path: the deepest manifest directory it
/// lies under, or `None` when no manifest is above it.
fn owner<'a>(roots: &'a [String], path: &str) -> Option<&'a str> {
    roots
        .iter()
        .filter(|d| under(d, path))
        .max_by_key(|d| d.len())
        .map(String::as_str)
}

fn read(root: &Path, rel: &str) -> Result<String, Uninspectable> {
    std::fs::read_to_string(root.join(rel))
        .map_err(|e| Uninspectable::Unreadable(format!("`{}` ({})", rel, e)))
}

/// TOML section headers, as written: `[package]`, `[[bin]]`,
/// `[target.'cfg(unix)'.dependencies]`. Line-scanned; nothing is parsed.
fn toml_sections(src: &str) -> Vec<String> {
    src.lines()
        .map(str::trim)
        .filter(|l| l.starts_with('['))
        .map(|l| {
            let end = l.rfind(']').map_or(l.len(), |i| i + 1);
            l[..end].to_string()
        })
        .collect()
}

/// Every trait the tracked files suggest.
pub fn detect(root: &Path, files: &[String]) -> Result<Vec<Detection>, Uninspectable> {
    let mut roots: Vec<String> = files
        .iter()
        .filter(|f| MANIFESTS.contains(&base(f)))
        .map(|f| dir_of(f).to_string())
        .collect();
    roots.sort();
    roots.dedup();

    let mut out: Vec<Detection> = Vec::new();
    let mut push = |anchor: &str, name: &str, evidence: String| {
        let d = Detection {
            anchor: anchor.to_string(),
            name: name.to_string(),
            evidence,
        };
        if !out.iter().any(|x| x.anchor == d.anchor && x.name == d.name) {
            out.push(d);
        }
    };

    // A workspace root that is only a workspace — no dependencies and no bin
    // of its own — speaks for nothing; its members are detected one by one.
    let mut silent_roots: Vec<String> = Vec::new();

    for f in files.iter().filter(|f| MANIFESTS.contains(&base(f))) {
        let dir = dir_of(f);
        let owned = |p: &&String| owner(&roots, p) == Some(dir);
        match base(f) {
            "Cargo.toml" => {
                let src = read(root, f)?;
                let sections = toml_sections(&src);
                let has_package = sections.iter().any(|s| s == "[package]");
                if !has_package {
                    if sections.iter().any(|s| s == "[workspace]") {
                        silent_roots.push(dir.to_string());
                    }
                    continue;
                }
                if sections.iter().any(|s| s == "[[bin]]") {
                    push(f, "cli", "a `[[bin]]` target".into());
                }
                let main = join(dir, "src/main.rs");
                let bin = join(dir, "src/bin/");
                if let Some(p) = files
                    .iter()
                    .filter(owned)
                    .find(|p| **p == main || (p.starts_with(&bin) && p.ends_with(".rs")))
                {
                    push(f, "cli", format!("`{}`", p));
                }
            }
            "pyproject.toml" => {
                let src = read(root, f)?;
                let sections = toml_sections(&src);
                if sections
                    .iter()
                    .any(|s| s == "[project.scripts]" || s == "[tool.poetry.scripts]")
                {
                    push(f, "cli", "a scripts table".into());
                }
            }
            "setup.cfg" => {
                if read(root, f)?.contains("console_scripts") {
                    push(f, "cli", "`console_scripts`".into());
                }
            }
            "package.json" => {
                let src = read(root, f)?;
                let doc = json::parse(&src)
                    .map_err(|e| Uninspectable::NotJson(format!("`{}` is not JSON: {}", f, e)))?;
                let deps = |k: &str| -> Vec<String> {
                    match doc.get(k) {
                        Some(Json::Obj(m)) => m.keys().cloned().collect(),
                        _ => Vec::new(),
                    }
                };
                let own: Vec<String> = deps("dependencies")
                    .into_iter()
                    .chain(deps("peerDependencies"))
                    .collect();
                let has_bin = doc.get("bin").is_some_and(|b| !b.is_null());
                let is_workspace = doc.get("workspaces").is_some()
                    || files.iter().any(|p| *p == join(dir, "pnpm-workspace.yaml"));
                if is_workspace && own.is_empty() && !has_bin {
                    silent_roots.push(dir.to_string());
                    continue;
                }
                if has_bin {
                    push(f, "cli", "a `bin` entry".into());
                }
                if let Some(u) = own.iter().find(|d| UI_PACKAGES.contains(&d.as_str())) {
                    push(f, "ui", format!("`{}` in dependencies", u));
                }
            }
            _ => {}
        }
    }

    // Go: every `package main` is a command, wherever it sits under the
    // module — `cmd/main.go` as much as `cmd/x/main.go`. Anchored per
    // directory, because that is where an area would say "this is a CLI".
    for p in files
        .iter()
        .filter(|p| p.ends_with(".go") && !p.ends_with("_test.go"))
    {
        let src = read(root, p)?;
        if go_package_main(&src) {
            push(p, "cli", "`package main`".into());
        }
    }
    // One Go detection per directory: the first file found speaks for it.
    let mut seen_dirs: Vec<String> = Vec::new();
    out.retain(|d| {
        if !d.anchor.ends_with(".go") {
            return true;
        }
        let dir = dir_of(&d.anchor).to_string();
        if seen_dirs.contains(&dir) {
            false
        } else {
            seen_dirs.push(dir);
            true
        }
    });

    // UI by file type, per owning package: a component library whose `react`
    // is a devDependency still has `.tsx` files.
    let mut ui_owners: Vec<String> = Vec::new();
    for p in files
        .iter()
        .filter(|p| UI_EXTENSIONS.iter().any(|e| p.ends_with(e)))
    {
        let anchor = match owner(&roots, p) {
            Some(d) if silent_roots.iter().any(|s| s == d) => continue,
            Some(d) => manifest_in(files, d).unwrap_or_else(|| p.clone()),
            None => p.clone(),
        };
        if ui_owners.contains(&anchor) {
            continue;
        }
        ui_owners.push(anchor.clone());
        let d = Detection {
            anchor,
            name: "ui".into(),
            evidence: format!("`{}`", p),
        };
        if !out.iter().any(|x| x.anchor == d.anchor && x.name == d.name) {
            out.push(d);
        }
    }

    out.sort_by(|a, b| a.anchor.cmp(&b.anchor).then(a.name.cmp(&b.name)));
    Ok(out)
}

/// The manifest a package directory is anchored at, preferring the one a
/// reviewer would open first.
fn manifest_in(files: &[String], dir: &str) -> Option<String> {
    MANIFESTS
        .iter()
        .map(|m| join(dir, m))
        .find(|m| files.contains(m))
}

/// Whether a Go file is `package main` and not excluded by `//go:build
/// ignore`. Only the header is read: build constraints and the package clause
/// both precede any declaration.
fn go_package_main(src: &str) -> bool {
    for line in src.lines() {
        let l = line.trim();
        if l.starts_with("//go:build") && l.split_whitespace().any(|w| w == "ignore") {
            return false;
        }
        if let Some(rest) = l.strip_prefix("package ") {
            return rest.split_whitespace().next() == Some("main");
        }
    }
    false
}

/// A `--check` finding. Advisory text; the kind is for `--json`.
#[derive(Debug, Clone)]
pub struct Finding {
    pub kind: &'static str,
    pub path: String,
    pub message: String,
}

fn names(list: &[Area], path: &str, name: &str) -> bool {
    list.iter()
        .any(|a| glob::matches(&a.glob, path) && a.traits.iter().any(|t| t == name))
}

/// What detection reports that the declaration does not cover, and every glob
/// that matches nothing.
pub fn check(reg: &Registry, files: &[String], found: &[Detection]) -> Vec<Finding> {
    let vocab = reg.vocabulary();
    let mut out = Vec::new();
    for d in found {
        if !vocab.contains(&d.name) {
            out.push(Finding {
                kind: "undeclared-trait",
                path: d.anchor.clone(),
                message: format!(
                    "`{}` looks like `{}` ({}), and no `traits:` declares `{}`; \
                     it cannot be given an area until one does",
                    d.anchor, d.name, d.evidence, d.name
                ),
            });
        } else if !names(&reg.areas, &d.anchor, &d.name)
            && !names(&reg.disclaims, &d.anchor, &d.name)
        {
            out.push(Finding {
                kind: "uncovered",
                path: d.anchor.clone(),
                message: format!(
                    "`{}` looks like `{}` ({}), and no area or disclaim matching it \
                     names `{}`",
                    d.anchor, d.name, d.evidence, d.name
                ),
            });
        }
    }
    for (field, list) in [("areas", &reg.areas), ("disclaims", &reg.disclaims)] {
        for a in list.iter() {
            if !files.iter().any(|f| glob::matches(&a.glob, f)) {
                out.push(Finding {
                    kind: "stale-glob",
                    path: a.glob.clone(),
                    message: format!("`{}` under `{}` matches no tracked file", a.glob, field),
                });
            }
        }
    }
    out
}

/// The areas a detection proposes, one glob per anchor directory.
pub fn proposal(found: &[Detection]) -> Vec<(String, Vec<String>, Vec<String>)> {
    let mut v: Vec<(String, Vec<String>, Vec<String>)> = Vec::new();
    for d in found {
        let dir = dir_of(&d.anchor);
        let g = if dir.is_empty() {
            "**".to_string()
        } else {
            format!("{}/**", dir)
        };
        match v.iter_mut().find(|(x, _, _)| *x == g) {
            Some((_, t, ev)) => {
                if !t.contains(&d.name) {
                    t.push(d.name.clone());
                }
                ev.push(format!("{}: {} — {}", d.name, d.anchor, d.evidence));
            }
            None => v.push((
                g,
                vec![d.name.clone()],
                vec![format!("{}: {} — {}", d.name, d.anchor, d.evidence)],
            )),
        }
    }
    for (_, t, _) in v.iter_mut() {
        t.sort();
    }
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

pub fn detect_text(reg: &Registry, found: &[Detection]) -> String {
    let vocab = reg.vocabulary();
    let mut s = String::from(
        "# Proposed by `aval traits --detect`: ADVISORY. Review before committing;\n\
         # a false positive belongs under `disclaims:`, not deleted silently.\n",
    );
    let p = proposal(found);
    if p.is_empty() {
        s.push_str("# nothing detected\n");
        return s;
    }
    s.push_str("areas:\n");
    for (g, t, ev) in &p {
        for e in ev {
            s.push_str(&format!("  # {}\n", e));
        }
        s.push_str(&format!("  \"{}\": [{}]\n", g, t.join(", ")));
    }
    let missing: Vec<&String> = p
        .iter()
        .flat_map(|(_, t, _)| t.iter())
        .filter(|t| !vocab.contains(t))
        .collect();
    if !missing.is_empty() {
        let mut m: Vec<&str> = missing.iter().map(|t| t.as_str()).collect();
        m.sort();
        m.dedup();
        s.push_str(&format!(
            "# note: not in the vocabulary: {} — `areas-declared` refuses these until a \
             `traits:` declares them\n",
            m.join(", ")
        ));
    }
    s
}

pub fn detect_json(found: &[Detection]) -> Json {
    let rows: Vec<Json> = found
        .iter()
        .map(|d| {
            Json::obj()
                .set("anchor", d.anchor.as_str())
                .set("trait", d.name.as_str())
                .set("evidence", d.evidence.as_str())
        })
        .collect();
    let areas: Vec<Json> = proposal(found)
        .into_iter()
        .map(|(g, t, _)| Json::obj().set("glob", g).set("traits", t))
        .collect();
    Json::obj().set("detected", rows).set("areas", areas)
}

pub fn check_json(findings: &[Finding]) -> Json {
    let rows: Vec<Json> = findings
        .iter()
        .map(|f| {
            Json::obj()
                .set("kind", f.kind)
                .set("path", f.path.as_str())
                .set("message", f.message.as_str())
        })
        .collect();
    Json::obj().set("findings", rows)
}

fn areas_json(list: &[Area]) -> Vec<Json> {
    list.iter()
        .map(|a| {
            Json::obj()
                .set("glob", a.glob.as_str())
                .set("traits", a.traits.clone())
        })
        .collect()
}

/// `aval traits --json`, and the `aval_traits` tool: the declaration only.
pub fn traits_json(l: &Loaded) -> Json {
    let reg = l.graph.registry();
    Json::obj()
        .set("vocabulary", reg.vocabulary())
        .set("areas", areas_json(&reg.areas))
        .set_opt("in_force", applicability::repo_traits(reg))
        .set("disclaims", areas_json(&reg.disclaims))
        .set_opt(
            "omitted",
            summary_omitted(l).as_ref().map(render::omitted_json),
        )
}

pub fn traits_text(l: &Loaded) -> String {
    let reg = l.graph.registry();
    let mut s = String::new();
    let vocab = reg.vocabulary();
    s.push_str(&format!(
        "vocabulary  {}\n",
        if vocab.is_empty() {
            "(none)".to_string()
        } else {
            vocab.join(", ")
        }
    ));
    if reg.areas.is_empty() {
        s.push_str("areas       (none: every rule is listed)\n");
    } else {
        s.push_str("areas\n");
        for a in &reg.areas {
            s.push_str(&format!("  {}  [{}]\n", a.glob, a.traits.join(", ")));
        }
    }
    if !reg.disclaims.is_empty() {
        s.push_str("disclaims\n");
        for a in &reg.disclaims {
            s.push_str(&format!("  {}  [{}]\n", a.glob, a.traits.join(", ")));
        }
    }
    if let Some(line) = summary(l) {
        s.push_str(&line);
        s.push('\n');
    }
    s
}

/// What the default rule listing hides, over every active rule.
fn summary_omitted(l: &Loaded) -> Option<aval_core::applicability::Omitted> {
    let reg = l.graph.registry();
    let active: Vec<_> = l
        .graph
        .corpus()
        .rules
        .iter()
        .filter(|r| l.graph.rule_reason(r).is_none())
        .collect();
    applicability::filter(reg, &Scope::for_query(reg, &[]), active).1
}

/// The hook's line. `None` when no areas are declared, because then nothing is
/// filtered; otherwise always a line, zeros included, so an area that hides
/// everything is seen at every session start.
pub fn summary(l: &Loaded) -> Option<String> {
    let o = summary_omitted(l)?;
    Some(format!(
        "traits here: {} — hidden by traits: {} constraint(s), {} heuristic(s) (aval rules --all-traits)",
        if o.traits.is_empty() {
            "none".to_string()
        } else {
            o.traits.join(", ")
        },
        o.constraints,
        o.heuristics
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_main_header() {
        assert!(go_package_main("// Copyright\npackage main\n"));
        assert!(go_package_main("package main // cmd\n"));
        assert!(!go_package_main("package server\n"));
        assert!(!go_package_main("//go:build ignore\n\npackage main\n"));
        assert!(go_package_main("//go:build linux\n\npackage main\n"));
    }

    #[test]
    fn toml_headers_are_scanned_not_parsed() {
        let src = "[package]\nname = \"x\"\n[target.'cfg(unix)'.dependencies]\n\
                   axum.workspace = true\ndeps = { a = 1,\n  b = 2 }\n[[bin]] # main\n";
        let s = toml_sections(src);
        assert!(s.contains(&"[package]".to_string()));
        assert!(s.contains(&"[[bin]]".to_string()));
        assert!(s.contains(&"[target.'cfg(unix)'.dependencies]".to_string()));
    }

    #[test]
    fn owner_is_the_deepest_manifest() {
        let roots = vec!["".to_string(), "web".to_string()];
        assert_eq!(owner(&roots, "web/src/a.tsx"), Some("web"));
        assert_eq!(owner(&roots, "cmd/main.go"), Some(""));
        assert_eq!(owner(&["web".to_string()], "x.tsx"), None);
    }
}
