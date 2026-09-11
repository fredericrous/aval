//! Whether `HEADS.md` states the current heads, and writing it when it does not.
//!
//! Freshness is decided on canonicalised text (SEMANTICS section 12.2), so a
//! repository may run a markdown formatter over the projection without the
//! gate calling it stale. Every difference that is not formatting is still a
//! difference.

use crate::load::{Loaded, HEADS};
use aval_core::model::{Finding, Layer};
use aval_core::project;
use std::path::PathBuf;

pub enum Freshness {
    Current,
    Missing,
    /// The file says something other than the projection.
    Stale(Vec<Difference>),
}

pub enum Difference {
    /// A line the file carries that the projection does not.
    Extra(String),
    /// A line the projection carries that the file does not.
    Absent(String),
    /// Same lines, wrong order. The projection is sorted (section 12) and
    /// nothing else would enforce that if a reordering counted as fresh.
    Order,
}

pub fn path(l: &Loaded) -> PathBuf {
    l.adr_dir.join(HEADS)
}

/// The path as it reads in a finding: relative, so the message is the same
/// wherever the command ran from.
fn display_path(l: &Loaded) -> String {
    format!("{}/{}", l.graph.registry().dir, HEADS)
}

/// Whether this corpus has a projection at all.
///
/// A registry that only vendors packs keeps no records and has no `dir`, so
/// there is nowhere to put `HEADS.md` and nothing about the consumer for it to
/// state. `aval heads` still prints the fleet's heads to stdout, which is what
/// the session hook reads; what does not exist is a file to keep fresh.
pub fn applies(l: &Loaded) -> bool {
    l.graph.registry().has_dir()
}

pub fn freshness(l: &Loaded) -> Freshness {
    if !applies(l) {
        return Freshness::Current;
    }
    let want = project::render(&l.graph);
    let Ok(have) = std::fs::read_to_string(path(l)) else {
        return Freshness::Missing;
    };
    compare(&have, &want)
}

fn compare(have: &str, want: &str) -> Freshness {
    let (ch, cw) = (project::canonicalise(have), project::canonicalise(want));
    if ch == cw {
        return Freshness::Current;
    }
    Freshness::Stale(differences(&ch, &cw))
}

/// What changed, as lines rather than as characters.
///
/// A multiset difference rather than a positional walk: one inserted row would
/// make every later line look wrong, and a list of forty differences for one
/// edit is a list nobody reads. Ordering is reported separately, and only when
/// the two sides carry the same lines.
fn differences(have: &str, want: &str) -> Vec<Difference> {
    let mut out = Vec::new();
    let hl: Vec<&str> = have.lines().collect();
    let wl: Vec<&str> = want.lines().collect();

    let mut remaining: Vec<&str> = wl.clone();
    for line in &hl {
        match remaining.iter().position(|x| x == line) {
            Some(i) => {
                remaining.remove(i);
            }
            // A blank line carries no claim, so reporting one says nothing a
            // reader can act on. It still counts toward the comparison above.
            None if line.trim().is_empty() => {}
            None => out.push(Difference::Extra((*line).to_string())),
        }
    }
    for line in remaining {
        if line.trim().is_empty() {
            continue;
        }
        out.push(Difference::Absent(line.to_string()));
    }
    if out.is_empty() {
        out.push(Difference::Order);
    }
    out
}

/// The Layer C finding, one per difference so each names a line.
pub fn findings(l: &Loaded) -> Vec<Finding> {
    let file = display_path(l);
    let f = |m: String| Finding::new(Layer::C, "heads-fresh", m).in_file(file.clone());
    match freshness(l) {
        Freshness::Current => Vec::new(),
        Freshness::Missing => vec![f("HEADS.md is missing; run `aval heads --write`".into())],
        Freshness::Stale(diffs) => {
            let mut out: Vec<Finding> = diffs
                .iter()
                .take(MAX_REPORTED)
                .map(|d| f(describe(d)))
                .collect();
            if diffs.len() > MAX_REPORTED {
                out.push(f(format!(
                    "and {} further difference(s); run `aval heads --write`",
                    diffs.len() - MAX_REPORTED
                )));
            }
            out
        }
    }
}

const MAX_REPORTED: usize = 10;

/// Canonical rows have no padding, which is unreadable in a message. This is
/// display only; nothing compares the result.
fn readable(l: &str) -> String {
    let t = l.trim();
    if t.starts_with('|') && t.ends_with('|') {
        return t.replace('|', " | ").trim().to_string();
    }
    t.to_string()
}

fn describe(d: &Difference) -> String {
    match d {
        Difference::Extra(l) => {
            format!("HEADS.md says `{}`, which the corpus does not", readable(l))
        }
        Difference::Absent(l) => format!("HEADS.md is missing `{}`", readable(l)),
        Difference::Order => {
            "HEADS.md carries the right rows in the wrong order; the projection is sorted".into()
        }
    }
}

pub enum Wrote {
    /// The file already stated the projection. Its bytes were left alone, so a
    /// formatter's padding survives rather than being undone on every run.
    Unchanged,
    Written,
}

/// Write the projection unless the file already states it.
///
/// SEMANTICS section 9 says Layer C must not affect `heads --write`, and this
/// reads the existing file to decide. The rule's purpose — a stale `HEADS.md`
/// must never prevent regenerating it — holds because anything that is not
/// already current is overwritten, including a file that is unreadable as
/// text. There is no state this cannot repair.
pub fn write(l: &Loaded) -> std::io::Result<Wrote> {
    if matches!(freshness(l), Freshness::Current) {
        return Ok(Wrote::Unchanged);
    }
    std::fs::write(path(l), project::render(&l.graph))?;
    Ok(Wrote::Written)
}
