//! `aval relevant` — which decisions bear on what is about to be touched.
//!
//! The resolver is exact: a caller has to know a key's name before it can ask
//! anything (SEMANTICS section 12.1). That is right for an answer and useless
//! as a starting point, and the gap showed: an agent about to edit a file has
//! no way to find out that three of the corpus's keys govern it and a fourth is
//! deliberately undecided. It reads `HEADS.md`, which is every decision the
//! repository has ever made, or it reads nothing.
//!
//! So relevance is **retrieval at the input edge**, and everything downstream of
//! it is unchanged. The ranking is a `suggestion` (SEMANTICS section 5.1): it
//! resolves nothing, writes nothing, and is not part of what the corpus says is
//! true. What it hands back for each key it ranks is that key's real verdict,
//! from `resolve` — so a caller sees both "this governs you" and "this is
//! undecided, so do not invent it".
//!
//! # The signals
//!
//! Four, combined with the weights below, all deterministic and all local:
//!
//! | Signal | Weight | Source |
//! |---|---|---|
//! | text | 1.0 | BM25 over the query's words |
//! | path | 0.6 | BM25 over the paths' own words |
//! | mention | 2.0 each, at most 3 | a record's body names the path, backticked or as a glob |
//! | co-change | 0.5 each, at most 3 | git says the commits that wrote the record also touched the path |
//!
//! A mention outranks every amount of word overlap because it is the one signal
//! the author put there on purpose: a record that backticks
//! `kubernetes/apps/forgejo` is about that directory in a way no vocabulary
//! match can be. Co-change is the weakest, and capped hardest, because it is
//! circumstantial — a decision record and a config file in one commit may share
//! nothing but a Tuesday.

use crate::load::Loaded;
use crate::render::{self, Reply};
use aval_core::json::Json;
use aval_core::model::{Adr, AdrId, Entry, Rule, Slot};
use aval_core::relevance::{self, Doc, Index};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::{Command, Stdio};

/// How much each signal is worth. Public because they are documented in the
/// README and asserted by the battery: a weight is part of what the ranking
/// means, not a number buried in an expression.
pub const W_TEXT: f64 = 1.0;
pub const W_PATH: f64 = 0.6;
pub const W_MENTION: f64 = 2.0;
pub const W_CO_CHANGE: f64 = 0.5;
/// How many matching paths one signal may count. Past this a caller passing a
/// hundred paths would rank every record that ever mentioned a directory.
pub const EVIDENCE_CAP: usize = 3;

/// A row must score at least this fraction of the best row to be reported.
///
/// BM25 answers for every document that shares one word with the query, and on
/// a small corpus that is most of it. `--top` alone would then always return
/// `--top` rows, so the fifth row would be noise dressed as a recommendation
/// exactly when the first is a real match. A relative floor lets a query with
/// one good answer report one.
pub const RELATIVE_FLOOR: f64 = 0.2;

/// A rule's statement, shortened for a listing.
///
/// `aval rules` prints statements whole because listing them IS its job. Here
/// the rules are a footnote to the keys, and a 300-character constraint would
/// bury the ranking above it. The id is exact, so `aval rule <id>` is the rest.
const STATEMENT_WIDTH: usize = 96;

/// Field weights inside a key's document.
///
/// The key's own name is weighted highest because it is the thing being named:
/// a caller searching "object store" wants `storage.object-store` first, and the
/// body of whichever record decided it second.
const W_KEY: f64 = 3.0;
const W_DESCRIPTION: f64 = 2.0;
const W_TITLE: f64 = 3.0;
const W_CHOICE: f64 = 3.0;
const W_REASON: f64 = 2.0;
const W_BODY: f64 = 1.0;
/// A superseded record's title and choice, at a third of a live one's. It is
/// still evidence about what the key is *about* — the argument is what moved —
/// so it is not excluded, and it must not outweigh what currently holds.
const W_SUPERSEDED: f64 = 1.0;

/// Rule document fields.
const W_RULE_ID: f64 = 3.0;
const W_RULE_STATEMENT: f64 = 2.0;
const W_RULE_BODY: f64 = 1.0;

/// How far back the co-change signal looks, and how large a commit it believes.
///
/// A repository-wide rename touches everything and says nothing about any one
/// decision, so a commit past `MAX_COMMIT_FILES` is dropped whole rather than
/// counted weakly.
const MAX_COMMITS: usize = 120;
const MAX_COMMIT_FILES: usize = 400;

/// How many keys and rules are reported when nobody says.
///
/// Five: enough that a second and third decision bearing on the same change are
/// visible, few enough that a caller reads all of them. The tail of a long
/// ranking is where a suggestion starts being mistaken for a survey.
pub const DEFAULT_TOP: usize = 5;

/// The one line every rendering carries, in both directions.
pub const ADVISORY: &str =
    "A ranking is a suggestion: it resolves nothing. Only `aval resolve <key>` answers.";

/// What to rank against.
#[derive(Debug, Clone, Default)]
pub struct Query {
    /// Repository-relative paths the caller is about to touch.
    pub paths: Vec<String>,
    /// A description of the task, in words.
    pub text: Option<String>,
    /// Add what git says is modified, staged or untracked.
    pub changed: bool,
    /// How many keys, and how many rules, to report.
    pub top: usize,
    /// The scope every ranked key is resolved at.
    pub scope: String,
}

impl Query {
    /// Whether anything was asked at all. Ranking with no signal would return
    /// the corpus in an arbitrary order, dressed as a recommendation.
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
            && !self.changed
            && self.text.as_deref().map(str::trim).unwrap_or("").is_empty()
    }
}

/// One ranked key, before it is rendered.
#[derive(Debug)]
struct Ranked {
    key: String,
    score: f64,
    text: f64,
    path: f64,
    mentions: Vec<String>,
    co_changed: Vec<String>,
}

// ------------------------------------------------------------------- paths

/// Repository-relative, `/`-separated, without a leading `./` or trailing `/`.
///
/// Textual, like `load::normalise` and for the same reason: `canonicalize`
/// resolves symlinks and can hand back a path outside the repository, and this
/// path is only ever compared as text.
fn normalise(p: &str) -> String {
    p.replace('\\', "/")
        .trim()
        .trim_start_matches("./")
        .trim_end_matches('/')
        .to_string()
}

/// Is `a` the whole path `b`, or a directory of it?
///
/// Segment-wise, so `crates/av` is not a prefix of `crates/aval`.
fn covers(a: &str, b: &str) -> bool {
    a == b || (b.len() > a.len() && b.starts_with(a) && b.as_bytes()[a.len()] == b'/')
}

/// Two paths are related when either contains the other.
///
/// Both directions: a caller naming a directory means every file under it, and
/// a record naming a file is about the directory the caller named.
fn related(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && (covers(a, b) || covers(b, a))
}

/// The literal directory prefix of a pattern, or `None` when it has no usable
/// one. `kubernetes/apps/*/values.yaml` → `kubernetes/apps`.
fn glob_prefix(pattern: &str) -> Option<String> {
    let cut = pattern.find(['*', '?', '[', '{'])?;
    let head = &pattern[..cut];
    let at = head.rfind('/')?;
    let prefix = &head[..at];
    if prefix.is_empty() {
        None
    } else {
        Some(prefix.to_string())
    }
}

/// Does a path the caller named fall under a path a record's body names?
fn mentions(input: &str, mentioned: &str) -> bool {
    match glob_prefix(mentioned) {
        Some(prefix) => covers(&prefix, input),
        None if mentioned.contains(['*', '?', '[', '{']) => false,
        None => related(input, mentioned),
    }
}

/// Paths a document's body names: backticked tokens and markdown link targets.
///
/// Close to `links::collect` and deliberately not shared with it. That one
/// answers "does this citation still resolve", so it drops a glob — the one
/// thing worth keeping here, because `kubernetes/apps/*/values.yaml` is a
/// record telling you exactly which files it governs. A pinned `path@<rev>` is
/// kept too, for the same reason: it is historical as a citation and perfectly
/// current as a subject.
fn mentioned_paths(src: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut fenced = false;
    let body = match aval_core::yaml::split_frontmatter(src) {
        Some((_, _, body)) => body,
        None => src,
    };
    let mut push = |raw: &str| {
        let t = normalise(
            raw.split('#')
                .next()
                .unwrap_or(raw)
                .split('@')
                .next()
                .unwrap_or(raw),
        );
        // A path is a token with a separator. Without that rule a backticked
        // `accepted` and a sibling link `0007-x.md` both become "paths", and
        // neither is one.
        if t.contains('/') && !t.contains("://") && !t.starts_with('/') && !out.contains(&t) {
            out.push(t);
        }
    };
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let mut rest = line;
        while let Some(open) = rest.find('`') {
            let after = &rest[open + 1..];
            let Some(close) = after.find('`') else { break };
            let token = &after[..close];
            if !token.chars().any(char::is_whitespace) {
                push(token);
            }
            rest = &after[close + 1..];
        }
        let mut rest = line;
        while let Some(at) = rest.find("](") {
            let after = &rest[at + 2..];
            let Some(close) = after.find(')') else { break };
            push(after[..close].split_whitespace().next().unwrap_or(""));
            rest = &after[close + 1..];
        }
    }
    out
}

// --------------------------------------------------------------------- git
//
// Both calls are read-only, both are allowed to fail, and a failure is silence
// rather than a finding: a shallow checkout, a corpus outside a repository and
// a machine with no git are all ordinary, and none of them makes a ranking
// wrong — it makes one signal absent. SEMANTICS section 13 takes the same line
// about provenance.

fn git(root: &Path, args: &[String]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn words(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

/// What the working tree has that `HEAD` does not: modified, staged, untracked.
///
/// Three questions rather than one, because `diff HEAD` misses an untracked
/// file entirely and a repository with no commit at all answers only the last
/// two. Each is independent, so the ones that work still answer.
pub fn changed_paths(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for args in [
        words(&["diff", "--name-only", "HEAD"]),
        words(&["diff", "--name-only", "--cached"]),
        words(&["ls-files", "--others", "--exclude-standard"]),
    ] {
        for line in git(root, &args).unwrap_or_default().lines() {
            let p = normalise(line);
            if !p.is_empty() && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// For each record file, the other files the commits that wrote it touched.
///
/// Two calls, not two per record: the first asks which commits touched the
/// corpus, the second asks what each of those commits changed. A pathspec on
/// the second would filter the answer down to the records again, which is the
/// opposite of the question.
fn co_changed(root: &Path, records: &BTreeSet<String>) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    if records.is_empty() {
        return out;
    }
    let mut args = words(&["log", "--format=%H"]);
    args.push(format!("--max-count={}", MAX_COMMITS));
    args.push("--".to_string());
    args.extend(records.iter().cloned());
    let Some(log) = git(root, &args) else {
        return out;
    };
    let shas: Vec<String> = log.lines().map(str::to_string).collect();
    if shas.is_empty() {
        return out;
    }
    let mut args = words(&["show", "--name-only", "--no-renames", "--format=%x01%H"]);
    args.extend(shas);
    let Some(shown) = git(root, &args) else {
        return out;
    };

    let mut touched: Vec<String> = Vec::new();
    let flush = |files: &mut Vec<String>, out: &mut BTreeMap<String, BTreeSet<String>>| {
        if !files.is_empty() && files.len() <= MAX_COMMIT_FILES {
            let here: Vec<&String> = files.iter().filter(|f| records.contains(*f)).collect();
            let others: Vec<&String> = files.iter().filter(|f| !records.contains(*f)).collect();
            for rec in here {
                let e = out.entry(rec.clone()).or_default();
                for o in &others {
                    e.insert((*o).clone());
                }
            }
        }
        files.clear();
    };
    for line in shown.lines() {
        // `%x01` starts a commit's header line, and a file name never does.
        if line.starts_with('\u{1}') {
            flush(&mut touched, &mut out);
            continue;
        }
        let p = normalise(line);
        if !p.is_empty() {
            touched.push(p);
        }
    }
    flush(&mut touched, &mut out);
    out
}

// --------------------------------------------------------------- documents

/// A record's first `# ` heading, which is the title a person would give it.
fn title(body: &str) -> &str {
    for line in body.lines() {
        if let Some(t) = line.strip_prefix("# ") {
            return t.trim();
        }
    }
    ""
}

/// Every entry in the corpus that decides a key, and whether it still holds.
fn entries_by_key(l: &Loaded) -> BTreeMap<&str, Vec<(&Adr, &Entry, bool)>> {
    let mut out: BTreeMap<&str, Vec<(&Adr, &Entry, bool)>> = BTreeMap::new();
    for adr in &l.graph.corpus().adrs {
        for e in &adr.decisions {
            let head = l.graph.heads(e.slot()).iter().any(|(h, _)| h.id == adr.id);
            out.entry(e.key.as_str()).or_default().push((adr, e, head));
        }
    }
    out
}

/// The documents the ranking is over: one per registered key, in registry
/// order, which is the order the JSON and the text both report ties in.
fn key_docs(
    l: &Loaded,
    bodies: &BTreeMap<&str, (&str, &str)>,
    by_key: &BTreeMap<&str, Vec<(&Adr, &Entry, bool)>>,
) -> Vec<Doc> {
    l.graph
        .registry()
        .keys
        .iter()
        .map(|k| {
            let mut d = Doc::new(k.name.as_str());
            d.add(&k.name, W_KEY);
            if let Some(desc) = &k.description {
                d.add(desc, W_DESCRIPTION);
            }
            for (adr, e, head) in by_key.get(k.name.as_str()).into_iter().flatten() {
                let (t, body) = bodies.get(adr.file.as_str()).copied().unwrap_or(("", ""));
                if *head {
                    d.add(t, W_TITLE);
                    d.add(e.choice().unwrap_or(""), W_CHOICE);
                    d.add(e.reason.as_deref().unwrap_or(""), W_REASON);
                    d.add(body, W_BODY);
                } else {
                    d.add(t, W_SUPERSEDED);
                    d.add(e.choice().unwrap_or(""), W_SUPERSEDED);
                }
            }
            d
        })
        .collect()
}

fn rule_docs(rules: &[&Rule]) -> Vec<Doc> {
    rules
        .iter()
        .map(|r| {
            let mut d = Doc::new(r.id.as_str());
            d.add(&r.id, W_RULE_ID);
            d.add(&r.statement, W_RULE_STATEMENT);
            d.add(&r.body, W_RULE_BODY);
            if let Some(s) = &r.source {
                d.add(s, W_RULE_BODY);
            }
            d
        })
        .collect()
}

// ------------------------------------------------------------------ answer

/// Rank, resolve each ranked key, and render both ways.
///
/// One producer for the CLI and the tool surface, like every other command
/// (SEMANTICS section 14.1): the JSON a client receives is the JSON the shell
/// prints.
pub fn relevant_in(l: &Loaded, q: &Query) -> Reply {
    let reg = l.graph.registry();
    if !reg.has_scope(&q.scope) {
        let sug = aval_core::model::suggest(&q.scope, reg.scopes.iter().map(String::as_str));
        let mut text = format!("aval: `{}` is not a declared scope\n", q.scope);
        if let Some(s) = sug {
            text.push_str(&format!("  did you mean `{}`?\n", s));
        }
        return Reply {
            json: render::error_json(2, &format!("`{}` is not a declared scope", q.scope), &[]),
            text,
            exit: 2,
            is_error: true,
        };
    }

    // ---- the query, as terms
    let mut paths: Vec<String> = Vec::new();
    for p in q.paths.iter().map(|p| normalise(p)) {
        if !p.is_empty() && !paths.contains(&p) {
            paths.push(p);
        }
    }
    if q.changed {
        for p in changed_paths(&l.root) {
            if !paths.contains(&p) {
                paths.push(p);
            }
        }
    }
    let text_terms = relevance::tokenize(q.text.as_deref().unwrap_or(""));
    let mut path_terms: Vec<String> = Vec::new();
    for t in paths.iter().flat_map(|p| relevance::path_tokens(p)) {
        if !path_terms.contains(&t) {
            path_terms.push(t);
        }
    }

    // ---- the documents
    let bodies: BTreeMap<&str, (&str, &str)> = l
        .files
        .iter()
        .map(|(rel, src)| {
            let body = match aval_core::yaml::split_frontmatter(src) {
                Some((_, _, b)) => b,
                None => src.as_str(),
            };
            (rel.as_str(), (title(body), body))
        })
        .collect();
    let by_key = entries_by_key(l);
    let index = Index::build(key_docs(l, &bodies, &by_key));

    // ---- the two path signals, per record, folded onto the keys it decides
    let record_files: BTreeSet<String> = l.files.iter().map(|(rel, _)| rel.clone()).collect();
    let mentioned: BTreeMap<&str, Vec<String>> = if paths.is_empty() {
        BTreeMap::new()
    } else {
        bodies
            .iter()
            .map(|(rel, (_, body))| (*rel, mentioned_paths(body)))
            .collect()
    };
    let co = if paths.is_empty() {
        BTreeMap::new()
    } else {
        co_changed(&l.root, &record_files)
    };

    // ---- score
    let mut ranked: Vec<Ranked> = Vec::new();
    for (i, k) in reg.keys.iter().enumerate() {
        // A key the registry does not apply at the scope asked is not a thin
        // answer, it is the wrong axis (SEMANTICS section 2.1). `resolve` says
        // so with exit 7; a ranking says so by not offering it, because the
        // caller asked what governs it *here*.
        if !reg.admits(&k.name, &q.scope) {
            continue;
        }
        let mut hit_mention: Vec<String> = Vec::new();
        let mut hit_co: Vec<String> = Vec::new();
        // Heads only. A superseded record's body describes the world that
        // decision was made in, and crediting its paths to the current answer
        // would point a caller at the file the replacement moved away from.
        for (adr, _, head) in by_key.get(k.name.as_str()).into_iter().flatten() {
            if !*head {
                continue;
            }
            let empty = Vec::new();
            let says = mentioned.get(adr.file.as_str()).unwrap_or(&empty);
            for p in &paths {
                if !hit_mention.contains(p) && says.iter().any(|m| mentions(p, m)) {
                    hit_mention.push(p.clone());
                }
                if !hit_co.contains(p)
                    && co
                        .get(adr.file.as_str())
                        .is_some_and(|f| f.iter().any(|c| related(p, c)))
                {
                    hit_co.push(p.clone());
                }
            }
        }
        let text = index.score(i, &text_terms);
        let path = index.score(i, &path_terms);
        let score = W_TEXT * text
            + W_PATH * path
            + W_MENTION * hit_mention.len().min(EVIDENCE_CAP) as f64
            + W_CO_CHANGE * hit_co.len().min(EVIDENCE_CAP) as f64;
        if relevance::round4(score) <= 0.0 {
            continue;
        }
        hit_mention.sort();
        hit_co.sort();
        ranked.push(Ranked {
            key: k.name.clone(),
            score: relevance::round4(score),
            text: relevance::round4(text),
            path: relevance::round4(path),
            mentions: hit_mention,
            co_changed: hit_co,
        });
    }
    // Score, then name. A tie broken by anything else — registry order, the
    // order a directory listing came back in — is a ranking that depends on
    // something the caller cannot see.
    ranked.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.key.cmp(&b.key)));
    let total = ranked.len();
    let floor = ranked.first().map_or(0.0, |r| r.score * RELATIVE_FLOOR);
    ranked.retain(|r| r.score >= floor);
    ranked.truncate(q.top);

    // ---- rules, ranked the same way and reported apart
    let active: Vec<&Rule> = l
        .graph
        .corpus()
        .rules
        .iter()
        .filter(|r| l.graph.rule_active(r).is_none())
        .collect();
    let rule_index = Index::build(rule_docs(&active));
    let mut rules: Vec<(&Rule, f64)> = active
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let s = W_TEXT * rule_index.score(i, &text_terms)
                + W_PATH * rule_index.score(i, &path_terms);
            (*r, relevance::round4(s))
        })
        .filter(|(_, s)| *s > 0.0)
        .collect();
    rules.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.id.cmp(&b.0.id)));
    let floor = rules.first().map_or(0.0, |r| r.1 * RELATIVE_FLOOR);
    rules.retain(|r| r.1 >= floor);
    rules.truncate(q.top);

    render(l, q, &paths, &ranked, total, &rules)
}

/// One key's verdict, and where else the corpus answers it.
fn verdict_of<'a>(
    l: &'a Loaded,
    key: &'a str,
    scope: &'a str,
) -> Option<(aval_core::graph::Verdict, Slot<'a>)> {
    let slot = Slot { key, scope };
    // An inconsistent graph is unreachable past Layer A, and is never turned
    // into a verdict (SEMANTICS section 4). Here it means one key is silently
    // absent from a ranking, which is what a suggestion is allowed to be.
    l.graph.resolve(key, scope).ok().map(|v| (v, slot))
}

/// The scopes a key is decided at, other than the one that answered.
///
/// On a corpus that decides most of its keys per scope — which is the shape a
/// fleet corpus has — a bare `undecided` at the default scope is true and
/// useless on its own. This says where to ask instead, from the graph, with no
/// inference: these are occupied slots, not guesses.
fn elsewhere<'a>(
    l: &'a Loaded,
    key: &str,
    answered: &str,
) -> Vec<(&'a str, &'static str, Vec<AdrId>)> {
    render::decided_at(&l.graph, key)
        .into_iter()
        .filter(|(scope, _, _)| *scope != answered)
        .collect()
}

fn render(
    l: &Loaded,
    q: &Query,
    paths: &[String],
    ranked: &[Ranked],
    total: usize,
    rules: &[(&Rule, f64)],
) -> Reply {
    let mut rows: Vec<Json> = Vec::new();
    let mut deps: Vec<Json> = Vec::new();
    let mut text = format!("advisory   {}\n\n", ADVISORY);
    let width = ranked.iter().map(|r| r.key.len()).max().unwrap_or(0);

    for r in ranked {
        let Some((v, slot)) = verdict_of(l, &r.key, &q.scope) else {
            continue;
        };
        let also = elsewhere(l, &r.key, matched_scope(&v).unwrap_or(&q.scope));
        let unresolved = matches!(
            v,
            aval_core::graph::Verdict::Undecided | aval_core::graph::Verdict::Contradiction { .. }
        );

        let mut why = Json::obj()
            .set("text", r.text)
            .set("path", r.path)
            .set("mentions", r.mentions.clone())
            .set("co_changed", r.co_changed.clone());
        if !also.is_empty() {
            why = why.set(
                "decided_elsewhere",
                also.iter()
                    .map(|(scope, state, adrs)| {
                        Json::obj()
                            .set("scope", *scope)
                            .set("state", *state)
                            .set("adrs", adrs.iter().map(AdrId::as_str).collect::<Vec<_>>())
                    })
                    .collect::<Vec<_>>(),
            );
        }
        rows.push(
            render::verdict_json(&v, slot, &[])
                .set("score", r.score)
                .set("why", why),
        );
        deps.push(
            Json::obj()
                .set("key", r.key.as_str())
                .set("state", v.token())
                .set("exit", v.exit())
                .set("unresolved", unresolved)
                .set_opt("adr", v.adr().map(|a| a.as_str().to_string())),
        );

        // --- the same row, for a person
        let lead = match &v {
            aval_core::graph::Verdict::Active { adr, choice, .. } => {
                format!("active          {}   {}", adr, choice)
            }
            aval_core::graph::Verdict::Retired { adr, .. } => format!("retired         {}", adr),
            other => format!("{:<15} {}", other.token(), other.note(slot)),
        };
        text.push_str(&format!(
            "  {:>8.4}  {:<width$}  {}\n",
            r.score,
            r.key,
            lead,
            width = width
        ));
        if let aval_core::graph::Verdict::Contradiction { heads, .. } = &v {
            text.push_str(&format!(
                "{:width$}  stop; do not pick one: {}\n",
                "",
                heads.join(", "),
                width = width + 12
            ));
        }
        if !also.is_empty() {
            let list: Vec<String> = also
                .iter()
                .map(|(scope, state, adrs)| format!("{} ({} {})", scope, state, adrs.join(", ")))
                .collect();
            text.push_str(&format!(
                "{:width$}  also decided at {}\n",
                "",
                list.join(", "),
                width = width + 12
            ));
        }
    }

    if rows.is_empty() {
        text.push_str("  nothing in this corpus matched.\n");
    }

    if !rules.is_empty() {
        text.push_str("\nrules mentioning the same words (advisory, and adopted by a record):\n");
        let rw = rules.iter().map(|(r, _)| r.id.len()).max().unwrap_or(0);
        for (r, _) in rules {
            text.push_str(&format!(
                "  {:<10} {:<rw$}  {}\n",
                r.level.as_str(),
                r.id,
                shorten(&r.statement),
                rw = rw
            ));
        }
    }

    let plural = |n: usize, one: &str, many: &str| {
        if n == 1 {
            format!("{} {}", n, one)
        } else {
            format!("{} {}", n, many)
        }
    };
    let asked = match (
        paths.len(),
        q.text.as_deref().unwrap_or("").trim().is_empty(),
    ) {
        (0, _) => "a text query".to_string(),
        (n, true) => plural(n, "path", "paths"),
        (n, false) => format!("{} and a text query", plural(n, "path", "paths")),
    };
    text.push_str(&format!(
        "\nshowing {} of {} matched, from {}. {}\n",
        plural(rows.len(), "key", "keys"),
        total,
        asked,
        ADVISORY
    ));

    let json = Json::obj()
        .set("kind", "suggestion")
        .set("advisory", true)
        .set("note", ADVISORY)
        .set("scope", q.scope.as_str())
        .set(
            "query",
            Json::obj()
                .set("paths", paths.to_vec())
                .set_opt("text", q.text.clone())
                .set("changed", q.changed)
                .set("top", q.top),
        )
        .set("ranked", rows.len())
        .set("matched", total)
        .set("keys", rows)
        .set(
            "rules",
            rules
                .iter()
                .map(|(r, s)| {
                    Json::obj()
                        .set("id", r.id.as_str())
                        .set("level", r.level.as_str())
                        .set("adopts", r.adopts.as_str())
                        .set("statement", r.statement.as_str())
                        .set("score", *s)
                })
                .collect::<Vec<_>>(),
        )
        .set("dependencies", deps);

    Reply {
        json,
        text,
        // Always 0. A ranking is a suggestion, and a suggestion has no verdict
        // to report — a non-zero code here would put "I looked and found little"
        // in the same range as "I could not look".
        exit: 0,
        is_error: false,
    }
}

/// One line of a statement, cut at a word boundary.
///
/// Cut on a character boundary, not a byte one: a statement carrying `—` or an
/// accented letter would otherwise panic on the slice.
fn shorten(s: &str) -> String {
    if s.chars().count() <= STATEMENT_WIDTH {
        return s.to_string();
    }
    let head: String = s.chars().take(STATEMENT_WIDTH).collect();
    let cut = head.rfind(' ').unwrap_or(head.len());
    format!("{} …", &head[..cut])
}

fn matched_scope(v: &aval_core::graph::Verdict) -> Option<&str> {
    match v {
        aval_core::graph::Verdict::Active { matched_scope, .. }
        | aval_core::graph::Verdict::Retired { matched_scope, .. }
        | aval_core::graph::Verdict::Contradiction { matched_scope, .. } => {
            Some(matched_scope.as_str())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_covers_the_files_under_it_and_not_a_neighbour() {
        assert!(covers("crates/aval", "crates/aval/src/mcp.rs"));
        assert!(!covers("crates/av", "crates/aval/src/mcp.rs"));
        assert!(covers("a/b", "a/b"));
        assert!(!covers("crates/aval/src/mcp.rs", "crates/aval"));
        // Both directions, because a caller may name either end.
        assert!(related("crates/aval/src/mcp.rs", "crates/aval"));
    }

    #[test]
    fn a_glob_matches_by_its_literal_directory_prefix() {
        assert_eq!(
            glob_prefix("kubernetes/apps/*/values.yaml").as_deref(),
            Some("kubernetes/apps")
        );
        assert_eq!(glob_prefix("*.tsx"), None);
        assert!(mentions(
            "kubernetes/apps/forgejo/values.yaml",
            "kubernetes/apps/*/values.yaml"
        ));
        assert!(!mentions(
            "docs/adr/0001-x.md",
            "kubernetes/apps/*/values.yaml"
        ));
        // A pattern with no directory to stand on matches nothing rather than
        // everything.
        assert!(!mentions("app/routes/x.tsx", "*.tsx"));
    }

    #[test]
    fn a_body_names_paths_in_backticks_and_in_links() {
        let src = "---\nid: ADR-0001\n---\n# t\n\nSee `kubernetes/apps/forgejo` and \
                   [values](infrastructure/values.yaml), plus \
                   `kubernetes/**/*.yaml`.\n";
        let got = mentioned_paths(src);
        assert!(
            got.contains(&"kubernetes/apps/forgejo".to_string()),
            "{:?}",
            got
        );
        assert!(
            got.contains(&"infrastructure/values.yaml".to_string()),
            "{:?}",
            got
        );
        // A glob is kept here, where `links` drops it: a pattern is how a
        // record says which files it governs.
        assert!(
            got.contains(&"kubernetes/**/*.yaml".to_string()),
            "{:?}",
            got
        );
    }

    #[test]
    fn fenced_code_and_frontmatter_name_nothing() {
        let src =
            "---\nid: ADR-0001\ndecisions: [a/b.yaml]\n---\n```\n`x/y.yaml`\n```\n`z/w.yaml`\n";
        assert_eq!(mentioned_paths(src), ["z/w.yaml"]);
    }

    #[test]
    fn a_url_and_a_bare_word_are_not_paths() {
        let src = "---\nid: A\n---\n[x](https://example.com/a/b) and `accepted` and `/etc/hosts`\n";
        assert!(
            mentioned_paths(src).is_empty(),
            "{:?}",
            mentioned_paths(src)
        );
    }

    #[test]
    fn a_title_is_the_first_heading() {
        assert_eq!(
            title("\nintro\n# 0021 — Ceph RGW\n## Context\n"),
            "0021 — Ceph RGW"
        );
        assert_eq!(title("no heading here\n"), "");
    }

    #[test]
    fn a_query_with_no_signal_is_empty() {
        assert!(Query::default().is_empty());
        assert!(Query {
            text: Some("   ".into()),
            ..Query::default()
        }
        .is_empty());
        assert!(!Query {
            changed: true,
            ..Query::default()
        }
        .is_empty());
    }
}
