//! Getting another repository's `aval.pack`, over git and nothing else.
//!
//! # Why git is the transport
//!
//! `aval` links no crates, so it has no TLS stack and no HTTP client, and it
//! must not grow one: this binary runs on the pre-commit path. The only
//! network primitive already in the box is `git`, and it happens to be the
//! right one anyway — it is content-addressed, so a moving `@v1` can be turned
//! into a commit id *before* anything is fetched and whatever arrives can be
//! refused unless it hashes to that id.
//!
//! It also settles the credential question by not asking it. The fleet corpus
//! this was built for is a private repository on one forge, read by consumers
//! on another that sits behind a client certificate. Shelling out to `git`
//! means SSH keys, credential helpers, `.netrc` and mTLS all work already,
//! with the user's own credentials and none of `aval`'s. No token had to be
//! minted for any of it.
//!
//! # Nothing here is on the hook path
//!
//! `aval add` is a setup verb. No hook, no gate and no `resolve` reaches this
//! module: they read a file that is already in the repository. A decision
//! corpus that needed the network to answer a question would be useless on a
//! plane and unusable in CI, which is the same reason the answer is vendored
//! in the first place.
//!
//! The structure here is `amont`'s, deliberately and almost line for line. Its
//! version of this has been in use across this fleet for months, the cases it
//! guards against were each found the hard way, and a second shape would be a
//! second thing to get wrong.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A source, as the user wrote it.
pub struct Source {
    /// The spec minus any `@rev`. Identity, and what is reported.
    pub label: String,
    pub url: String,
    pub rev: String,
}

fn git(args: &[&str], in_dir: Option<&Path>) -> Result<String, String> {
    let mut c = Command::new("git");
    c.args(args);
    if let Some(d) = in_dir {
        c.current_dir(d);
    }
    let o = c
        .output()
        .map_err(|e| format!("cannot run git: {} — it has to be on PATH", e))?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&o.stdout).to_string())
}

/// Whether a spec is a filesystem path.
///
/// A question about the string, not about this host: a Windows path handed to
/// a test on Linux is still a Windows path, and `Path::is_absolute` would
/// disagree and turn it into a URL.
fn looks_like_path(s: &str) -> bool {
    let b = s.as_bytes();
    if s.starts_with('/') || s.starts_with("\\\\") {
        return true;
    }
    if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
    {
        return true;
    }
    Path::new(s).exists()
}

/// Split a trailing `@rev`, but only past the last path separator.
///
/// `git@github.com:acme/repo.git` carries an `@` in its userinfo. Splitting on
/// the last `@` outright would read `github.com` as a revision and quietly
/// fetch something else entirely.
fn split_rev(spec: &str) -> Result<(&str, Option<&str>), String> {
    let sep = spec.rfind(['/', '\\']).map_or(0, |i| i + 1);
    match spec[sep..].rfind('@') {
        None => Ok((spec, None)),
        Some(rel) => {
            let at = sep + rel;
            let rev = &spec[at + 1..];
            if rev.is_empty() {
                return Err(format!("{}: `@` with no revision after it", spec));
            }
            Ok((&spec[..at], Some(rev)))
        }
    }
}

pub fn parse_source(spec: &str) -> Result<Source, String> {
    let (body, rev) = split_rev(spec)?;
    let url = if let Some(rest) = body.strip_prefix("github:") {
        let n = rest.split('/').count();
        if n != 2 || rest.split('/').any(|s| s.is_empty()) {
            return Err(format!("{}: `github:` takes owner/repo", spec));
        }
        format!("https://github.com/{}.git", rest)
    } else if let Some(rest) = body.strip_prefix("forgejo:") {
        // Three segments, because the host is exactly what this shorthand
        // exists for: a self-hosted forge cannot be assumed.
        let n = rest.split('/').count();
        if n != 3 || rest.split('/').any(|s| s.is_empty()) {
            return Err(format!("{}: `forgejo:` takes host/owner/repo", spec));
        }
        format!("https://{}.git", rest)
    } else if body.contains("://") || body.contains('@') || looks_like_path(body) {
        body.to_string()
    } else {
        return Err(format!(
            "{}: not a git URL — use github:owner/repo, forgejo:host/owner/repo, \
             a full URL, or a path",
            spec
        ));
    };
    Ok(Source {
        label: body.to_string(),
        url,
        rev: rev.unwrap_or("HEAD").to_string(),
    })
}

/// Commit ids `ls-remote` reported, minus peel lines.
fn named_refs(out: &str) -> Vec<(String, String)> {
    out.lines()
        .filter_map(|l| {
            let (id, name) = l.split_once('\t')?;
            // `refs/tags/v1^{}` is the commit a tag points at, listed beside
            // the tag object. It is the same answer twice, not two answers.
            if name.ends_with("^{}") {
                return None;
            }
            Some((name.to_string(), id.to_string()))
        })
        .collect()
}

/// Turn a revision into the commit id it names right now.
pub fn resolve(s: &Source) -> Result<String, String> {
    let out = git(&["ls-remote", &s.url, &s.rev], None).map_err(|e| {
        format!(
            "{}: cannot reach the remote, or {} names nothing ({})",
            s.label, s.rev, e
        )
    })?;
    let mut refs = named_refs(&out);
    refs.sort();
    // Deduplicate by id, not by name. Any non-bare clone advertises `HEAD` and
    // `refs/remotes/origin/HEAD` together, so adding a local checkout as a
    // source — the offline case, and the most natural one to try — was refused
    // over two names for one commit.
    let mut ids: Vec<String> = refs.iter().map(|(_, i)| i.clone()).collect();
    ids.sort();
    ids.dedup();
    match ids.len() {
        0 => Err(format!(
            "{}: {} names nothing on that remote",
            s.label, s.rev
        )),
        1 => Ok(ids.remove(0)),
        n => {
            let names: Vec<String> = refs
                .iter()
                .map(|(n, i)| format!("{} @ {}", n, short(i)))
                .collect();
            Err(format!(
                "{}: {} is ambiguous — it names {} different commits on that \
                 remote ({}); name a commit id instead",
                s.label,
                s.rev,
                n,
                names.join(", ")
            ))
        }
    }
}

pub fn short(id: &str) -> &str {
    &id[..id.len().min(7)]
}

/// Fetch the pack at a resolved id, and refuse anything else.
///
/// The ref is fetched rather than the bare id because fetching an id is not
/// universally allowed (`uploadpack.allowAnySHA1InWant`); comparing what
/// arrived against what was resolved catches a ref that moved between the two
/// calls, which is the case the id exists to close.
pub fn fetch(s: &Source, id: &str, into: &Path, file: &str) -> Result<String, String> {
    std::fs::create_dir_all(into).map_err(|e| format!("{}: {}", into.display(), e))?;
    git(&["init", "-q"], Some(into)).map_err(|e| format!("{}: {}", s.label, e))?;
    git(&["fetch", "-q", "--depth", "1", &s.url, &s.rev], Some(into))
        .map_err(|e| format!("{}: cannot fetch {} ({})", s.label, s.rev, e))?;
    let got = git(&["rev-parse", "FETCH_HEAD"], Some(into))
        .map_err(|e| format!("{}: {}", s.label, e))?
        .trim()
        .to_string();
    if got != id {
        return Err(format!(
            "{}: {} moved while we were reading it ({} → {}) — nothing was \
             written; run the same command again",
            s.label, s.rev, id, got
        ));
    }
    git(&["show", &format!("FETCH_HEAD:{}", file)], Some(into))
        .map_err(|_| format!("{}: has no {} at {}", s.label, file, short(id)))
}

/// A scratch directory that removes itself, on the failure path too.
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new(tag: &str) -> Scratch {
        Scratch(std::env::temp_dir().join(format!("aval-pack-{}-{}", std::process::id(), tag)))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorthands_and_urls_become_git_urls() {
        let s = parse_source("github:fredericrous/decisions").unwrap();
        assert_eq!(s.url, "https://github.com/fredericrous/decisions.git");
        assert_eq!(s.label, "github:fredericrous/decisions");
        assert_eq!(s.rev, "HEAD");

        let s = parse_source("forgejo:git.daddyshome.fr/fredericrous/x@v2").unwrap();
        assert_eq!(s.url, "https://git.daddyshome.fr/fredericrous/x.git");
        assert_eq!(s.rev, "v2");
    }

    /// The one that would fetch the wrong repository in silence.
    #[test]
    fn an_ssh_url_keeps_its_userinfo() {
        let s = parse_source("git@github.com:acme/repo.git").unwrap();
        assert_eq!(s.url, "git@github.com:acme/repo.git");
        assert_eq!(s.rev, "HEAD");
        let s = parse_source("git@github.com:acme/repo.git@v1").unwrap();
        assert_eq!(s.url, "git@github.com:acme/repo.git");
        assert_eq!(s.rev, "v1");
    }

    #[test]
    fn a_path_is_a_source_on_any_platform() {
        assert!(looks_like_path("/tmp/pack"));
        assert!(looks_like_path("C:\\packs\\fleet"));
        assert!(looks_like_path("\\\\server\\share\\fleet"));
        assert!(!looks_like_path("acme/decisions"));
    }

    #[test]
    fn a_windows_path_with_an_at_sign_is_not_split_on_it() {
        let (body, rev) = split_rev("C:\\a@b\\pack").unwrap();
        assert_eq!(body, "C:\\a@b\\pack");
        assert_eq!(rev, None);
    }

    #[test]
    fn a_source_that_is_not_a_url_is_refused() {
        for bad in [
            "",
            "acme/decisions",
            "github:acme",
            "github:acme/repo/x",
            "x@",
        ] {
            assert!(parse_source(bad).is_err(), "{} should be refused", bad);
        }
    }

    #[test]
    fn a_peeled_tag_line_is_not_a_second_ref() {
        let out = "aaa\trefs/tags/v1\nbbb\trefs/tags/v1^{}\n";
        let r = named_refs(out);
        assert_eq!(r.len(), 1);
        // The tag object's id, which is what FETCH_HEAD will record.
        assert_eq!(r[0].1, "aaa");
    }
}
