//! The glob dialect of `areas:` and `disclaims:` (SEMANTICS section 2.5).
//!
//! Hand-rolled because this workspace takes no dependencies, and small because
//! the dialect is: literals, `?`, `*` within a segment, and `**` as a whole
//! segment. No character classes, no braces, no negation — each would be a
//! second way to say something the reviewer of an `areas:` diff has to parse
//! in their head, and none is needed to describe a part of a tree.
//!
//! Paths and globs are repository-relative and `/`-separated, and comparison
//! is case-sensitive: git's paths are, whatever the filesystem does.
//! `*` and `?` match a leading `.`, which is git's pathspec behaviour rather
//! than a shell's.

/// Why `glob` is not a usable area glob, or `None` when it is.
pub fn bad(glob: &str) -> Option<String> {
    if glob.is_empty() {
        return Some("an area glob cannot be empty".into());
    }
    if glob.starts_with('/') {
        return Some(format!(
            "`{}` starts with `/`; areas are repository-relative",
            glob
        ));
    }
    if glob.starts_with("./") {
        return Some(format!("`{}` starts with `./`; drop it", glob));
    }
    if glob.ends_with('/') {
        return Some(format!(
            "`{}` ends with `/`; areas classify files, so write `{}**`",
            glob, glob
        ));
    }
    if glob.starts_with('!') {
        return Some(format!("`{}`: negation is not part of the dialect", glob));
    }
    if let Some(c) = glob.chars().find(|c| "[]{}".contains(*c)) {
        return Some(format!(
            "`{}` uses `{}`; the dialect is literals, `?`, `*` and `**`",
            glob, c
        ));
    }
    for seg in glob.split('/') {
        if seg.is_empty() {
            return Some(format!("`{}` has an empty segment", glob));
        }
        if seg == ".." || seg == "." {
            return Some(format!("`{}` has a `{}` segment", glob, seg));
        }
        if seg.contains("**") && seg != "**" {
            return Some(format!(
                "`{}`: `**` must be a whole segment, as in `a/**/b`",
                glob
            ));
        }
    }
    None
}

/// Whether `path` matches `glob`. A malformed glob matches nothing; `bad`
/// is what reports it.
pub fn matches(glob: &str, path: &str) -> bool {
    if bad(glob).is_some() || path.is_empty() {
        return false;
    }
    let g: Vec<&str> = glob.split('/').collect();
    let p: Vec<&str> = path.split('/').collect();
    segments(&g, &p)
}

fn segments(g: &[&str], p: &[&str]) -> bool {
    match g.split_first() {
        None => p.is_empty(),
        Some((&"**", rest)) => {
            // Zero or more whole segments. A trailing `**` needs at least one
            // segment left, so `src/**` matches what is under `src` and not
            // `src` itself.
            if rest.is_empty() {
                return !p.is_empty();
            }
            (0..=p.len()).any(|i| segments(rest, &p[i..]))
        }
        Some((seg, rest)) => match p.split_first() {
            Some((head, tail)) => segment(seg, head) && segments(rest, tail),
            None => false,
        },
    }
}

/// One segment: `*` any run, `?` one character, anything else itself.
fn segment(g: &str, s: &str) -> bool {
    // Iterative with one backtrack point, the usual wildcard match. Operates
    // on chars so `?` means one character, not one byte.
    let g: Vec<char> = g.chars().collect();
    let s: Vec<char> = s.chars().collect();
    let (mut gi, mut si) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while si < s.len() {
        if gi < g.len() && (g[gi] == '?' || g[gi] == s[si]) {
            gi += 1;
            si += 1;
        } else if gi < g.len() && g[gi] == '*' {
            star = Some((gi, si));
            gi += 1;
        } else if let Some((sg, ss)) = star {
            gi = sg + 1;
            si = ss + 1;
            star = Some((sg, ss + 1));
        } else {
            return false;
        }
    }
    while gi < g.len() && g[gi] == '*' {
        gi += 1;
    }
    gi == g.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_star_is_zero_or_more_segments() {
        assert!(matches("a/**/b", "a/b"));
        assert!(matches("a/**/b", "a/x/y/b"));
        assert!(!matches("a/**/b", "a/x/c"));
        assert!(matches("src/**", "src/main.rs"));
        assert!(matches("src/**", "src/a/b/c.rs"));
        assert!(!matches("src/**", "src"));
        assert!(matches("**", "anything/at/all"));
        assert!(matches("**/main.go", "main.go"));
        assert!(matches("**/main.go", "cmd/x/main.go"));
    }

    #[test]
    fn star_and_question_stay_in_a_segment() {
        assert!(matches("cmd/*/main.go", "cmd/aval/main.go"));
        assert!(!matches("cmd/*/main.go", "cmd/a/b/main.go"));
        assert!(!matches("*", "a/b"));
        assert!(matches("*.md", "README.md"));
        assert!(matches("?.rs", "a.rs"));
        assert!(!matches("?.rs", "ab.rs"));
    }

    #[test]
    fn star_matches_a_leading_dot() {
        assert!(matches("*", ".github"));
        assert!(matches("**", ".github/workflows/ci.yaml"));
        assert!(matches("?git", ".git"));
    }

    #[test]
    fn case_sensitive() {
        assert!(!matches("Web/**", "web/a.ts"));
        assert!(!matches("*.MD", "a.md"));
    }

    #[test]
    fn malformed_globs() {
        for g in [
            "", "/a", "./a", "a/", "!a", "a/[b]", "{a,b}", "a/../b", "a/./b", "a//b", "a**", "**b",
            "a/**b",
        ] {
            assert!(bad(g).is_some(), "{g:?} should be refused");
            assert!(!matches(g, "a/b"));
        }
        for g in ["a", "**", "a/**", "a/**/b", "*.md", ".github/**", "a?c"] {
            assert!(bad(g).is_none(), "{g:?} should be accepted");
        }
    }
}
