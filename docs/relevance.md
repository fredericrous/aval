# Which decisions bear on this change

`resolve` needs a key, which is a chicken-and-egg problem at the start of a
task: the way to learn that a decision governs the file you are about to edit
is to already know its name. The alternative is `HEADS.md`, which is every
decision the repository has ever made.

`aval relevant` ranks the vocabulary against what you are about to touch. Run
in this repository, whose decisions are vendored from a fleet pack:

```console
$ aval relevant --path crates/aval/src/mcp.rs --text "release a new version by pushing a tag"
advisory   A ranking is a suggestion: it resolves nothing. Only `aval resolve <key>` answers.

   15.7899  release.trigger         active          decisions:ADR-0008   A pushed version tag, never a merge
    7.9137  release.version-scheme  active          decisions:ADR-0012   Semantic Versioning 2.0.0

rules mentioning the same words (advisory, and adopted by a record):
  heuristic  components.release-reuse-equivalence  The granule of reuse is the granule of release: what is reused together is versioned and …
  constraint components.acyclic-dependencies       There are no cycles in the component dependency graph.
  …

hidden by traits (cli): 5 constraint(s), 4 heuristic(s); `aval rules --all-traits` lists them

showing 2 keys of 5 matched, from 1 path and a text query. A ranking is a suggestion: it resolves nothing. Only `aval resolve <key>` answers.
```

**This is retrieval, not resolution**, and the output says so on every run. The
order is a guess about attention. What is not a guess is the verdict on each
row: it comes from the same `resolve` the rest of the tool runs. That is what
makes an `undecided` row the interesting one — nobody decided it, and an agent
that quietly fills the gap is doing the thing the corpus exists to prevent:

```console
$ aval relevant --text "which forge hosts the repository"
advisory   A ranking is a suggestion: it resolves nothing. Only `aval resolve <key>` answers.

    7.9168  forge.primary              undecided       no accepted decision for forge.primary
    2.3326  ci.system                  active          decisions:ADR-0017   GitHub Actions for an open-source repository, Forgejo Actions for a private one
…
```

## The signals

| Signal | Weight | What it reads |
|---|---|---|
| text | 1.0 | BM25 over `--text`, against one document per key: the key's name and description, then the title, choice, reason and body of whatever decides it, and superseded titles at a third of the weight |
| path | 0.6 | the same BM25 over the words in each `--path` — directories and file stems, extensions dropped |
| mention | 2.0 per path, three at most | a record's body names that path itself: backticked, as a markdown link, or as a glob matched by its literal directory |
| co-change | 0.5 per path, three at most | `git log` says the commits that wrote the record also touched that path |

A mention outranks any amount of word overlap, because it is the one signal an
author put there on purpose. Co-change is weakest and capped hardest: a record
and a config file in one commit may share nothing but a Tuesday. The tokenizer
is frozen — lowercase, ASCII-fold, split on every non-alphanumeric, drop stop
words and one-character tokens, light suffix stemming — and the conformance
battery compares the resulting order byte for byte, so changing it is a change
somebody reviews rather than a ranking that quietly moved.

`--changed` adds what git reports modified, staged and untracked, which is the
whole query for "what does this branch touch". `--scope S` ranks only the keys
answerable at S and resolves each one there. `--top N` defaults to 5, and a row
must also score a fifth of the top row to be printed, so a query with one good
answer reports one. [Rules](rules-and-traits.md) that match the same words are
listed after the keys, clearly apart, and the rules the repository's traits
hide are counted rather than dropped.

Exit is **always 0**, usage errors aside. A ranking has no verdict to report,
and "I ranked and found little" must not share a code with "I could not look".

## For a router

`--json` carries a `dependencies` array — the same keys, compacted:

```json
{"dependencies":[
  {"key":"release.trigger","state":"active","exit":0,"adr":"decisions:ADR-0008","unresolved":false},
  {"key":"forge.primary","state":"undecided","exit":4,"unresolved":true}]}
```

One row per ranked key, in ranked order, and `unresolved` is true for exactly
`undecided` and `contradiction`. That is what a dispatcher reads —
[relais](https://github.com/fredericrous/relais) routes on unresolved decision
dependencies — so a change whose decisions are not settled goes to a person
instead of to a worker. The full verdict, with the choice and the reason, is in
`keys`; `why` on each row says which signal earned it its place, and
`decided_elsewhere` names the scopes that do decide a key the asked scope does
not.
