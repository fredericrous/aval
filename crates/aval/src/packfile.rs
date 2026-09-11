//! `aval pack` — publishing this corpus's declarations, and keeping the
//! published copy honest.
//!
//! The same shape as `heads`, for the same reason: a generated file that a
//! command writes and a check verifies. `pack-fresh` exists because a producer
//! that publishes an `aval.pack` disagreeing with its own records hands every
//! consumer a decision nobody made. The failure would be invisible at both
//! ends — the producer resolves from its records, the consumer from the pack,
//! and only somebody holding both would notice.
//!
//! Freshness is decided after canonicalisation, exactly as `HEADS.md` is, so a
//! repository may run a formatter over the file without the gate calling it
//! stale.

use crate::load::Loaded;
use aval_core::model::{Finding, Layer};
use aval_core::pack;
use aval_core::project;
use std::path::PathBuf;

pub fn path(l: &Loaded) -> PathBuf {
    l.root.join(pack::FILE)
}

/// Whether this corpus publishes at all.
///
/// Only a repository that already has an `aval.pack` is held to keeping it
/// current. Publishing is a choice, and a check that fired on every corpus
/// that had never published would turn a new check into a new failure for
/// seven repositories that were clean.
pub fn publishes(l: &Loaded) -> bool {
    path(l).is_file()
}

/// What a consumer would receive.
///
/// Vendored records are excluded. A pack states what *this* repository
/// decided; re-exporting what it borrowed would let one consumer's copy of a
/// decision reach another by a route neither of them chose, and the id prefix
/// would stack until nothing matched the record it names.
pub fn render(l: &Loaded) -> String {
    let c = l.graph.corpus();
    let mut own = c.clone();
    own.adrs.retain(|a| !a.is_vendored());
    own.registry.keys.retain(|k| {
        !l.packs
            .iter()
            .any(|p| p.keys.iter().any(|pk| pk.name == k.name))
    });
    pack::render(&own)
}

pub fn findings(l: &Loaded) -> Vec<Finding> {
    if !publishes(l) {
        return Vec::new();
    }
    let want = project::canonicalise(&render(l));
    let have = std::fs::read_to_string(path(l))
        .map(|t| project::canonicalise(&t))
        .unwrap_or_default();
    if have == want {
        return Vec::new();
    }
    vec![Finding::new(
        Layer::C,
        "pack-fresh",
        "the published declarations are not the ones this corpus states; \
         run `aval pack --write`",
    )
    .in_file(pack::FILE)]
}

pub enum Wrote {
    Unchanged,
    Written,
}

pub fn write(l: &Loaded) -> std::io::Result<Wrote> {
    let want = render(l);
    if let Ok(have) = std::fs::read_to_string(path(l)) {
        if project::canonicalise(&have) == project::canonicalise(&want) {
            return Ok(Wrote::Unchanged);
        }
    }
    std::fs::write(path(l), want)?;
    Ok(Wrote::Written)
}
