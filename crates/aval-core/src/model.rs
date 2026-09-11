//! The types the whole tool agrees on, and the finding it reports.
//!
//! Everything here is data. No I/O, no clocks, no git: SEMANTICS section 12.

use std::fmt;

/// The distinguished default scope. Written `*` and never listed in a registry.
pub const DEFAULT_SCOPE: &str = "*";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Draft,
    Accepted,
}

impl Status {
    pub fn parse(s: &str) -> Option<Status> {
        match s {
            "draft" => Some(Status::Draft),
            "accepted" => Some(Status::Accepted),
            _ => None,
        }
    }

    pub fn is_accepted(self) -> bool {
        matches!(self, Status::Accepted)
    }
}

/// What an entry says about its slot. Exactly one of the two, never both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    /// The answer for this slot.
    Choice(String),
    /// This slot deliberately has no answer.
    Retire,
}

/// One decision one ADR makes: SEMANTICS section 1.4.
#[derive(Debug, Clone)]
pub struct Entry {
    pub key: String,
    pub scope: String,
    pub kind: EntryKind,
    pub first: bool,
    pub replaces: Vec<String>,
    pub overrides: Option<String>,
    pub reason: Option<String>,
    pub line: usize,
}

impl Entry {
    pub fn is_retire(&self) -> bool {
        matches!(self.kind, EntryKind::Retire)
    }

    pub fn choice(&self) -> Option<&str> {
        match &self.kind {
            EntryKind::Choice(c) => Some(c),
            EntryKind::Retire => None,
        }
    }

    /// The slot this entry occupies.
    pub fn slot(&self) -> Slot<'_> {
        Slot {
            key: &self.key,
            scope: &self.scope,
        }
    }
}

/// A `(key, scope)` pair. Resolution answers about slots, not about keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slot<'a> {
    pub key: &'a str,
    pub scope: &'a str,
}

impl fmt::Display for Slot<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.scope == DEFAULT_SCOPE {
            write!(f, "{}", self.key)
        } else {
            write!(f, "{}@{}", self.key, self.scope)
        }
    }
}

#[derive(Debug, Clone)]
pub struct Adr {
    pub id: String,
    pub status: Status,
    pub decisions: Vec<Entry>,
    /// Basename, for messages and for `id-matches-filename`.
    pub file: String,
}

impl Adr {
    pub fn entry_at(&self, slot: Slot<'_>) -> Option<&Entry> {
        self.decisions
            .iter()
            .find(|e| e.key == slot.key && e.scope == slot.scope)
    }
}

#[derive(Debug, Clone)]
pub struct KeyDef {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Registry {
    /// Where ADR files live, relative to the registry.
    pub dir: String,
    pub scopes: Vec<String>,
    pub keys: Vec<KeyDef>,
}

impl Registry {
    pub fn has_key(&self, k: &str) -> bool {
        self.keys.iter().any(|d| d.name == k)
    }

    pub fn has_scope(&self, s: &str) -> bool {
        s == DEFAULT_SCOPE || self.scopes.iter().any(|d| d == s)
    }
}

/// Which of the three layers a finding belongs to: SEMANTICS section 9.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    /// The graph cannot be built.
    A,
    /// The graph is fine; the corpus disagrees with itself.
    B,
    /// Facts about the repository around the graph. Never affects resolution.
    C,
}

impl Layer {
    pub fn as_str(self) -> &'static str {
        match self {
            Layer::A => "A",
            Layer::B => "B",
            Layer::C => "C",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub layer: Layer,
    /// The check id, as listed in SEMANTICS section 10.
    pub check: &'static str,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub message: String,
}

impl Finding {
    pub fn new(layer: Layer, check: &'static str, message: impl Into<String>) -> Self {
        Finding {
            layer,
            check,
            file: None,
            line: None,
            message: message.into(),
        }
    }

    pub fn at(mut self, file: impl Into<String>, line: usize) -> Self {
        self.file = Some(file.into());
        self.line = Some(line);
        self
    }

    pub fn in_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.file, self.line) {
            (Some(p), Some(l)) => write!(f, "{}:{}: ", p, l)?,
            (Some(p), None) => write!(f, "{}: ", p)?,
            _ => {}
        }
        write!(f, "{} [{}]", self.message, self.check)
    }
}

/// A parsed corpus that has not yet been validated as a graph.
#[derive(Debug, Clone)]
pub struct Corpus {
    pub registry: Registry,
    pub adrs: Vec<Adr>,
}

impl Corpus {
    pub fn adr(&self, id: &str) -> Option<&Adr> {
        self.adrs.iter().find(|a| a.id == id)
    }

    /// Every slot any entry occupies, sorted and deduplicated.
    pub fn slots(&self) -> Vec<Slot<'_>> {
        let mut v: Vec<Slot<'_>> = self
            .adrs
            .iter()
            .flat_map(|a| a.decisions.iter().map(|e| e.slot()))
            .collect();
        v.sort();
        v.dedup();
        v
    }
}

/// Optimal string alignment distance, for the advisory did-you-mean on exit 7.
///
/// Levenshtein would do, except that it scores a transposition as two edits,
/// and a transposition is the typo people actually make: `clodu` for `cloud`
/// would then cost the same as two unrelated substitutions and fall outside any
/// budget tight enough to be useful. Counting it as one edit is the whole
/// reason this is not the three-line version.
///
/// A suggestion never resolves anything (SEMANTICS section 5); this only picks
/// which name to print.
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    // Three rows, because a transposition looks back two.
    let mut prev2: Vec<usize> = vec![0; b.len() + 1];
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(prev2[j - 2] + 1);
            }
            cur[j] = best;
        }
        std::mem::swap(&mut prev2, &mut prev);
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// The closest candidate, when one is close enough to be worth printing.
pub fn suggest<'a, I: IntoIterator<Item = &'a str>>(needle: &str, hay: I) -> Option<&'a str> {
    // A third of the length, so `clodu` suggests `cloud` and `x` suggests
    // nothing. A wrong suggestion is worse than none: it invites a caller to
    // "correct" and proceed, which section 5 forbids.
    let budget = (needle.chars().count() / 3).max(1);
    hay.into_iter()
        .map(|c| (edit_distance(needle, c), c))
        .filter(|(d, _)| *d <= budget)
        .min_by_key(|(d, c)| (*d, c.len()))
        .map(|(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_displays_the_default_scope_as_a_bare_key() {
        let s = Slot {
            key: "api.gateway",
            scope: DEFAULT_SCOPE,
        };
        assert_eq!(s.to_string(), "api.gateway");
        let s = Slot {
            key: "api.gateway",
            scope: "cloud",
        };
        assert_eq!(s.to_string(), "api.gateway@cloud");
    }

    #[test]
    fn suggest_finds_a_near_miss() {
        assert_eq!(suggest("clodu", ["cloud", "homelab"]), Some("cloud"));
        assert_eq!(
            suggest("storage.objectstore", ["storage.object-store", "x.y"]),
            Some("storage.object-store")
        );
    }

    #[test]
    fn a_transposition_costs_one_edit() {
        // The reason this is optimal string alignment and not Levenshtein.
        assert_eq!(edit_distance("clodu", "cloud"), 1);
        assert_eq!(edit_distance("cloud", "cloud"), 0);
        assert_eq!(edit_distance("", "abc"), 3);
    }

    #[test]
    fn suggest_declines_when_nothing_is_close() {
        assert_eq!(suggest("zzzzzzzz", ["cloud", "homelab"]), None);
    }

    #[test]
    fn a_finding_renders_with_its_check_id() {
        let f =
            Finding::new(Layer::A, "id-unique", "two documents claim ADR-0011").at("0011-a.md", 2);
        assert_eq!(
            f.to_string(),
            "0011-a.md:2: two documents claim ADR-0011 [id-unique]"
        );
    }
}
