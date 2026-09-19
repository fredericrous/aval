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
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A wall-clock budget every git call in this process shares. Set by
/// `add --check --budget N` for callers that must not wait on a remote —
/// the session hook above all — and unset otherwise, when a person is at
/// the keyboard and can press ^C. Process-wide because the budget is a
/// property of the invocation, not of any one call, and threading it
/// through every signature would put a clock in functions that have no
/// business knowing one.
static DEADLINE: Mutex<Option<Instant>> = Mutex::new(None);

pub fn set_budget(seconds: u64) {
    *DEADLINE.lock().expect("deadline lock") = Some(Instant::now() + Duration::from_secs(seconds));
}

fn deadline() -> Option<Instant> {
    *DEADLINE.lock().expect("deadline lock")
}

/// A source, as the user wrote it.
#[derive(Debug)]
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
    // Never a prompt. A hook or a CI job that asks a remote must fail when
    // the remote asks for a password, not hang on a question nobody sees.
    // `GIT_SSH_COMMAND` is set only when the user has not, so a custom ssh
    // survives.
    c.env("GIT_TERMINAL_PROMPT", "0");
    if std::env::var_os("GIT_SSH_COMMAND").is_none() && std::env::var_os("GIT_SSH").is_none() {
        c.env(
            "GIT_SSH_COMMAND",
            "ssh -o BatchMode=yes -o ConnectTimeout=5",
        );
    }
    let Some(deadline) = deadline() else {
        let o = c
            .output()
            .map_err(|e| format!("cannot run git: {} — it has to be on PATH", e))?;
        if !o.status.success() {
            return Err(String::from_utf8_lossy(&o.stderr).trim().to_string());
        }
        return Ok(String::from_utf8_lossy(&o.stdout).to_string());
    };
    // Under a budget: poll, and kill at the deadline. A killed remote call
    // is an `unknown`, never a verdict.
    c.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c
        .spawn()
        .map_err(|e| format!("cannot run git: {} — it has to be on PATH", e))?;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(e) => return Err(format!("waiting for git: {e}")),
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("no answer within the budget".to_string());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let o = child
        .wait_with_output()
        .map_err(|e| format!("waiting for git: {e}"))?;
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

/// Whether `body` is a source on its own, so that what follows an `@` after
/// it can only be a revision.
///
/// The shorthands announce themselves; a URL has a host and then a path; the
/// scp form has a user, a host and a colon; a path looks like one. `git`, the
/// part of `git@github.com:acme/repo.git` before its `@`, is none of these.
fn is_complete_source(body: &str) -> bool {
    if body.starts_with("github:") || body.starts_with("forgejo:") {
        return true;
    }
    if let Some((_, after)) = body.split_once("://") {
        return after.contains('/');
    }
    if let Some((_, after)) = body.split_once('@') {
        return after.contains(':');
    }
    looks_like_path(body)
}

/// Whether a string can be a git ref name at all. A backslash cannot, and
/// neither can whitespace or `~ ^ : ? * [` — so `b\\pack` after the `@` in a
/// Windows path is not a revision, whatever the rest of the spec says.
fn plausible_ref(rev: &str) -> bool {
    !rev.is_empty()
        && !rev
            .chars()
            .any(|c| c.is_whitespace() || "\\~^:?*[".contains(c))
}

/// Split a trailing `@rev`.
///
/// `git@github.com:acme/repo.git` carries an `@` in its userinfo. Splitting on
/// the last `@` outright would read `github.com` as a revision and quietly
/// fetch something else entirely — so the last `@` is a separator only when
/// what precedes it is a complete source and what follows it could be a ref.
/// `repo@feature/test` passes that test where the earlier rule, which looked
/// for `@` only past the last `/`, saw a path called `repo@feature/test` and
/// fetched HEAD. The earlier rule remains as the fallback, and a spec that
/// exists on disk exactly as written is a path, whatever it contains.
fn split_rev(spec: &str) -> Result<(&str, Option<&str>), String> {
    if looks_like_path(spec) && Path::new(spec).exists() {
        return Ok((spec, None));
    }
    if let Some(at) = spec.rfind('@') {
        let (body, rev) = (&spec[..at], &spec[at + 1..]);
        if plausible_ref(rev) && is_complete_source(body) {
            return Ok((body, Some(rev)));
        }
    }
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

/// A full commit id: forty hex digits, or sixty-four on a sha256 repository.
/// A short id is not one — it is a prefix, and only the remote could say
/// whether it is unambiguous.
pub fn is_commit_id(rev: &str) -> bool {
    (rev.len() == 40 || rev.len() == 64) && rev.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Turn a revision into the commit id it names right now.
///
/// A commit id names itself. `ls-remote` takes its last argument as a REF
/// pattern, so a pin that IS a commit — the very thing the ambiguity error
/// below tells a caller to use — matched no ref and was reported as naming
/// nothing. Whether the remote will hand that commit over is `fetch`'s
/// question, and it asks it.
pub fn resolve(s: &Source) -> Result<String, String> {
    if is_commit_id(&s.rev) {
        return Ok(s.rev.to_ascii_lowercase());
    }
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
#[derive(Debug)]
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

    /// Under a budget, a remote that never answers is an error naming the
    /// budget, inside the budget — not a hang. The "remote" is an ssh
    /// command that sleeps, so this runs offline and deterministically.
    #[test]
    fn a_budget_kills_a_remote_call_that_does_not_answer() {
        let dir = std::env::temp_dir().join(format!("aval-budget-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sleeper = dir.join("ssh-that-sleeps.sh");
        std::fs::write(&sleeper, "#!/bin/sh\nsleep 30\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&sleeper, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        // Process-wide, like the budget itself; nothing else in this binary
        // reaches an ssh remote.
        std::env::set_var("GIT_SSH_COMMAND", sleeper.to_string_lossy().to_string());
        set_budget(1);
        let started = Instant::now();
        let got = resolve(&Source {
            label: "git@localhost:nowhere.git".into(),
            url: "git@localhost:nowhere.git".into(),
            rev: "main".into(),
        });
        let elapsed = started.elapsed();
        *DEADLINE.lock().unwrap() = None;
        std::env::remove_var("GIT_SSH_COMMAND");
        let err = got.expect_err("a remote that never answers is not resolved");
        assert!(err.contains("budget"), "{err}");
        assert!(
            elapsed < Duration::from_secs(5),
            "killed at the deadline, not at ssh's leisure: {elapsed:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

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

    /// A branch name with a slash in it is the ordinary case, not an edge:
    /// `feature/x`, `release/2.1`, `user/name/topic`. The earlier splitter
    /// looked for `@` only past the last `/` and read every one of these as a
    /// path called `repo@feature/x`, revision HEAD.
    #[test]
    fn a_revision_may_contain_slashes() {
        let (body, rev) = split_rev("/nowhere/such/repo@feature/test").unwrap();
        assert_eq!(body, "/nowhere/such/repo");
        assert_eq!(rev, Some("feature/test"));

        let s = parse_source("git@github.com:acme/repo.git@feature/x").unwrap();
        assert_eq!(s.url, "git@github.com:acme/repo.git");
        assert_eq!(s.rev, "feature/x");

        let s = parse_source("https://u@host/acme/repo.git@release/2.1").unwrap();
        assert_eq!(s.url, "https://u@host/acme/repo.git");
        assert_eq!(s.rev, "release/2.1");

        let s = parse_source("github:acme/repo@user/name/topic").unwrap();
        assert_eq!(s.rev, "user/name/topic");
    }

    /// The userinfo cases that the slash-tolerant rule must still not split.
    #[test]
    fn userinfo_is_still_not_a_revision() {
        let (body, rev) = split_rev("https://u@host/acme/repo.git").unwrap();
        assert_eq!(body, "https://u@host/acme/repo.git");
        assert_eq!(rev, None);
        let (body, rev) = split_rev("git@github.com:acme/repo.git").unwrap();
        assert_eq!(body, "git@github.com:acme/repo.git");
        assert_eq!(rev, None);
    }

    /// A directory that exists with an `@` in its name is a path, whatever
    /// follows the `@`.
    #[test]
    fn a_path_that_exists_is_never_split() {
        let d = std::env::temp_dir().join(format!("aval-fetch-{}-x@y", std::process::id()));
        let inner = d.join("pack");
        std::fs::create_dir_all(&inner).unwrap();
        let spec = inner.to_string_lossy().to_string();
        let (body, rev) = split_rev(&spec).unwrap();
        assert_eq!(body, spec);
        assert_eq!(rev, None);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A full commit id names itself; `ls-remote` would have matched it as a
    /// ref pattern and found nothing, which is what the ambiguity error told
    /// a caller to do next.
    #[test]
    fn a_commit_id_pin_resolves_to_itself_without_the_network() {
        let id = "0123456789ABCDEF0123456789abcdef01234567";
        assert!(is_commit_id(id));
        assert!(!is_commit_id("0123456"), "a prefix is not an id");
        assert!(!is_commit_id("v1.2.0"));
        let s = Source {
            label: "nowhere".into(),
            url: "/nowhere/at/all".into(),
            rev: id.into(),
        };
        assert_eq!(resolve(&s).unwrap(), id.to_ascii_lowercase());
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
