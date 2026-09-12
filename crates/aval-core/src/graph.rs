//! The decision graph: the Layer A checks that need more than one document,
//! then heads, resolution and derived status.
//!
//! A `Graph` can only be constructed from a corpus that passed Layer A, so
//! every method below operates on a model that is known to be well formed.
//! That is what keeps Layer B reachable: competing heads are a verdict about
//! the decisions, not a failure to build the graph (SEMANTICS section 9).

use crate::model::*;
use std::collections::BTreeSet;
use std::fmt;

/// A resolution result. Carries a stable machine token and a stable note, the
/// shape `PolicyDecision` uses as `rule_fired` plus `reason`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Active {
        adr: String,
        choice: String,
        matched_scope: String,
        /// True when the answer came from the default scope by fallback.
        inherited: bool,
        reason: Option<String>,
    },
    Undecided,
    Retired {
        adr: String,
        matched_scope: String,
        inherited: bool,
        reason: Option<String>,
    },
    Contradiction {
        heads: Vec<String>,
        /// The slot that actually disagreed, which may be the default scope
        /// reached by fallback rather than the one asked about.
        matched_scope: String,
    },
    Unknown {
        what: Unknown,
        name: String,
        suggestion: Option<String>,
    },
}

/// The graph contradicts itself: a slot is occupied and has no head.
///
/// Deliberately NOT a `Verdict`. It used to be one, carrying exit 3 — a
/// FAILURE code — inside an enum whose every other variant is an answer, which
/// made "the tool broke" indistinguishable from "here is what was decided" at
/// the type level. Every caller then had to remember a variant that means the
/// opposite of the others, and one did not: the MCP surface reported it as
/// `isError: false`, against the rule SEMANTICS section 14.1 states.
///
/// Only a replacement cycle can produce it, and Layer A rejects those, so this
/// is unreachable against a corpus that loaded. That is the argument for
/// keeping it out of the success type rather than for trusting callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inconsistent {
    pub message: String,
}

impl fmt::Display for Inconsistent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Inconsistent {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unknown {
    Key,
    Scope,
    /// The scope is declared and the key is registered, but the registry does
    /// not apply that key at that scope. The question is malformed rather than
    /// unanswered, so it is never allowed to fall back to the default scope and
    /// return an answer about a different axis.
    ScopeForKey {
        declared: Vec<String>,
    },
}

impl Unknown {
    pub fn as_str(&self) -> &'static str {
        match self {
            Unknown::Key => "key",
            Unknown::Scope => "scope",
            Unknown::ScopeForKey { .. } => "scope-for-key",
        }
    }
}

/// The scopes a restricted key admits, for a message. An empty list is a
/// meaningful declaration, not a missing one: the key is decided fleet-wide
/// only, so say that rather than printing nothing.
fn scope_list(declared: &[String]) -> String {
    if declared.is_empty() {
        "the default scope only".to_string()
    } else {
        declared.join(", ")
    }
}

impl Verdict {
    /// The machine token. Part of the contract: SEMANTICS section 15.
    pub fn token(&self) -> &'static str {
        match self {
            Verdict::Active { .. } => "active",
            Verdict::Undecided => "undecided",
            Verdict::Retired { .. } => "retired",
            Verdict::Contradiction { .. } => "contradiction",
            Verdict::Unknown { .. } => "unknown",
        }
    }

    /// The exit code. SEMANTICS section 14: verdicts start at 4 so that no
    /// verdict shares a range with a failure to reach one.
    pub fn exit(&self) -> i32 {
        match self {
            Verdict::Active { .. } => 0,
            Verdict::Undecided => 4,
            Verdict::Contradiction { .. } => 5,
            Verdict::Retired { .. } => 6,
            Verdict::Unknown { .. } => 7,
        }
    }

    /// The human note. Stable, asserted by fixtures, and a breaking change to
    /// alter.
    pub fn note(&self, slot: Slot<'_>) -> String {
        match self {
            Verdict::Active { adr, .. } => format!("decided by {}", adr),
            Verdict::Undecided => format!("no accepted decision for {}", slot),
            Verdict::Retired { adr, .. } => format!("retired by {}", adr),
            Verdict::Contradiction { heads, .. } => format!(
                "{} heads for {}; the corpus disagrees with itself",
                heads.len(),
                slot
            ),
            Verdict::Unknown { what, name, .. } => match what {
                Unknown::Key => "no such decision key".to_string(),
                Unknown::Scope => "no such scope".to_string(),
                Unknown::ScopeForKey { declared } => format!(
                    "`{}` is not decided per `{}`; the registry applies it to {}",
                    slot.key,
                    name,
                    scope_list(declared)
                ),
            },
        }
    }

    pub fn adr(&self) -> Option<&str> {
        match self {
            Verdict::Active { adr, .. } | Verdict::Retired { adr, .. } => Some(adr),
            _ => None,
        }
    }
}

/// Where an ADR stands, derived and never written: SEMANTICS section 8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerivedStatus {
    Draft,
    Active,
    PartiallySuperseded,
    Superseded,
    Empty,
}

impl DerivedStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            DerivedStatus::Draft => "draft",
            DerivedStatus::Active => "active",
            DerivedStatus::PartiallySuperseded => "partially superseded",
            DerivedStatus::Superseded => "superseded",
            DerivedStatus::Empty => "empty",
        }
    }
}

#[derive(Debug)]
pub struct Graph {
    corpus: Corpus,
}

impl Graph {
    /// Run every cross-document Layer A check and build the graph.
    pub fn build(corpus: Corpus) -> Result<Graph, Vec<Finding>> {
        let mut f = Vec::new();
        check_ids_unique(&corpus, &mut f);
        check_vocabulary(&corpus, &mut f);
        check_edges(&corpus, &mut f);
        check_retirements(&corpus, &mut f);
        check_cycles(&corpus, &mut f);
        if f.is_empty() {
            Ok(Graph { corpus })
        } else {
            f.sort_by_key(|x| (x.file.clone(), x.line));
            Err(f)
        }
    }

    pub fn corpus(&self) -> &Corpus {
        &self.corpus
    }

    pub fn registry(&self) -> &Registry {
        &self.corpus.registry
    }

    /// Every accepted entry at this slot, replaced or not.
    fn accepted_at(&self, slot: Slot<'_>) -> Vec<(&Adr, &Entry)> {
        self.corpus
            .adrs
            .iter()
            .filter(|a| a.status.is_accepted())
            .filter_map(|a| a.entry_at(slot).map(|e| (a, e)))
            .collect()
    }

    /// True when at least one accepted entry occupies the slot. A slot holding
    /// only drafts is not occupied, which is what lets a proposal be opened
    /// without changing the current answer.
    pub fn occupied(&self, slot: Slot<'_>) -> bool {
        !self.accepted_at(slot).is_empty()
    }

    /// The heads of a slot: accepted, and not replaced by an accepted entry.
    ///
    /// One pass over the slot's entries, and set membership rather than a
    /// linear scan per candidate: `heads` is called once per occupied slot by
    /// `keys`, `heads --json` and the projection, so the old shape — two
    /// allocations of the same vector and an O(n·m) `contains` — multiplied
    /// with the size of the corpus rather than the size of the slot.
    pub fn heads(&self, slot: Slot<'_>) -> Vec<(&Adr, &Entry)> {
        let at = self.accepted_at(slot);
        let replaced: BTreeSet<&str> = at
            .iter()
            .flat_map(|(_, e)| e.replaces().iter().map(|s| s.as_str()))
            .collect();
        let mut h: Vec<(&Adr, &Entry)> = at
            .into_iter()
            .filter(|(a, _)| !replaced.contains(a.id.as_str()))
            .collect();
        h.sort_by(|x, y| x.0.id.cmp(&y.0.id));
        h
    }

    /// SEMANTICS section 5.
    pub fn resolve(&self, key: &str, scope: &str) -> Result<Verdict, Inconsistent> {
        if !self.corpus.registry.has_key(key) {
            return Ok(Verdict::Unknown {
                what: Unknown::Key,
                name: key.to_string(),
                suggestion: suggest(
                    key,
                    self.corpus.registry.keys.iter().map(|k| k.name.as_str()),
                )
                .map(str::to_string),
            });
        }
        if !self.corpus.registry.has_scope(scope) {
            return Ok(Verdict::Unknown {
                what: Unknown::Scope,
                name: scope.to_string(),
                suggestion: suggest(
                    scope,
                    self.corpus.registry.scopes.iter().map(|s| s.as_str()),
                )
                .map(str::to_string),
            });
        }
        if !self.corpus.registry.admits(key, scope) {
            let declared = self
                .corpus
                .registry
                .key(key)
                .and_then(|d| d.scopes.clone())
                .unwrap_or_default();
            return Ok(Verdict::Unknown {
                what: Unknown::ScopeForKey { declared },
                name: scope.to_string(),
                suggestion: None,
            });
        }
        self.resolve_at(key, scope, scope)
    }

    fn resolve_at(&self, key: &str, scope: &str, queried: &str) -> Result<Verdict, Inconsistent> {
        let slot = Slot { key, scope };
        let h = self.heads(slot);
        if h.len() > 1 {
            return Ok(Verdict::Contradiction {
                heads: h.iter().map(|(a, _)| a.id.clone()).collect(),
                matched_scope: scope.to_string(),
            });
        }
        if let Some((adr, e)) = h.first() {
            let inherited = scope != queried;
            return Ok(match &e.kind {
                EntryKind::Choice(c) => Verdict::Active {
                    adr: adr.id.clone(),
                    choice: c.clone(),
                    matched_scope: scope.to_string(),
                    inherited,
                    reason: e.reason.clone(),
                },
                EntryKind::Retire => Verdict::Retired {
                    adr: adr.id.clone(),
                    matched_scope: scope.to_string(),
                    inherited,
                    reason: e.reason.clone(),
                },
            });
        }
        if self.occupied(slot) {
            // Only a replacement cycle can produce this, and Layer A rejects
            // those. Never degrade it into a verdict.
            return Err(Inconsistent {
                message: format!("{} is occupied but has no head", slot),
            });
        }
        if scope != DEFAULT_SCOPE {
            return self.resolve_at(key, DEFAULT_SCOPE, queried);
        }
        Ok(Verdict::Undecided)
    }

    /// The chain for a slot, oldest first. History, explicitly not authority.
    pub fn history(&self, slot: Slot<'_>) -> Vec<&Adr> {
        // Walk back from each head through `replaces`, then present in
        // discovery order reversed so the oldest reads first.
        let mut order: Vec<&Adr> = Vec::new();
        let mut stack: Vec<&Adr> = self.heads(slot).into_iter().map(|(a, _)| a).collect();
        while let Some(a) = stack.pop() {
            if order.iter().any(|x| x.id == a.id) {
                continue;
            }
            order.push(a);
            if let Some(e) = a.entry_at(slot) {
                for p in e.replaces() {
                    if let Some(pa) = self.corpus.adr(p) {
                        stack.push(pa);
                    }
                }
            }
        }
        order.reverse();
        order
    }

    /// SEMANTICS section 8. Per-entry head status drives the document status,
    /// so partial supersession is visible rather than collapsed.
    pub fn derived_status(&self, adr: &Adr) -> DerivedStatus {
        if !adr.status.is_accepted() {
            return DerivedStatus::Draft;
        }
        if adr.decisions.is_empty() {
            return DerivedStatus::Empty;
        }
        let flags: Vec<bool> = adr
            .decisions
            .iter()
            .map(|e| self.heads(e.slot()).iter().any(|(a, _)| a.id == adr.id))
            .collect();
        match (flags.iter().all(|b| *b), flags.iter().any(|b| *b)) {
            (true, _) => DerivedStatus::Active,
            (false, true) => DerivedStatus::PartiallySuperseded,
            (false, false) => DerivedStatus::Superseded,
        }
    }

    /// Which of an ADR's entries are still heads, and which are not.
    pub fn entry_status<'s>(&'s self, adr: &'s Adr) -> Vec<(&'s Entry, bool)> {
        adr.decisions
            .iter()
            .map(|e| (e, self.heads(e.slot()).iter().any(|(a, _)| a.id == adr.id)))
            .collect()
    }

    /// Layer B. Every slot carrying more than one head.
    pub fn single_head_findings(&self) -> Vec<Finding> {
        let mut out = Vec::new();
        for slot in self.corpus.slots() {
            let h = self.heads(slot);
            if h.len() > 1 {
                let who: Vec<String> = h.iter().map(|(a, _)| a.id.clone()).collect();
                out.push(Finding::new(
                    Layer::B,
                    "single-head",
                    format!("{} has {} heads: {}", slot, h.len(), who.join(", ")),
                ));
            }
        }
        out
    }
}

fn a(check: &'static str, msg: impl Into<String>) -> Finding {
    Finding::new(Layer::A, check, msg)
}

fn check_ids_unique(c: &Corpus, out: &mut Vec<Finding>) {
    for (i, adr) in c.adrs.iter().enumerate() {
        if let Some(prev) = c.adrs[..i].iter().find(|p| p.id == adr.id) {
            out.push(
                a(
                    "id-unique",
                    format!("`{}` is already claimed by {}", adr.id, prev.file),
                )
                .in_file(adr.file.clone()),
            );
        }
    }
}

fn check_vocabulary(c: &Corpus, out: &mut Vec<Finding>) {
    for adr in &c.adrs {
        for e in &adr.decisions {
            if !c.registry.has_key(&e.key) {
                let hint = suggest(&e.key, c.registry.keys.iter().map(|k| k.name.as_str()))
                    .map(|s| format!("; did you mean `{}`", s))
                    .unwrap_or_default();
                out.push(
                    a(
                        "key-registered",
                        format!("`{}` is not in the registry{}", e.key, hint),
                    )
                    .at(adr.file.clone(), e.line),
                );
            }
            if !c.registry.has_scope(&e.scope) {
                let hint = suggest(&e.scope, c.registry.scopes.iter().map(|s| s.as_str()))
                    .map(|s| format!("; did you mean `{}`", s))
                    .unwrap_or_default();
                out.push(
                    a(
                        "scope-declared",
                        format!("scope `{}` is not in the registry{}", e.scope, hint),
                    )
                    .at(adr.file.clone(), e.line),
                );
            } else if c.registry.has_key(&e.key) && !c.registry.admits(&e.key, &e.scope) {
                let declared = c.registry.key(&e.key).and_then(|d| d.scopes.clone());
                out.push(
                    a(
                        "scope-applies",
                        format!(
                            "`{}` is decided at scope `{}`, but the registry applies \
                             that key to {}",
                            e.key,
                            e.scope,
                            scope_list(&declared.unwrap_or_default())
                        ),
                    )
                    .at(adr.file.clone(), e.line),
                );
            }
        }
    }
}

fn check_edges(c: &Corpus, out: &mut Vec<Finding>) {
    for adr in &c.adrs {
        for e in &adr.decisions {
            for target in e.replaces() {
                match c.adr(target) {
                    None => out.push(
                        a(
                            "edge-resolves",
                            format!("`replaces: {}` names no ADR in this corpus", target),
                        )
                        .at(adr.file.clone(), e.line),
                    ),
                    Some(t) if t.entry_at(e.slot()).is_none() => out.push(
                        a(
                            "edge-resolves",
                            format!(
                                "{} has no entry for {}, so there is nothing to replace",
                                target,
                                e.slot()
                            ),
                        )
                        .at(adr.file.clone(), e.line),
                    ),
                    Some(t) => {
                        // SEMANTICS section 7: acceptance order must not be
                        // able to change resolution.
                        if adr.status.is_accepted() && !t.status.is_accepted() {
                            out.push(
                                a(
                                    "no-accepted-replaces-draft",
                                    format!("{} is a draft; there is nothing to replace", target),
                                )
                                .at(adr.file.clone(), e.line),
                            );
                        }
                    }
                }
            }
            if let Some(target) = &e.overrides {
                let global = Slot {
                    key: &e.key,
                    scope: DEFAULT_SCOPE,
                };
                match c.adr(target) {
                    None => out.push(
                        a(
                            "edge-resolves",
                            format!("`overrides: {}` names no ADR in this corpus", target),
                        )
                        .at(adr.file.clone(), e.line),
                    ),
                    Some(t) if t.entry_at(global).is_none() => out.push(
                        a(
                            "edge-resolves",
                            format!("{} has no entry for {}", target, global),
                        )
                        .at(adr.file.clone(), e.line),
                    ),
                    Some(_) => {}
                }
            }
        }
    }
}

/// SEMANTICS section 6.1: a retirement must name what it retires, and the two
/// branches are not interchangeable.
fn check_retirements(c: &Corpus, out: &mut Vec<Finding>) {
    for adr in &c.adrs {
        for e in adr.decisions.iter().filter(|e| e.is_retire()) {
            if !e.is_first() {
                continue; // the `replaces` branch; `check_edges` covers it
            }
            let global = Slot {
                key: &e.key,
                scope: DEFAULT_SCOPE,
            };
            let global_occupied = c
                .adrs
                .iter()
                .filter(|x| x.status.is_accepted())
                .any(|x| x.entry_at(global).is_some());
            match (&e.overrides, global_occupied) {
                (Some(_), true) => {}
                (Some(_), false) => out.push(
                    a(
                        "retire-names-predecessor",
                        format!(
                            "nothing is decided at {}, so this scope inherits nothing \
                             to retire; the answer is already undecided",
                            global
                        ),
                    )
                    .at(adr.file.clone(), e.line),
                ),
                (None, _) => out.push(
                    a(
                        "retire-names-predecessor",
                        "a retirement with `first: true` must name the inherited \
                         decision it opts out of, with `overrides:`"
                            .to_string(),
                    )
                    .at(adr.file.clone(), e.line),
                ),
            }
        }
    }
}

/// Replacement cycles, per slot. `replaces` only ever acts inside one slot, so
/// a cycle cannot span slots and each is searched independently.
fn check_cycles(c: &Corpus, out: &mut Vec<Finding>) {
    for slot in c.slots() {
        let mut stack: Vec<&str> = Vec::new();
        let mut done: Vec<&str> = Vec::new();
        for adr in &c.adrs {
            if adr.entry_at(slot).is_some() {
                visit(c, slot, &adr.id, &mut stack, &mut done, out);
            }
        }
    }
}

fn visit<'c>(
    c: &'c Corpus,
    slot: Slot<'c>,
    id: &'c str,
    stack: &mut Vec<&'c str>,
    done: &mut Vec<&'c str>,
    out: &mut Vec<Finding>,
) {
    if done.contains(&id) {
        return;
    }
    if stack.contains(&id) {
        let at = stack.iter().position(|x| *x == id).unwrap_or(0);
        let mut cycle: Vec<&str> = stack[at..].to_vec();
        cycle.push(id);
        if let Some(adr) = c.adr(id) {
            out.push(
                a(
                    "no-cycle",
                    format!("replacement cycle for {}: {}", slot, cycle.join(" -> ")),
                )
                .in_file(adr.file.clone()),
            );
        }
        done.push(id);
        return;
    }
    stack.push(id);
    if let Some(adr) = c.adr(id) {
        if let Some(e) = adr.entry_at(slot) {
            for p in e.replaces() {
                if c.adr(p).is_some() {
                    let pid: &'c str = &c.adr(p).unwrap().id;
                    visit(c, slot, pid, stack, done, out);
                }
            }
        }
    }
    stack.pop();
    done.push(id);
}
