//! Layer C: `links-resolve`.
//!
//! An append-only historical ADR can correctly cite a component that was later
//! deleted. Demanding that every citation resolve against today's tree would
//! reward deleting the evidence, which inverts the point of the corpus. So a
//! citation carrying `@<rev>` is recorded as historical and never checked, and
//! the repair for a dangling live citation is to pin it, not to remove it.
//!
//! SEMANTICS section 11 fixes exactly what is checked. The conservative half of
//! that list matters as much as the other: a check that cried wolf on fenced
//! example code would be turned off within a week.

use crate::load::Loaded;
use aval_core::model::{Finding, Layer};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A citation found in a document body.
struct Cite {
    target: String,
    line: usize,
    /// Markdown link targets resolve against the DOCUMENT, per the markdown
    /// standard. Backticked bare paths resolve against the repository root,
    /// which is how this corpus writes them. Conflating the two made every
    /// sibling ADR link read as dangling.
    document_relative: bool,
}

fn is_url(s: &str) -> bool {
    s.contains("://") || s.starts_with("mailto:") || s.starts_with("tel:")
}

fn is_glob(s: &str) -> bool {
    // `{a,b}` brace expansion counts: the corpus writes
    // `bootstrap/configs/{homelab,monitor,nas}.yaml` to name three files at
    // once, and no single path was ever meant.
    s.contains('*') || s.contains('?') || s.contains('[') || s.contains('{')
}

/// `path@<rev>`: historical by declaration, so not the tree's business.
fn is_pinned(s: &str) -> bool {
    s.contains('@')
}

fn looks_like_a_path(s: &str) -> bool {
    s.contains('/') && !s.chars().any(|c| c.is_whitespace())
}

/// `10.244.0.0/16`. Contains a slash and no whitespace, and is not a file.
fn is_cidr(s: &str) -> bool {
    let Some((addr, bits)) = s.rsplit_once('/') else {
        return false;
    };
    !bits.is_empty()
        && bits.chars().all(|c| c.is_ascii_digit())
        && addr.split('.').count() == 4
        && addr
            .split('.')
            .all(|o| !o.is_empty() && o.chars().all(|c| c.is_ascii_digit()))
}

/// `ghcr.io/fredericrous/homelab/manifests:latest`, or any other host-rooted
/// reference. A repository path's first segment is a directory name, which does
/// not carry a dot unless it is a dotfile like `.github`.
fn is_host_rooted(s: &str) -> bool {
    let first = s.split('/').next().unwrap_or("");
    first.contains('.') && !first.starts_with('.')
}

/// Trim a `:line` or `:line-line` suffix, which addresses a place inside a file
/// rather than naming a different one.
fn without_line_suffix(s: &str) -> &str {
    let Some((path, tail)) = s.rsplit_once(':') else {
        return s;
    };
    let numeric = |x: &str| !x.is_empty() && x.chars().all(|c| c.is_ascii_digit());
    let ok = match tail.split_once('-') {
        Some((a, b)) => numeric(a) && numeric(b),
        None => numeric(tail),
    };
    if ok {
        path
    } else {
        s
    }
}

/// Strip a trailing `#fragment`, which addresses a place inside the file.
fn without_fragment(s: &str) -> &str {
    match s.find('#') {
        Some(0) => "",
        Some(i) => &s[..i],
        None => s,
    }
}

fn collect(src: &str) -> Vec<Cite> {
    let mut out = Vec::new();
    let mut fenced = false;
    // Skip frontmatter: its edges are Layer A's business, not citations.
    let body_start = match src.strip_prefix("---\n") {
        Some(rest) => rest.find("\n---\n").map(|i| i + 9).unwrap_or(0),
        None => 0,
    };
    let skipped = src[..body_start].matches('\n').count();

    for (n, line) in src[body_start..].lines().enumerate() {
        let lineno = n + 1 + skipped;
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        collect_markdown_links(line, lineno, &mut out);
        collect_backticked(line, lineno, &mut out);
    }
    out
}

/// `[text](target)`. Only the target, and only when it is repo-relative.
fn collect_markdown_links(line: &str, lineno: usize, out: &mut Vec<Cite>) {
    let b: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < b.len() {
        if b[i] != ']' || i + 1 >= b.len() || b[i + 1] != '(' {
            i += 1;
            continue;
        }
        let start = i + 2;
        let Some(rel_end) = b[start..].iter().position(|c| *c == ')') else {
            break;
        };
        let target: String = b[start..start + rel_end].iter().collect();
        // A title after the URL: `(path "title")`.
        let target = target.split_whitespace().next().unwrap_or("").to_string();
        if !target.is_empty() {
            out.push(Cite {
                target,
                line: lineno,
                document_relative: true,
            });
        }
        i = start + rel_end + 1;
    }
}

/// Backticked bare tokens that look like paths.
fn collect_backticked(line: &str, lineno: usize, out: &mut Vec<Cite>) {
    let mut rest = line;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else { break };
        let token = &after[..close];
        if looks_like_a_path(token) {
            out.push(Cite {
                target: token.to_string(),
                line: lineno,
                document_relative: false,
            });
        }
        rest = &after[close + 1..];
    }
}

/// Does this citation point outside the repository?
///
/// `docs/adr/../../ddns-updater-operator/` resolves above the root: it names a
/// SIBLING repository, from back when that operator lived in this tree. This
/// tool cannot resolve another repository and must not guess, so such a
/// citation is skipped rather than reported as dangling. Pinning it would be
/// wrong too: it was never at a revision of THIS repository.
fn escapes_repo(l: &Loaded, adr_dir: &Path, c: &Cite) -> bool {
    let t = without_line_suffix(without_fragment(&c.target));
    let base = if t.starts_with("./") || t.starts_with("../") || c.document_relative {
        adr_dir
    } else {
        l.root.as_path()
    };
    let mut depth: i32 = base
        .strip_prefix(&l.root)
        .map(|r| r.components().count() as i32)
        .unwrap_or(0);
    for part in t.split('/') {
        match part {
            ".." => depth -= 1,
            "." | "" => {}
            _ => depth += 1,
        }
        if depth < 0 {
            return true;
        }
    }
    false
}

fn resolves(l: &Loaded, adr_dir: &Path, c: &Cite) -> bool {
    let t = without_line_suffix(without_fragment(&c.target));
    if t.is_empty() {
        return true;
    }
    let base = if t.starts_with("./") || t.starts_with("../") || c.document_relative {
        adr_dir
    } else {
        l.root.as_path()
    };
    base.join(t.trim_end_matches('/')).exists()
}

/// The repository's own top-level entries.
///
/// This is the signal that separates a repository path from everything else a
/// backticked token containing a slash can be: a URL path (`/user/settings/keys`),
/// a Vault path (`secret/data/forgejo/admin-api-token`), a container path
/// (`/var/lib/llamacpp-models`), a path into a DIFFERENT repository
/// (`app/routes/settings.git.tsx`), or a forge slug (`fredericrous/duro-app`).
/// All of those look exactly like a repository path and none of them is one.
///
/// The cost is a known blind spot: a citation whose whole top-level directory
/// was removed, `.github/workflows/x.yaml` after CI moved to `.forgejo/`, is
/// skipped rather than reported. That is the deliberate trade. A check that
/// reported fifty false dangling links to catch one real one would be switched
/// off within a week, and then it would catch nothing at all.
fn top_level(root: &Path) -> Vec<String> {
    std::fs::read_dir(root)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Paths git deliberately ignores.
///
/// `infrastructure/homelab/terraform.tfvars` holds secrets and is gitignored,
/// so it is absent from a fresh worktree and present in the real checkout.
/// Citing it is correct, and reporting it as dangling would be telling the
/// author to pin a file that is supposed to be there. Best-effort: no git, no
/// exclusions, and the check simply stays as conservative as it was before.
fn ignored(root: &Path, candidates: &[String]) -> Vec<String> {
    if candidates.is_empty() {
        return Vec::new();
    }
    let Ok(mut child) = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("check-ignore")
        .arg("--stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Vec::new();
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(format!("{}\n", candidates.join("\n")).as_bytes());
    }
    let Ok(out) = child.wait_with_output() else {
        return Vec::new();
    };
    // `git check-ignore` ABORTS THE WHOLE BATCH on a single path it considers
    // outside the repository, exiting 128 with an empty stdout. Read that as
    // "nothing is ignored" and every genuinely ignored path silently becomes a
    // false dangling-link report. Anything but a clean 0 or 1 is a failure to
    // answer, not an answer.
    if !matches!(out.status.code(), Some(0) | Some(1)) {
        // One path git refuses to answer about — inside a submodule, outside
        // the repository — aborts the WHOLE batch with an empty stdout. Reading
        // that as "nothing is ignored" turns every genuinely ignored path into
        // a false dangling-link report, so fall back to asking one at a time
        // and simply drop the ones git will not answer.
        return candidates
            .iter()
            .filter(|c| {
                Command::new("git")
                    .arg("-C")
                    .arg(root)
                    .arg("check-ignore")
                    .arg("--quiet")
                    .arg(c.as_str())
                    .stderr(Stdio::null())
                    .status()
                    .map(|s| s.code() == Some(0))
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.to_string())
        .collect()
}

/// Paths recorded in the index as gitlinks (mode 160000).
///
/// `vault-transit-unseal-operator` is one, and this repository carries no
/// `.gitmodules` to populate it, so the directory exists and is empty. A
/// citation into it names a file that IS tracked, just not here. Reporting it
/// as dangling would be wrong, and `git check-ignore` refuses to answer about
/// it at all.
fn gitlinks(root: &Path) -> Vec<String> {
    let Ok(out) = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("ls-files")
        .arg("--stage")
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.starts_with("160000"))
        .filter_map(|l| l.split('\t').nth(1))
        .map(|p| p.to_string())
        .collect()
}

/// The base a citation resolves against.
///
/// A document-relative link, or one written `./` or `../`, resolves beside the
/// document. A bare backticked token resolves from the repository root — that
/// is what the "first segment is a top-level entry" rule established about it.
/// All three consumers must agree, or the gitlink skip is computed against a
/// different base than the existence check and stops matching.
fn base_for<'a>(l: &'a Loaded, doc_dir: &'a Path, c: &Cite) -> &'a Path {
    let t = without_line_suffix(without_fragment(&c.target));
    if t.starts_with("./") || t.starts_with("../") || c.document_relative {
        doc_dir
    } else {
        l.root.as_path()
    }
}

/// The directory a document's relative links resolve against.
///
/// Per document, not per corpus. Records may live in more than one directory
/// now, and resolving every citation against the ADR directory would read
/// `[x](./diagram.svg)` in `docs/architecture/foo.md` as pointing inside
/// `docs/adr/` — dangling when it is fine, and resolving when it is not.
fn document_dir(l: &Loaded, rel: &str) -> PathBuf {
    match Path::new(rel).parent() {
        Some(p) if !p.as_os_str().is_empty() => l.root.join(p),
        _ => l.root.clone(),
    }
}

/// A citation target as a repository-relative path, resolved lexically.
///
/// Lexical because the target may not exist — that is usually the whole
/// question — so `canonicalize` is unavailable, and it resolves symlinks
/// anyway. Used for the gitignore and gitlink comparisons, which are string
/// prefix tests against repository-relative paths and were previously fed a
/// target with its leading `../` merely trimmed off.
fn repo_relative(l: &Loaded, base: &Path, target: &str) -> String {
    let start = if target.starts_with('/') {
        Vec::new()
    } else {
        base.strip_prefix(&l.root)
            .map(|r| {
                r.components()
                    .map(|c| c.as_os_str().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut parts: Vec<String> = start;
    for seg in target.trim_start_matches('/').split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other.to_string()),
        }
    }
    parts.join("/")
}

pub fn check(l: &Loaded) -> Vec<Finding> {
    let mut out = Vec::new();
    let roots = top_level(&l.root);
    let links = gitlinks(&l.root);
    let mut pending: Vec<(String, usize, String)> = Vec::new();
    for (name, src) in &l.files {
        // A draft PROPOSES. ADR-0016 writes "New patch file:
        // `infrastructure/homelab/patch/strix-machineconfig-patch.yaml`" about a
        // file the work would create, which is intent rather than evidence, and
        // reporting it as dangling asks the author to pin a future. A draft's
        // citations start being checked when the draft is accepted.
        let is_draft = l
            .graph
            .corpus()
            .adrs
            .iter()
            .any(|a| &a.file == name && !a.status.is_accepted());
        if is_draft {
            continue;
        }
        let doc_dir = document_dir(l, name);
        for c in collect(src) {
            let t = &c.target;
            if is_url(t) || t.starts_with('#') || is_glob(t) || is_pinned(t) {
                continue;
            }
            if is_cidr(t) || is_host_rooted(t) || t.starts_with('/') {
                continue;
            }
            if !looks_like_a_path(t) && !t.ends_with(".md") {
                continue;
            }
            // A markdown link is a link and is always checked. A backticked
            // token is only a repository path when it starts at the repository.
            if !c.document_relative && !t.starts_with("./") && !t.starts_with("../") {
                let first = t.split('/').next().unwrap_or("");
                if !roots.iter().any(|r| r == first) {
                    continue;
                }
            }
            if escapes_repo(l, &doc_dir, &c) || resolves(l, &doc_dir, &c) {
                continue;
            }
            let base = base_for(l, &doc_dir, &c);
            let normalised = repo_relative(l, base, without_line_suffix(without_fragment(t)));
            if links.iter().any(|g| normalised.starts_with(g.as_str())) {
                continue;
            }
            pending.push((name.clone(), c.line, t.to_string()));
        }
    }

    let excluded = ignored(
        &l.root,
        &pending
            .iter()
            .map(|(_, _, t)| t.clone())
            .collect::<Vec<_>>(),
    );
    for (name, line, t) in pending {
        if excluded.contains(&t) {
            continue;
        }
        out.push(
            Finding::new(
                Layer::C,
                "links-resolve",
                format!(
                    "`{}` does not exist. If it was true once, pin it as `{}@<rev>` \
                     rather than deleting the citation",
                    t, t
                ),
            )
            .at(name, line),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets(src: &str) -> Vec<String> {
        collect(src).into_iter().map(|c| c.target).collect()
    }

    #[test]
    fn brace_expansion_is_a_glob() {
        // `bootstrap/configs/{homelab,monitor,nas}.yaml` names three files at
        // once and no single path was ever meant.
        assert!(is_glob("bootstrap/configs/{homelab,monitor,nas}.yaml"));
        assert!(is_glob("kubernetes/**/*.yaml"));
        assert!(!is_glob("docs/adr/0001-x.md"));
    }

    #[test]
    fn a_cidr_is_not_a_file() {
        assert!(is_cidr("10.244.0.0/16"));
        assert!(!is_cidr("docs/adr"));
        assert!(!is_cidr("10.244.0.0/abc"));
    }

    #[test]
    fn a_host_rooted_reference_is_not_a_repository_path() {
        assert!(is_host_rooted(
            "ghcr.io/fredericrous/homelab/manifests:latest"
        ));
        assert!(is_host_rooted("git.daddyshome.fr/fredericrous/aval"));
        assert!(!is_host_rooted("docs/adr/0001-x.md"));
        // A dotfile directory is a real path, and starts with the dot rather
        // than merely containing one.
        assert!(!is_host_rooted(".github/workflows/ci.yaml"));
    }

    #[test]
    fn a_line_suffix_addresses_a_place_inside_a_file() {
        assert_eq!(
            without_line_suffix("infrastructure/homelab/cilium-values.yaml:172"),
            "infrastructure/homelab/cilium-values.yaml"
        );
        assert_eq!(
            without_line_suffix("bootstrap/pkg/workflow/monitor.go:23-36"),
            "bootstrap/pkg/workflow/monitor.go"
        );
        // A tag is not a line number, and must not be trimmed into one.
        assert_eq!(without_line_suffix("image:latest"), "image:latest");
    }

    #[test]
    fn a_markdown_link_resolves_against_the_document() {
        // A sibling ADR link carries no `./`, and treating it as root-relative
        // made every cross-reference in the corpus read as dangling.
        let c = &collect("see [0007](0007-ai-ops.md)\n")[0];
        assert!(c.document_relative);
        let c = &collect("see `docs/adr/0007-ai-ops.md`\n")[0];
        assert!(!c.document_relative);
    }

    #[test]
    fn finds_markdown_links_and_backticked_paths() {
        let t = targets("see [x](../architecture/x.md) and `kubernetes/app/y.yaml`\n");
        assert_eq!(t, ["../architecture/x.md", "kubernetes/app/y.yaml"]);
    }

    #[test]
    fn ignores_fenced_code() {
        let t = targets("before\n```\n`a/b.yaml`\n[x](c/d.md)\n```\nafter `e/f.md`\n");
        assert_eq!(t, ["e/f.md"]);
    }

    #[test]
    fn ignores_frontmatter() {
        let t = targets("---\nid: ADR-1\n---\n`a/b.md`\n");
        assert_eq!(t, ["a/b.md"]);
    }

    #[test]
    fn a_backticked_word_without_a_slash_is_not_a_path() {
        assert!(targets("the `accepted` status\n").is_empty());
    }

    #[test]
    fn urls_globs_fragments_and_pins_are_not_checked() {
        for s in [
            "[a](https://example.com/x)",
            "`kubernetes/**/*.yaml`",
            "[a](#section)",
            "`kubernetes/data-storage/garage/@1f2fa935`",
        ] {
            let cites = collect(&format!("{}\n", s));
            for c in cites {
                assert!(
                    is_url(&c.target)
                        || is_glob(&c.target)
                        || c.target.starts_with('#')
                        || is_pinned(&c.target),
                    "{} should be skipped",
                    c.target
                );
            }
        }
    }

    #[test]
    fn a_fragment_is_stripped_before_the_file_is_checked() {
        assert_eq!(without_fragment("a/b.md#head"), "a/b.md");
        assert_eq!(without_fragment("a/b.md"), "a/b.md");
    }
}

#[cfg(test)]
mod base_tests {
    use super::*;

    fn loaded_at(root: &str) -> Loaded {
        // Only `root` is read by the functions under test.
        Loaded {
            graph: aval_core::graph::Graph::build(aval_core::model::Corpus::new(
                aval_core::model::Registry::empty("docs/adr"),
                Vec::new(),
            ))
            .expect("empty corpus"),
            root: PathBuf::from(root),
            adr_dir: PathBuf::from(root).join("docs/adr"),
            files: Vec::new(),
            packs: Vec::new(),
        }
    }

    /// The regression this exists for. A backticked token resolves from the
    /// repository root, and the gitlink skip compares a repository-relative
    /// prefix. Computing that prefix from the document's directory instead
    /// produced `docs/adr/vault-transit-unseal-operator/...`, which matched no
    /// submodule, so every citation into a submodule was reported dangling.
    /// Found by running against homelab rather than by a test, which is why
    /// there is now a test.
    #[test]
    fn a_root_relative_token_is_not_rebased_on_the_document() {
        let l = loaded_at("/repo");
        let doc_dir = PathBuf::from("/repo/docs/adr");
        let c = Cite {
            target: "vault-transit-unseal-operator/README.md".into(),
            line: 1,
            document_relative: false,
        };
        let base = base_for(&l, &doc_dir, &c);
        assert_eq!(base, Path::new("/repo"));
        assert_eq!(
            repo_relative(&l, base, &c.target),
            "vault-transit-unseal-operator/README.md"
        );
    }

    #[test]
    fn a_document_relative_link_resolves_beside_its_document() {
        let l = loaded_at("/repo");
        let doc_dir = PathBuf::from("/repo/docs/architecture");
        let c = Cite {
            target: "./diagram.svg".into(),
            line: 1,
            document_relative: true,
        };
        let base = base_for(&l, &doc_dir, &c);
        assert_eq!(base, Path::new("/repo/docs/architecture"));
        assert_eq!(
            repo_relative(&l, base, &c.target),
            "docs/architecture/diagram.svg"
        );
    }

    #[test]
    fn a_parent_reference_climbs_out_of_the_documents_directory() {
        let l = loaded_at("/repo");
        let doc_dir = PathBuf::from("/repo/docs/adr");
        let c = Cite {
            target: "../architecture/storage.md".into(),
            line: 1,
            document_relative: true,
        };
        let base = base_for(&l, &doc_dir, &c);
        assert_eq!(
            repo_relative(&l, base, &c.target),
            "docs/architecture/storage.md"
        );
    }

    #[test]
    fn document_dir_is_the_documents_own_directory() {
        let l = loaded_at("/repo");
        assert_eq!(
            document_dir(&l, "docs/specs/notes.md"),
            PathBuf::from("/repo/docs/specs")
        );
        assert_eq!(document_dir(&l, "NOTES.md"), PathBuf::from("/repo"));
    }
}
