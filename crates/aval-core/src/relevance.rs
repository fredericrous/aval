//! Retrieval: which documents look like the thing a caller is about to touch.
//!
//! This is the one place in the crate where similarity is computed, and it is
//! deliberately walled off from everything else. **Nothing here resolves.**
//! `resolve` (SEMANTICS section 5) is exact, total and deterministic over the
//! graph; a ranking is a guess about attention, and section 5.1 gives it the
//! `suggestion` class that already exists for the did-you-mean — advisory, never
//! substituted for an answer.
//!
//! What that buys: the ranking can be wrong without any verdict being wrong.
//! The ranker never sees a head, never picks between two, and never decides that
//! a key is not worth reporting — the caller resolves every key it is handed.
//!
//! # The scoring
//!
//! Okapi BM25 over a document per key, with a **frozen** tokenizer. Frozen
//! because the ranking is compared byte-for-byte by the conformance battery: a
//! stemmer that grew a rule would reorder a corpus nobody touched, and
//! "retrieval improved" is indistinguishable from "retrieval broke" without a
//! fixture that fails. Changing anything in this file changes fixture output,
//! which is the signal SEMANTICS section 15 asks for.
//!
//! No training, no network, no clock: the same corpus and the same query give
//! the same order, on any machine, forever.

use std::collections::BTreeMap;

/// The BM25 term-frequency saturation. The literature's default.
pub const K1: f64 = 1.2;
/// The BM25 length normalisation. The literature's default.
pub const B: f64 = 0.75;

/// Tokens shorter than this are dropped.
///
/// **Two, not three.** The usual floor is three characters, and in this corpus
/// three would discard `ci`, `ui`, `s3`, `db` and `go` — every one of which is
/// a segment of a real decision key (`ci.system`, `ui.design-system`). A floor
/// that throws away the vocabulary it is searching is the wrong floor. One
/// character is still dropped: a lone `a` or `s` is punctuation's residue.
pub const MIN_TOKEN: usize = 2;

/// Words carrying no retrieval signal in English prose.
///
/// Deliberately short. A long stop list starts deleting domain words — `state`,
/// `system`, `use` — and every deletion is invisible at the call site, because
/// a query term that was dropped scores exactly like a term nothing matched.
const STOP: &[&str] = &[
    "about", "after", "all", "also", "an", "and", "any", "are", "as", "at", "be", "because",
    "been", "before", "being", "between", "both", "but", "by", "can", "cannot", "did", "do",
    "does", "each", "for", "from", "had", "has", "have", "how", "if", "in", "into", "is", "it",
    "its", "may", "more", "most", "must", "no", "nor", "not", "of", "on", "one", "only", "or",
    "other", "our", "out", "over", "own", "per", "rather", "same", "should", "since", "so", "some",
    "such", "than", "that", "the", "their", "them", "then", "there", "these", "they", "this",
    "those", "through", "to", "too", "under", "until", "up", "very", "was", "we", "were", "what",
    "when", "where", "which", "while", "who", "why", "will", "with", "would", "you", "your",
];

/// File extensions, dropped from a path's tokens.
///
/// A path is tokenised for what it is *about*, and `.rs` is about nothing: every
/// file in a Rust crate carries it, so it is a term with a document frequency of
/// one hundred percent and an inverse document frequency of nearly zero. It is
/// dropped outright rather than left to BM25 because a caller passing one path
/// has few tokens to spare and each wasted one costs a rank.
const EXTENSIONS: &[&str] = &[
    "bash", "c", "cc", "cfg", "conf", "cpp", "cs", "css", "erb", "fish", "go", "h", "hs", "htm",
    "html", "ini", "java", "js", "json", "jsx", "kt", "lock", "md", "mjs", "php", "pl", "png",
    "py", "rb", "rs", "rst", "scss", "sh", "sql", "svg", "swift", "tf", "toml", "ts", "tsx", "txt",
    "yaml", "yml", "zsh",
];

/// One character, folded to ASCII where there is an obvious equivalent.
///
/// Not a Unicode normalisation: this crate has no dependencies and NFD is not
/// three lines. It covers the Latin-1 letters a French-speaking estate actually
/// writes, and leaves every other character alone — a Greek or Cyrillic token
/// stays itself and still matches itself, which is all the tokenizer promises.
fn fold(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        other => other,
    }
}

/// A light suffix stemmer: enough to join `modes` to `mode`, and no more.
///
/// Porter would conflate more pairs and would also conflate pairs that matter
/// here — `routing` and `route` are one word, `router` is a different thing.
/// Four rules, each reversible by reading, is the trade: the failure mode of a
/// small stemmer is a match not made, and the failure mode of a large one is a
/// match made wrongly and never noticed.
fn stem(t: &str) -> String {
    let n = t.len();
    if n > 4 && t.ends_with("ies") {
        return format!("{}y", &t[..n - 3]);
    }
    if n > 5 && t.ends_with("ing") {
        return t[..n - 3].to_string();
    }
    if n > 4 && t.ends_with("ed") {
        return t[..n - 2].to_string();
    }
    // `status` and `https` keep their `s`: an `s` after `s` or `u` is part of
    // the word, not a plural. Without this, `status` stems to `statu`, which
    // matches nothing a reader would expect it to.
    if n > 3 && t.ends_with('s') && !t.ends_with("ss") && !t.ends_with("us") {
        return t[..n - 1].to_string();
    }
    t.to_string()
}

/// Text into terms. The frozen order: lowercase, fold, split, drop, stem.
///
/// Splitting on every non-alphanumeric is what makes this work on a decision
/// key without a special case: `storage.object-store` and "object store" reach
/// the same three terms.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        let c = fold(c.to_lowercase().next().unwrap_or(c));
        if c.is_alphanumeric() {
            cur.push(c);
        } else if !cur.is_empty() {
            push_token(&cur, &mut out);
            cur.clear();
        }
    }
    if !cur.is_empty() {
        push_token(&cur, &mut out);
    }
    out
}

fn push_token(t: &str, out: &mut Vec<String>) {
    if t.chars().count() < MIN_TOKEN || STOP.contains(&t) {
        return;
    }
    out.push(stem(t));
}

/// A path into terms: its directories, its stem, and not its extension.
pub fn path_tokens(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    for seg in path.split(['/', '\\']) {
        let mut parts = seg.rsplitn(2, '.');
        let last = parts.next().unwrap_or(seg);
        let rest = parts.next();
        match rest {
            // `mcp.rs` → `mcp`; a dotfile like `.adr.yaml` keeps `adr`.
            Some(head) if EXTENSIONS.contains(&last) => out.extend(tokenize(head)),
            _ => out.extend(tokenize(seg)),
        }
    }
    out
}

/// One document in the index: weighted term counts and their total.
///
/// Weights are repetition counts, so a title's terms simply occur more often
/// than a body's. That keeps the field weighting inside BM25's own model
/// instead of adding a second scoring scheme on top of it.
#[derive(Debug, Clone, Default)]
pub struct Doc {
    id: String,
    terms: BTreeMap<String, f64>,
    len: f64,
}

impl Doc {
    pub fn new(id: impl Into<String>) -> Doc {
        Doc {
            id: id.into(),
            terms: BTreeMap::new(),
            len: 0.0,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// Add a field's text at a weight. Empty text and a zero weight are no-ops.
    pub fn add(&mut self, text: &str, weight: f64) {
        if weight <= 0.0 {
            return;
        }
        for t in tokenize(text) {
            *self.terms.entry(t).or_insert(0.0) += weight;
            self.len += weight;
        }
    }

    /// Add terms already tokenised, for a field whose tokenizer differs — a
    /// path, which drops its extension.
    pub fn add_terms(&mut self, terms: &[String], weight: f64) {
        if weight <= 0.0 {
            return;
        }
        for t in terms {
            *self.terms.entry(t.clone()).or_insert(0.0) += weight;
            self.len += weight;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}

/// A built index: document frequencies and the mean document length.
#[derive(Debug, Default)]
pub struct Index {
    docs: Vec<Doc>,
    df: BTreeMap<String, usize>,
    avg_len: f64,
}

impl Index {
    pub fn build(docs: Vec<Doc>) -> Index {
        let mut df: BTreeMap<String, usize> = BTreeMap::new();
        let mut total = 0.0;
        for d in &docs {
            total += d.len;
            for t in d.terms.keys() {
                *df.entry(t.clone()).or_insert(0) += 1;
            }
        }
        let avg_len = if docs.is_empty() {
            0.0
        } else {
            total / docs.len() as f64
        };
        Index { docs, df, avg_len }
    }

    pub fn docs(&self) -> &[Doc] {
        &self.docs
    }

    /// BM25 for one document against a query, as a sum over the query's terms.
    ///
    /// A term nothing carries contributes nothing, and a term every document
    /// carries contributes almost nothing — which is the property that keeps a
    /// word like `decision` from ranking a decision corpus at random.
    pub fn score(&self, doc: usize, query: &[String]) -> f64 {
        let Some(d) = self.docs.get(doc) else {
            return 0.0;
        };
        if query.is_empty() || d.len == 0.0 {
            return 0.0;
        }
        let n = self.docs.len() as f64;
        let norm = if self.avg_len > 0.0 {
            1.0 - B + B * (d.len / self.avg_len)
        } else {
            1.0
        };
        let mut total = 0.0;
        // Deduplicated: a query repeating a word is asking once, and BM25's
        // saturation is over the DOCUMENT's term frequency, not the query's.
        let mut seen: Vec<&str> = Vec::new();
        for t in query {
            if seen.contains(&t.as_str()) {
                continue;
            }
            seen.push(t);
            let Some(tf) = d.terms.get(t) else { continue };
            let df = *self.df.get(t).unwrap_or(&0) as f64;
            let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
            total += idf * (tf * (K1 + 1.0)) / (tf + K1 * norm);
        }
        total
    }
}

/// Four decimals, so a score is comparable, printable and sortable as one
/// number rather than three.
///
/// Rounding before the sort is load-bearing: two scores that differ in the
/// fifteenth digit are the same score to a reader, and letting that difference
/// decide the order makes the order an artefact of floating-point addition
/// rather than of the corpus. Ties are broken by name, above.
pub fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Vec<String> {
        tokenize(s)
    }

    #[test]
    fn a_decision_key_and_its_prose_reach_the_same_terms() {
        assert_eq!(t("storage.object-store"), ["storage", "object", "store"]);
        assert_eq!(t("the Object Stores"), ["object", "store"]);
    }

    #[test]
    fn stop_words_and_one_character_tokens_go() {
        assert_eq!(t("it is a gateway"), ["gateway"]);
        // Two characters stay: `ci` and `ui` are decision keys here.
        assert_eq!(t("ci.system and ui"), ["ci", "system", "ui"]);
    }

    #[test]
    fn accents_fold_and_case_does_not_matter() {
        assert_eq!(t("Déployé"), t("deploye"));
    }

    #[test]
    fn the_stemmer_joins_a_plural_and_leaves_status_alone() {
        assert_eq!(stem("modes"), "mode");
        assert_eq!(stem("policies"), "policy");
        assert_eq!(stem("routing"), "rout");
        assert_eq!(stem("replaced"), "replac");
        assert_eq!(stem("status"), "status");
        assert_eq!(stem("class"), "class");
    }

    #[test]
    fn a_path_loses_its_extension_and_keeps_its_directories() {
        // `crates` stems to `crate`, exactly as the same word in prose does —
        // which is the point of running one tokenizer over both sides.
        assert_eq!(
            path_tokens("crates/aval/src/mcp.rs"),
            ["crate", "aval", "src", "mcp"]
        );
        assert_eq!(
            path_tokens("docs/adr/0021-ceph-rgw.md"),
            ["doc", "adr", "0021", "ceph", "rgw"]
        );
        // A directory carrying a dot is not an extension to strip.
        assert_eq!(
            path_tokens(".github/workflows/ci.yaml"),
            ["github", "workflow", "ci"]
        );
    }

    fn tiny() -> Index {
        let mut a = Doc::new("storage.object-store");
        a.add("storage object store", 3.0);
        a.add("Ceph RGW replaces Garage for homelab object storage", 1.0);
        let mut b = Doc::new("cni.routing-mode");
        b.add("cni routing mode", 3.0);
        b.add("Cilium native routing needs a shared L2 segment", 1.0);
        let mut c = Doc::new("gitops.reconciler");
        c.add("gitops reconciler", 3.0);
        c.add("Flux over Argo CD for every cluster", 1.0);
        Index::build(vec![a, b, c])
    }

    #[test]
    fn the_ranker_finds_the_document_a_query_is_about() {
        let ix = tiny();
        let q = tokenize("where do objects get stored");
        let best = (0..ix.docs().len())
            .max_by(|x, y| ix.score(*x, &q).total_cmp(&ix.score(*y, &q)))
            .expect("a document");
        assert_eq!(ix.docs()[best].id(), "storage.object-store");
    }

    #[test]
    fn a_query_matching_nothing_scores_zero_everywhere() {
        let ix = tiny();
        let q = tokenize("quarterly expiry reversal");
        for i in 0..ix.docs().len() {
            assert_eq!(ix.score(i, &q), 0.0);
        }
    }

    #[test]
    fn ranking_is_deterministic_over_the_same_index() {
        let q = tokenize("cilium routing");
        let once: Vec<f64> = (0..3).map(|i| tiny().score(i, &q)).collect();
        let twice: Vec<f64> = (0..3).map(|i| tiny().score(i, &q)).collect();
        assert_eq!(once, twice);
    }

    #[test]
    fn a_term_every_document_carries_ranks_nothing() {
        let mut a = Doc::new("a");
        a.add("decision decision decision", 1.0);
        let mut b = Doc::new("b");
        b.add("decision", 1.0);
        let ix = Index::build(vec![a, b]);
        let q = tokenize("decision");
        // idf of a term in every document is ln(1 + 0.5/2.5) ≈ 0.18, so the
        // score is near zero rather than the largest number in the ranking.
        assert!(ix.score(0, &q) < 0.5, "{}", ix.score(0, &q));
    }

    #[test]
    fn the_query_counts_a_repeated_term_once() {
        let ix = tiny();
        let a = ix.score(0, &tokenize("object"));
        let b = ix.score(0, &tokenize("object object object"));
        assert_eq!(a, b);
    }
}
