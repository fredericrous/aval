//! Output. Human text on stdout, or `--json` and nothing else.
//!
//! Every verdict prints its machine token first, so a human reading a terminal
//! and a script reading stdout are looking at the same word.

use crate::provenance::Provenance;
use aval_core::graph::{DerivedStatus, Graph, Verdict};
use aval_core::json::Json;
use aval_core::model::{Adr, Finding, Slot};

pub fn finding_json(f: &Finding) -> Json {
    Json::obj()
        .set("layer", f.layer.as_str())
        .set("check", f.check)
        .set("message", f.message.as_str())
        .set_opt("file", f.file.clone())
        .set_opt("line", f.line)
}

pub fn verdict_json(v: &Verdict, slot: Slot<'_>, prov: &[(String, Provenance)]) -> Json {
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
        } => base
            .set("unknown", what.as_str())
            .set_opt("suggestion", suggestion.clone()),
        Verdict::Undecided | Verdict::Internal(_) => base,
    }
}

pub fn verdict_text(v: &Verdict, slot: Slot<'_>, prov: &[(String, Provenance)]) -> String {
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
            s.push_str(&format!("unknown   no such {} `{}`\n", what.as_str(), name));
            if let Some(g) = suggestion {
                s.push_str(&format!(
                    "  did you mean `{}`? A suggestion is advisory and resolves nothing.\n",
                    g
                ));
            }
        }
        Verdict::Internal(m) => s.push_str(&format!("internal   {}\n", m)),
    }
    s
}

fn status_word(d: DerivedStatus) -> &'static str {
    d.as_str()
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
