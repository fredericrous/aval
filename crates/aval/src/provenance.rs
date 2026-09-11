//! Which commit introduced a decision entry.
//!
//! Best-effort by design. SEMANTICS section 13: a staged ADR has no commit, a
//! shallow checkout has no history, and an exported corpus is not a repository
//! at all. **Missing history must never turn a detectable contradiction into a
//! tool failure**, so every path below ends in a value rather than an error and
//! the verdict's exit code is unaffected.
//!
//! Provenance attributes the ENTRY, not the file: an ADR can gain a decision
//! long after the file was created, and blaming the file would name the wrong
//! commit in exactly the case that matters.

use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    /// The abbreviated commit that last wrote this entry's line.
    Committed(String),
    /// Present in the working tree, not yet committed.
    Uncommitted,
    /// No repository, no history, or git would not answer.
    Unavailable,
}

impl Provenance {
    pub fn token(&self) -> &'static str {
        match self {
            Provenance::Committed(_) => "committed",
            Provenance::Uncommitted => "uncommitted",
            Provenance::Unavailable => "unavailable",
        }
    }
}

/// Blame one line. `git blame` reports an all-zero SHA for a line that is not
/// yet committed, which is exactly the distinction section 13 asks for.
pub fn for_line(dir: &Path, file: &Path, line: usize) -> Provenance {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .arg("blame")
        .arg("-L")
        .arg(format!("{},{}", line, line))
        .arg("--porcelain")
        .arg("--")
        .arg(file)
        .output();
    let Ok(out) = out else {
        return Provenance::Unavailable;
    };
    if !out.status.success() {
        return Provenance::Unavailable;
    }
    let Ok(text) = String::from_utf8(out.stdout) else {
        return Provenance::Unavailable;
    };
    let Some(first) = text.lines().next() else {
        return Provenance::Unavailable;
    };
    let Some(sha) = first.split_whitespace().next() else {
        return Provenance::Unavailable;
    };
    if sha.len() < 7 {
        return Provenance::Unavailable;
    }
    if sha.chars().all(|c| c == '0') {
        return Provenance::Uncommitted;
    }
    Provenance::Committed(sha[..8].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_that_is_not_a_repository_is_unavailable() {
        // The failure mode that must not escalate: no repository, no panic, no
        // error, just a value the caller can render.
        let p = for_line(Path::new("/"), Path::new("nope.md"), 1);
        assert_eq!(p, Provenance::Unavailable);
        assert_eq!(p.token(), "unavailable");
    }

    #[test]
    fn a_missing_file_in_a_real_repository_is_unavailable() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert_eq!(
            for_line(here, Path::new("does-not-exist.md"), 1),
            Provenance::Unavailable
        );
    }
}
