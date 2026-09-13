//! Output. Human text on stdout, or `--json` and nothing else.
//!
//! Every verdict prints its machine token first, so a human reading a terminal
//! and a script reading stdout are looking at the same word.

use crate::load::{self, Corpora, Loaded, Repo};
use crate::provenance::{self, Provenance};
use aval_core::graph::{DerivedStatus, Graph, Inconsistent, Unknown, Verdict};
use aval_core::json::Json;
use aval_core::model::{Adr, AdrId, Finding, Slot};
use aval_core::pack::Pack;
use std::path::Path;

/// A resolved answer, with everything either rendering needs.
///
/// Resolution is not the whole job: a contradiction is enriched with the commit
/// that introduced each competing head, and an answer that came from a pack
/// carries the pack's name. **Both renderings need both**, so a helper that
/// returned only the JSON would leave its second caller to rebuild the
/// enrichment — which is how the copy this crate's `lib.rs` records came to
/// drift. One value, two renderings, one producer.
#[derive(Debug)]
pub struct Answer<'a> {
    pub verdict: Verdict,
    pub slot: Slot<'a>,
    pub prov: Vec<(AdrId, Provenance)>,
    pub pack: Option<String>,
}

/// Resolve, and enrich the verdict the way both surfaces need it.
///
/// `Err` is not a verdict and never becomes one: an inconsistent graph means no
/// question was answered, and every surface must say so in whatever way it says
/// "the tool could not answer".
pub fn answer<'a>(
    g: &Graph,
    root: &Path,
    key: &'a str,
    scope: &'a str,
) -> Result<Answer<'a>, Inconsistent> {
    let verdict = g.resolve(key, scope)?;
    let slot = Slot { key, scope };

    // Competing heads are the one verdict where "which commit did this" is
    // load-bearing, because parallel worktrees make them routine. Enrichment
    // only; a repository with no history still gets exit 5.
    let prov = match &verdict {
        Verdict::Contradiction {
            heads,
            matched_scope,
        } => heads
            .iter()
            .map(|id| {
                let at = Slot {
                    key,
                    scope: matched_scope,
                };
                let found = g
                    .heads(at)
                    .into_iter()
                    .find(|(a, _)| &a.id == id)
                    .map(|(a, e)| (a.file.clone(), e.line));
                match found {
                    Some((file, line)) => (
                        id.clone(),
                        provenance::for_line(root, &root.join(file), line),
                    ),
                    None => (id.clone(), Provenance::Unavailable),
                }
            })
            .collect(),
        _ => Vec::new(),
    };

    // Which pack the answer came from, when it came from one. A consumer has
    // to be able to tell a decision it can change from one it cannot, and
    // reading the id prefix works only for somebody who already knows the
    // convention.
    let pack = verdict
        .adr()
        .and_then(|id| g.corpus().adr(id))
        .and_then(|a| a.pack.clone());

    Ok(Answer {
        verdict,
        slot,
        prov,
        pack,
    })
}

impl Answer<'_> {
    pub fn exit(&self) -> i32 {
        self.verdict.exit()
    }

    pub fn json(&self) -> Json {
        verdict_json(&self.verdict, self.slot, &self.prov).set_opt("pack", self.pack.clone())
    }

    pub fn text(&self) -> String {
        let mut s = verdict_text(&self.verdict, self.slot, &self.prov);
        if let Some(p) = &self.pack {
            s.push_str(&format!(
                "  vendored: from the `{}` pack; change it there, not here\n",
                p
            ));
        }
        s
    }
}

/// The `--json` error object: a question that could not be asked at all.
///
/// Shared so the MCP server reports an unreadable corpus in the same shape the
/// CLI does, rather than inventing a second one.
pub fn error_json(exit: i32, message: &str, findings: &[Finding]) -> Json {
    Json::obj()
        .set("ok", false)
        .set("exit", exit)
        .set("error", message)
        .set(
            "findings",
            findings.iter().map(finding_json).collect::<Vec<_>>(),
        )
}

pub fn finding_json(f: &Finding) -> Json {
    Json::obj()
        .set("layer", f.layer.as_str())
        .set("check", f.check)
        .set("message", f.message.as_str())
        .set_opt("file", f.file.clone())
        .set_opt("line", f.line)
}

pub fn verdict_json(v: &Verdict, slot: Slot<'_>, prov: &[(AdrId, Provenance)]) -> Json {
    let base = Json::obj()
        .set("state", v.token())
        .set("exit", v.exit())
        .set("key", slot.key)
        .set("scope", slot.scope)
        .set("note", v.note(slot));
    match v {
        Verdict::Active {
            adr,
            choice,
            matched_scope,
            inherited,
            reason,
        } => base
            .set("adr", adr.as_str())
            .set("choice", choice.as_str())
            .set("matched_scope", matched_scope.as_str())
            .set("inherited", *inherited)
            .set_opt("reason", reason.clone()),
        Verdict::Retired {
            adr,
            matched_scope,
            inherited,
            reason,
        } => base
            .set("adr", adr.as_str())
            .set("matched_scope", matched_scope.as_str())
            .set("inherited", *inherited)
            .set_opt("reason", reason.clone()),
        Verdict::Contradiction {
            heads,
            matched_scope,
        } => base
            .set(
                "heads",
                heads
                    .iter()
                    .map(|h| {
                        let p = prov.iter().find(|(id, _)| id == h);
                        Json::obj().set("adr", h.as_str()).set(
                            "provenance",
                            Json::obj()
                                .set("state", p.map(|(_, x)| x.token()).unwrap_or("unavailable"))
                                .set_opt(
                                    "commit",
                                    p.and_then(|(_, x)| match x {
                                        Provenance::Committed(s) => Some(s.clone()),
                                        _ => None,
                                    }),
                                ),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
            .set("matched_scope", matched_scope.as_str()),
        Verdict::Unknown {
            what, suggestion, ..
        } => {
            let b = base
                .set("unknown", what.as_str())
                .set_opt("suggestion", suggestion.clone());
            match what {
                // The caller asked on the wrong axis; give it the right one
                // rather than only the word "unknown".
                Unknown::ScopeForKey { declared } => b.set(
                    "applies_to",
                    declared.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                ),
                _ => b,
            }
        }
        Verdict::Undecided => base,
    }
}

pub fn verdict_text(v: &Verdict, slot: Slot<'_>, prov: &[(AdrId, Provenance)]) -> String {
    let mut s = String::new();
    match v {
        Verdict::Active {
            adr,
            choice,
            matched_scope,
            inherited,
            ..
        } => {
            s.push_str(&format!("active   {}   {}\n", adr, choice));
            if *inherited {
                s.push_str(&format!(
                    "  inherited: {} decides the default scope; nothing decides {}\n",
                    adr, slot
                ));
            } else if matched_scope != "*" {
                s.push_str(&format!("  scope: {}\n", matched_scope));
            }
        }
        Verdict::Retired {
            adr,
            inherited,
            reason,
            ..
        } => {
            s.push_str(&format!("retired   {}\n", adr));
            if let Some(r) = reason {
                s.push_str(&format!("  {}\n", r));
            }
            if *inherited {
                s.push_str("  inherited from the default scope\n");
            }
        }
        Verdict::Undecided => {
            s.push_str(&format!("undecided   {}\n", v.note(slot)));
        }
        Verdict::Contradiction { heads, .. } => {
            s.push_str(&format!("contradiction   {}\n", v.note(slot)));
            for h in heads {
                match prov.iter().find(|(id, _)| id == h) {
                    Some((_, Provenance::Committed(sha))) => {
                        s.push_str(&format!("  {}  introduced in {}\n", h, sha))
                    }
                    Some((_, Provenance::Uncommitted)) => {
                        s.push_str(&format!("  {}  not yet committed\n", h))
                    }
                    _ => s.push_str(&format!("  {}\n", h)),
                }
            }
            s.push_str("  Stop. Do not pick one; write the ADR that replaces both.\n");
        }
        Verdict::Unknown {
            what,
            name,
            suggestion,
        } => {
            // A key that does not apply at a declared scope is a different
            // failure from a name nobody has heard of, and saying "no such
            // scope" about a scope the registry declares would be false.
            if matches!(what, Unknown::ScopeForKey { .. }) {
                s.push_str(&format!("unknown   {}\n", v.note(slot)));
                s.push_str(
                    "  The question names the wrong axis. Nothing is wrong with the corpus.\n",
                );
                return s;
            }
            s.push_str(&format!("unknown   no such {} `{}`\n", what.as_str(), name));
            if let Some(g) = suggestion {
                s.push_str(&format!(
                    "  did you mean `{}`? A suggestion is advisory and resolves nothing.\n",
                    g
                ));
            }
        }
    }
    s
}

fn status_word(d: DerivedStatus) -> &'static str {
    d.as_str()
}

// ------------------------------------------------------------------ keys
//
// The decision vocabulary. This is DISCOVERY, not authority: §12.1 rules out
// finding a decision by similarity, so the way to ask about a key is to know
// its exact name, and until now nothing could list them.

/// What occupies a slot, for one key.
///
/// Deliberately **direct occupancy only** — inheritance is not folded in.
/// "Decided at this scope" and "answers at this scope" are different questions,
/// and merging them rebuilds the fallback ambiguity §5 exists to prevent. The
/// caller that wants the effective answer resolves.
/// One occupied slot: its verdict word, the records behind it, and the choice
/// when a single accepted record answers.
///
/// `None` for an unoccupied slot. The `adrs` list holds several only for a
/// contradiction, but is a list always — a caller must never have to split a
/// joined string to find the competing records.
fn slot_row(g: &Graph, slot: Slot<'_>) -> Option<(&'static str, Vec<AdrId>, Option<String>)> {
    let heads = g.heads(slot);
    let state = match heads.len() {
        0 => return None,
        1 if heads[0].1.is_retire() => "retired",
        1 => "active",
        _ => "contradiction",
    };
    let adrs = heads.iter().map(|(a, _)| a.id.clone()).collect();
    let choice = match heads.len() {
        1 => heads[0].1.choice().map(|c| c.to_string()),
        _ => None,
    };
    Some((state, adrs, choice))
}

fn decided_at<'a>(g: &'a Graph, key: &str) -> Vec<(&'a str, &'static str, Vec<AdrId>)> {
    g.corpus()
        .slots()
        .into_iter()
        .filter(|s| s.key == key)
        .filter_map(|slot| slot_row(g, slot).map(|(state, adrs, _)| (slot.scope, state, adrs)))
        .collect()
}

/// Every occupied slot, **including the contradicted ones**.
///
/// Deliberately a superset of `HEADS.md`. `project::head_slots` keeps only
/// slots with exactly one head, so the projection drops a contradicted slot
/// entirely — right for a document a person reads beside the corpus, wrong for
/// a caller that would otherwise see a corpus looking settled exactly where it
/// disagrees with itself.
pub fn heads_json(g: &Graph) -> Json {
    let rows: Vec<Json> = g
        .corpus()
        .slots()
        .into_iter()
        .filter_map(|slot| {
            let (state, adrs, choice) = slot_row(g, slot)?;
            Some(
                Json::obj()
                    .set("key", slot.key)
                    .set("scope", slot.scope)
                    .set("state", state)
                    .set("adrs", adrs.iter().map(AdrId::as_str).collect::<Vec<_>>())
                    .set_opt("choice", choice),
            )
        })
        .collect();
    Json::obj().set("heads", rows)
}

/// Which pack declares `key`, if one does.
///
/// `KeyDef` carries no origin — `merge_declarations` folds pack keys into the
/// registry — but the parsed packs are kept, so the answer is right here rather
/// than needing a field in the model that two places would have to agree on.
fn key_pack<'a>(packs: &'a [Pack], key: &str) -> Option<&'a str> {
    packs
        .iter()
        .find(|p| p.keys.iter().any(|k| k.name == key))
        .map(|p| p.name.as_str())
}

/// How much of the vocabulary to report.
///
/// Discovery is the call a caller makes BECAUSE it does not know a key name,
/// and on the fleet's largest corpus the full answer is 10.5 KB of which the
/// names are 1 KB. `Names` is what that caller actually needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    /// Key, description, and the scopes it is answerable at.
    Names,
    /// Everything, including where each key is already decided.
    Full,
}

pub fn keys_json(g: &Graph, packs: &[Pack], detail: Detail) -> Json {
    let reg = g.registry();
    let keys: Vec<Json> = reg
        .keys
        .iter()
        .map(|k| {
            let row = Json::obj()
                .set("key", k.name.as_str())
                .set_opt("description", k.description.clone())
                // `null` and `[]` are different and both are meaning-bearing:
                // null admits every declared scope, [] admits only the default
                // one. Neither may collapse into the other, or into absence.
                .set(
                    "scopes",
                    match &k.scopes {
                        None => Json::Null,
                        Some(list) => {
                            Json::Arr(list.iter().map(|s| Json::from(s.as_str())).collect())
                        }
                    },
                )
                .set_opt("pack", key_pack(packs, &k.name).map(|s| s.to_string()));
            match detail {
                Detail::Names => row,
                Detail::Full => {
                    let decided: Vec<Json> = decided_at(g, &k.name)
                        .into_iter()
                        .map(|(scope, state, adrs)| {
                            Json::obj()
                                .set("scope", scope)
                                .set("state", state)
                                .set("adrs", adrs.iter().map(AdrId::as_str).collect::<Vec<_>>())
                        })
                        .collect();
                    row.set("decided", decided)
                }
            }
        })
        .collect();
    Json::obj()
        .set(
            "scopes",
            reg.scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        )
        .set("keys", keys)
}

pub fn keys_text(g: &Graph, packs: &[Pack]) -> String {
    let reg = g.registry();
    let mut s = String::new();
    s.push_str(&format!(
        "{} keys · scopes: {}\n",
        reg.keys.len(),
        if reg.scopes.is_empty() {
            "none declared".to_string()
        } else {
            reg.scopes.join(", ")
        }
    ));
    for k in &reg.keys {
        s.push('\n');
        match &k.description {
            Some(d) => s.push_str(&format!("{}   {}\n", k.name, d)),
            None => s.push_str(&format!("{}\n", k.name)),
        }
        let where_ = match &k.scopes {
            None => "every declared scope".to_string(),
            Some(l) if l.is_empty() => "the default scope only".to_string(),
            Some(l) => l.join(", "),
        };
        match key_pack(packs, &k.name) {
            Some(p) => s.push_str(&format!(
                "  answerable at {}   ·   declared by the `{}` pack\n",
                where_, p
            )),
            None => s.push_str(&format!("  answerable at {}\n", where_)),
        }
        for (scope, state, adrs) in decided_at(g, &k.name) {
            s.push_str(&format!(
                "  {:<14} {}   {}\n",
                state,
                scope,
                adrs.join(", ")
            ));
        }
    }
    s
}

/// The exit-7 object for an ADR id the corpus does not carry.
///
/// Byte-for-byte what `cmd_show` built inline. It lives here so the MCP server
/// answers an unknown record the same way rather than inventing a second shape.
pub fn show_unknown_json(id: &str, suggestion: Option<String>) -> Json {
    Json::obj()
        .set("state", "unknown")
        .set("exit", 7)
        .set("adr", id)
        .set_opt("suggestion", suggestion)
}

/// The exit-7 object for a key or scope `history` cannot ask about.
///
/// `history` had **no** JSON for this at all: both rejections printed to stderr
/// and returned 7, so `history --json` wrote nothing to stdout and left a
/// machine caller with an exit code and silence, against section 12. The field
/// names follow `resolve`'s unknown verdict, because it is the same question.
pub fn history_unknown_json(
    what: &str,
    key: &str,
    scope: &str,
    suggestion: Option<String>,
) -> Json {
    Json::obj()
        .set("state", "unknown")
        .set("exit", 7)
        .set("key", key)
        .set("scope", scope)
        .set("unknown", what)
        .set_opt("suggestion", suggestion)
}

pub fn show_json(g: &Graph, adr: &Adr) -> Json {
    let entries: Vec<Json> = g
        .entry_status(adr)
        .into_iter()
        .map(|(e, is_head)| {
            Json::obj()
                .set("key", e.key.as_str())
                .set("scope", e.scope.as_str())
                .set("head", is_head)
                .set_opt("choice", e.choice().map(|c| c.to_string()))
                .set("retire", e.is_retire())
        })
        .collect();
    Json::obj()
        .set("adr", adr.id.as_str())
        .set("file", adr.file.as_str())
        .set(
            "status",
            if adr.status.is_accepted() {
                "accepted"
            } else {
                "draft"
            },
        )
        .set("derived_status", status_word(g.derived_status(adr)))
        .set("decisions", entries)
}

pub fn show_text(g: &Graph, adr: &Adr) -> String {
    let mut s = format!(
        "{}   {}\n  file: {}\n",
        adr.id,
        status_word(g.derived_status(adr)),
        adr.file
    );
    if adr.decisions.is_empty() {
        s.push_str("  no decisions\n");
        return s;
    }
    s.push('\n');
    for (e, is_head) in g.entry_status(adr) {
        let mark = if is_head { "head" } else { "superseded" };
        let what = e.choice().unwrap_or("retired");
        s.push_str(&format!("  {:<12} {}   {}\n", mark, e.slot(), what));
    }
    if matches!(g.derived_status(adr), DerivedStatus::PartiallySuperseded) {
        s.push_str(
            "\n  Partially superseded. The decisions marked head are still authoritative.\n",
        );
    }
    s
}

pub fn history_json(g: &Graph, slot: Slot<'_>, chain: &[&Adr]) -> Json {
    let items: Vec<Json> = chain
        .iter()
        .map(|a| {
            let e = a.entry_at(slot);
            Json::obj()
                .set("adr", a.id.as_str())
                .set("head", g.heads(slot).iter().any(|(h, _)| h.id == a.id))
                .set_opt("choice", e.and_then(|x| x.choice()).map(|c| c.to_string()))
                .set("retire", e.map(|x| x.is_retire()).unwrap_or(false))
        })
        .collect();
    Json::obj()
        .set("key", slot.key)
        .set("scope", slot.scope)
        .set("history", items)
}

pub fn history_text(g: &Graph, slot: Slot<'_>, chain: &[&Adr]) -> String {
    let mut s = format!("history of {}\n", slot);
    if chain.is_empty() {
        s.push_str("  nothing has decided this slot\n");
        return s;
    }
    for a in chain {
        let is_head = g.heads(slot).iter().any(|(h, _)| h.id == a.id);
        let e = a.entry_at(slot);
        let what = e.and_then(|x| x.choice()).unwrap_or("retired");
        s.push_str(&format!(
            "  {:<10} {}   {}\n",
            if is_head { "head" } else { "history" },
            a.id,
            what
        ));
    }
    s.push_str("\n  This is history. Only the head is authority.\n");
    s
}

// ------------------------------------------------------------ per corpus
//
// One corpus, one command, every rendering at once. The CLI's single-corpus
// paths keep their own code — their text goes to stderr on a miss and that is
// a contract — but `--all-repos` and the MCP server both need the JSON, the
// text and the exit of one corpus's answer without loading it twice, and this
// is the one producer of that.

/// Everything one corpus says in answer to one command.
#[derive(Debug, Clone)]
pub struct Reply {
    pub json: Json,
    pub text: String,
    pub exit: i32,
    /// Whether the tool surface reports this as a failure: a corpus that could
    /// not answer at all, or a name it does not carry. Never a verdict.
    pub is_error: bool,
}

pub fn resolve_in(l: &Loaded, key: &str, scope: &str) -> Reply {
    match answer(&l.graph, &l.root, key, scope) {
        Ok(a) => Reply {
            json: a.json(),
            text: a.text(),
            exit: a.exit(),
            is_error: false,
        },
        Err(e) => Reply {
            json: error_json(3, &e.to_string(), &[]),
            text: format!("aval: {}\n", e),
            exit: 3,
            is_error: true,
        },
    }
}

pub fn keys_in(l: &Loaded, detail: Detail) -> Reply {
    Reply {
        json: keys_json(&l.graph, &l.packs, detail),
        text: keys_text(&l.graph, &l.packs),
        exit: 0,
        is_error: false,
    }
}

pub fn heads_in(l: &Loaded) -> Reply {
    Reply {
        json: heads_json(&l.graph),
        text: aval_core::project::render(&l.graph),
        exit: 0,
        is_error: false,
    }
}

pub fn show_in(l: &Loaded, id: &str) -> Reply {
    match l.graph.corpus().adr(id) {
        Some(adr) => Reply {
            json: show_json(&l.graph, adr),
            text: show_text(&l.graph, adr),
            exit: 0,
            is_error: false,
        },
        None => {
            let names: Vec<&str> = l
                .graph
                .corpus()
                .adrs
                .iter()
                .map(|a| a.id.as_str())
                .collect();
            let sug = aval_core::model::suggest(id, names).map(str::to_string);
            let mut text = format!("aval: no such ADR `{}`\n", id);
            if let Some(s) = &sug {
                text.push_str(&format!(
                    "  did you mean {}? A suggestion is advisory.\n",
                    s
                ));
            }
            Reply {
                json: show_unknown_json(id, sug),
                text,
                exit: 7,
                is_error: true,
            }
        }
    }
}

pub fn history_in(l: &Loaded, key: &str, scope: &str) -> Reply {
    let reg = l.graph.registry();
    let unknown = |what: &str, names: Vec<&str>, needle: &str, line: String| {
        let sug = aval_core::model::suggest(needle, names).map(str::to_string);
        let mut text = line;
        if let Some(g) = &sug {
            text.push_str(&format!(
                "  did you mean `{}`? A suggestion is advisory.\n",
                g
            ));
        }
        Reply {
            json: history_unknown_json(what, key, scope, sug),
            text,
            exit: 7,
            is_error: true,
        }
    };
    if !reg.has_key(key) {
        return unknown(
            "key",
            reg.keys.iter().map(|k| k.name.as_str()).collect(),
            key,
            format!("aval: `{}` is not a registered decision key\n", key),
        );
    }
    if !reg.has_scope(scope) {
        return unknown(
            "scope",
            reg.scopes.iter().map(|s| s.as_str()).collect(),
            scope,
            format!("aval: `{}` is not a declared scope\n", scope),
        );
    }
    let slot = Slot { key, scope };
    let chain = l.graph.history(slot);
    Reply {
        json: history_json(&l.graph, slot, &chain),
        text: history_text(&l.graph, slot, &chain),
        exit: 0,
        is_error: false,
    }
}

// --------------------------------------------------------------- aggregate
//
// Several corpora, one command. An aggregate is a REPORT, not a verdict: its
// exit code borrows `check`'s contract rather than `resolve`'s, and its JSON is
// a map of each member's own payload, byte-equal to what that corpus answers
// on its own. The CLI's `--all-repos` and the MCP server's no-`repo` call are
// the same `Aggregate`; that is what keeps them from drifting.

#[derive(Debug)]
pub enum Outcome {
    Answered(Reply),
    /// The corpus would not load. Its error object stands where its answer
    /// would; the report does not fail because one member did.
    Unloadable {
        json: Json,
        message: String,
    },
}

#[derive(Debug)]
pub struct Member {
    pub repo: Repo,
    pub outcome: Outcome,
}

#[derive(Debug)]
pub struct Aggregate {
    pub members: Vec<Member>,
    /// Worktrees left out because their parent is itself a member: the same
    /// corpus would otherwise answer twice. Named, so the omission is visible.
    pub excluded: Vec<(String, String)>,
}

/// Load each repository once and ask it the same question.
///
/// A linked worktree whose parent is among `repos` is excluded — it is a branch
/// of a corpus already answering. One whose parent is not is the only
/// representative of that repository and stays in.
pub fn across(repos: &[Repo], f: impl Fn(&Loaded, &Repo) -> Reply) -> Aggregate {
    let mut members = Vec::new();
    let mut excluded = Vec::new();
    for r in repos {
        if let Some(parent) = r.worktree.as_ref().and_then(|w| w.parent.clone()) {
            excluded.push((r.name.clone(), parent));
            continue;
        }
        let outcome = match load::load(&r.root) {
            Ok(l) => Outcome::Answered(f(&l, r)),
            Err(e) => Outcome::Unloadable {
                json: error_json(3, &e.to_string(), e.findings()),
                message: e.to_string(),
            },
        };
        members.push(Member {
            repo: r.clone(),
            outcome,
        });
    }
    Aggregate { members, excluded }
}

impl Aggregate {
    pub fn json(&self) -> Json {
        let mut repos = Json::obj();
        for m in &self.members {
            let j = match &m.outcome {
                Outcome::Answered(r) => r.json.clone(),
                Outcome::Unloadable { json, .. } => json.clone(),
            };
            repos = repos.set(&m.repo.name, j);
        }
        let mut ex = Json::obj();
        for (name, parent) in &self.excluded {
            ex = ex.set(name, parent.as_str());
        }
        Json::obj()
            .set("repos", repos)
            .set("worktrees_excluded", ex)
    }

    pub fn text(&self) -> String {
        let mut s = String::new();
        for (i, m) in self.members.iter().enumerate() {
            if i > 0 {
                s.push('\n');
            }
            s.push_str(&format!("== {} ==\n", m.repo.name));
            match &m.outcome {
                Outcome::Answered(r) => s.push_str(&r.text),
                Outcome::Unloadable { message, .. } => s.push_str(&format!("aval: {}\n", message)),
            }
        }
        for (name, parent) in &self.excluded {
            s.push_str(&format!("(excluded {}: a worktree of {})\n", name, parent));
        }
        s
    }

    /// `0` every member loaded and none exited 5 · `1` some member exited 5 or
    /// did not load · `3` no member loaded. A report has no verdict of its own;
    /// `contradiction` is the one member state a fleet-wide caller must not
    /// miss, and a member that could not be read is a finding, not silence.
    pub fn exit(&self) -> i32 {
        let loaded = self
            .members
            .iter()
            .filter(|m| matches!(m.outcome, Outcome::Answered(_)))
            .count();
        if loaded == 0 {
            return 3;
        }
        let finding = self.members.iter().any(|m| match &m.outcome {
            Outcome::Answered(r) => r.exit == 5,
            Outcome::Unloadable { .. } => true,
        });
        if finding {
            1
        } else {
            0
        }
    }
}

// ------------------------------------------------------------------- repos

fn repo_json(r: &Repo) -> Json {
    let worktree = match &r.worktree {
        None => Json::Null,
        Some(w) => Json::obj()
            .set("parent_root", w.parent_root.display().to_string())
            .set(
                "parent",
                match &w.parent {
                    Some(p) => Json::from(p.as_str()),
                    None => Json::Null,
                },
            ),
    };
    Json::obj()
        .set("name", r.name.as_str())
        .set("root", r.root.display().to_string())
        .set("worktree", worktree)
        .set("shadowed", r.shadowed)
}

/// What discovery saw, and why each directory is or is not answering.
pub fn repos_json(c: &Corpora) -> Json {
    let (mode, skipped, warning) = match c {
        Corpora::One {
            skipped, warning, ..
        } => ("corpus", skipped, warning.clone()),
        Corpora::Many { skipped, .. } => ("workspace", skipped, None),
    };
    let repos: Vec<Json> = c.repos().into_iter().map(repo_json).collect();
    let skipped: Vec<Json> = skipped
        .iter()
        .map(|s| {
            Json::obj()
                .set("name", s.name.as_str())
                .set("reason", s.reason.as_str())
        })
        .collect();
    Json::obj()
        .set("mode", mode)
        .set("repos", repos)
        .set("skipped", skipped)
        .set_opt("warning", warning)
}

pub fn repos_text(c: &Corpora) -> String {
    let mut s = String::new();
    let (mode, skipped, warning) = match c {
        Corpora::One {
            skipped, warning, ..
        } => ("corpus", skipped, warning.as_deref()),
        Corpora::Many { skipped, .. } => ("workspace", skipped, None),
    };
    let repos = c.repos();
    s.push_str(&format!(
        "{}: {} repositor{}\n",
        mode,
        repos.len(),
        if repos.len() == 1 { "y" } else { "ies" }
    ));
    for r in repos {
        let mut note = String::new();
        if let Some(w) = &r.worktree {
            note.push_str(&match &w.parent {
                Some(p) => format!("   worktree of {}", p),
                None => format!("   worktree of {}", w.parent_root.display()),
            });
        }
        if r.shadowed {
            note.push_str("   shadowed");
        }
        s.push_str(&format!("  {:<28} {}{}\n", r.name, r.root.display(), note));
    }
    if !skipped.is_empty() {
        s.push_str("skipped:\n");
        for k in skipped {
            s.push_str(&format!("  {}: {}\n", k.name, k.reason));
        }
    }
    if let Some(w) = warning {
        s.push_str(&format!("warning: {}\n", w));
    }
    s
}

// ---------------------------------------------------------- resource names
//
// A repository name is a directory basename, and a valid basename may hold a
// space, `#`, `%` or `?`. Inside a URI those are not text, so a name is
// percent-encoded on the way out and decoded on the way in — and what comes
// back is looked up against the discovered names, never used as a path.

/// RFC 3986 unreserved characters pass; every other byte is `%XX`.
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// `None` for a malformed escape or bytes that are not UTF-8.
pub fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}
