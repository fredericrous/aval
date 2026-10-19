---
status: active
branch: fix/pack-check-content
repos: [aval]
adrs: []
---
# aval: a pack is behind only when its declarations moved

## Review panel

👉 **Decide:** none. Approve if one aval 1.9.1 (pipe drain, compare by content, decode escapes) should silence 23 false notices.
📍 aval · planned, not started · next: worktree aval-wt-pack-content, then the fetch.rs drain. Panel: backend, lang:rust, tui, unix (opus-5-5), codex.
**Changed by review:** a fetch that fails after a move stays `behind`; the budgeted fetch deadlocked on packs over 64 KB (Codex), now fixed; the `aval:` version field is ignored.
**Verdicts:** 4 approve-with-changes in round 1; backend twice more, on deltas; Codex rework, resolved by its edits. Carried into implementation: 7, listed below.
📄 Full reviews: [2026-10-19-pack-behind-when-declarations-moved.reviews.md](2026-10-19-pack-behind-when-declarations-moved.reviews.md) (carried into implementation: the six project.rs tests, not four; the fetch-fail row uses a 40-hex `rev` that `resolve` answers offline; the parse-error sentence names the moved-commit comparison; the exact CurrentMoved line pinned in a test; an unbalanced backtick; an ASCII-punctuation constant; docs note that re-add shows the diff).

## Context

The 2026-10-18 fleet refresh found two aval defects.

1. **`aval add --check` compares commit ids, not declarations.** It sends
   `standing()` (`crates/aval/src/add.rs:545`) straight to `Behind` whenever the
   revision names a different commit. Plan-doc commits to `decisions` (#78,
   #79) left the pack byte-identical, yet all 23 consumers report "behind",
   and the session hook prints BEHIND THEIR SOURCE in every repository. A
   notice that fires on no change is the notice nobody reads on the day it
   matters, which is the argument SEMANTICS already makes for `--quiet`.
2. **`heads --check` calls a prettier-formatted HEADS.md stale.** prettier
   escapes `*` as `\*` inside table cells. `canonicalise()`
   (`crates/aval-core/src/project.rs:149`) normalises padding and the
   delimiter row, but not a CommonMark backslash escape that a formatter
   adds. application-landscape and website-builder needed a `.prettierignore`
   entry as a workaround. SEMANTICS §12.2 says canonicalisation "never has to
   decode an escape". That holds only while nothing adds one to one side.

Outcome: a pack whose source moved without changing its declarations is
`current`. A formatter's escaping is formatting. Released as aval 1.9.1.

## Approach

### 1. `standing()`: compare declarations when the commit moved

In `add.rs`, when `fetch::resolve` returns an id different from `o.commit`:
- fetch `pack::FILE` at the new id into a `Scratch`, the same way
  `unedited()` fetches at the old one;
- first repair the budgeted subprocess path in `fetch.rs`: drain stdout and stderr concurrently while enforcing the deadline, using the standard library. On the deadline, kill and reap `git`, then return without joining the reader threads (an `ssh` or `git-remote-https` grandchild may still hold the pipe open, so a join could hang); join them only after a normal exit. Regression: `--budget 2` against an unreachable ssh source returns in under 3 s. Waiting for exit before draining deadlocks when `git show` fills the pipe;
- add a local integration regression with a valid pack at least 269 KB long: after a docs-only producer commit, `add --check --quiet --budget 5` must finish before the deadline, exit 0, and print nothing. Repeat with a changed declaration and require `behind`, exit 1;
- introduce a comparison specifically for the moved-commit standing check: parse both YAML mappings, exclude only the top-level `aval` generator-version field, and compare every remaining value with `yaml::same_values`. Parse failures still compare unequal. Leave `declares_the_same` and existing `aval add` behavior unchanged;
- add a regression where the producer commit changes only the top-level `aval` version: the consumer remains `current`, including with `--quiet`. Pair it with a changed declaration that remains `Behind`;
- if it is true, return `Current`, because the file already declares what the
  source publishes now. Otherwise return `Behind(id)` as today. A published
  file that does not parse is not the same (`declares_the_same` is false on a
  parse error), so it is `Behind`; a test pins it;
- if the fetch fails (budget, network, or the revision moved again), return
  `Behind(id)` as today: the revision already answered that it moved, and
  `Unknown` exits 0 silently under `--quiet` (`main.rs:1183-1208`), which
  would hide a real move whenever the 5 s hook budget runs out.

Rename nothing. Carry the new commit on the `Current` result (or a `CurrentMoved` variant) so the line can read `current <name> @ <old>  (<source>; <rev> now names <new>, same declarations)`, so the reader sees why the commit differs. Keep the plain line when the commit has not moved. A pack that is current in content
but carries an old banner commit is fine: `aval add` refreshes the banner the
next time it runs for a real change. `Edited` keeps its meaning, since it is
still asked only when the commit is unchanged. A file that was edited AND whose
source moved is `Behind` unless the edit happens to equal the new
publication, and then it is genuinely current.

Cost: one extra fetch whenever the revision names a commit other than the vendored one. That recurs on every check while the banner is stale, because nothing refreshes it for an unchanged pack. The hook's `--budget` still bounds it, because `fetch::set_budget` covers every fetch. A test runs `--check` twice on a stale-banner pack and expects `current` both times.

### 2. `canonicalise()`: decode a backslash escape inside a table cell

In `canonical_row()` (`project.rs:171`), for each non-delimiter field after
`split_unescaped`:
- decode every CommonMark backslash escape of ASCII punctuation (`\X` →
  `X`; it means the literal X, so `\*` and `*` say the same thing), in one
  left-to-right pass that consumes both characters, so `\\*` is `\` then `*`;
- re-encode with `cell()`'s own escaping (`\` → `\\`, `|` → `\|`).

Re-encoding is what keeps `| A\|B |` (one cell) distinct from `| A | B |`
(two cells). Both sides go through it, so the render, which only ever emits
`\\` and `\|`, is unchanged by it. Inside a backtick code span a backslash is literal in CommonMark, so no decode happens there (unit test). A backslash before a non-punctuation
character stays as it is, because CommonMark does not treat it as an escape.

Lines outside tables keep today's whitelist exactly. The banner and headings
are not touched.

### 3. Spec, docs, version

- `SEMANTICS.md`:
  - §12.2 table: add the row "a backslash escape of ASCII punctuation in a
    table cell — a formatter adds or drops them, and `\*` and `*` say the same
    thing";
  - §12.2 last paragraph: reword "never has to decode an escape" to state
    what is decoded and why the re-encode keeps the column split;
  - §"Staleness is reported, never repaired" (line ~300): `behind` means the
    revision names a commit that publishes different declarations.
- `docs/sharing.md` § "Knowing when a pack fell behind": one sentence to the
  same effect.
- Doc comments: `project.rs:147` (what is decoded), `add.rs:505` (`Behind`
  means different declarations), `add.rs:561` (`unedited` is no longer the
  only fetch).
- `CHANGELOG.md` `## v1.9.1` → `### Fixed`, three entries; the third says that under `--budget` (the hook) any pack over the pipe buffer was silently `unknown`, so `edited`/`behind` never reached the hook for large packs.
- `Cargo.toml` version and the `aval-core` dependency: 1.9.0 → 1.9.1.

### Tests

The local amont registry places `cargo-test` at pre-push, despite the nearby fleet decision naming pre-commit. Run `cargo test` explicitly before committing this change, record its result, and run `amont rehearse --wait` before pushing. Do not rely on pre-commit to execute the tests.

- `crates/aval/tests/pack.rs`: the producer commits a change that leaves the
  pack unchanged (a `docs/plans/x.md` file), and `add --check` exits 0 with
  `current`. With `--quiet` it prints nothing. The existing "moved" tests, which
  add a record, stay `behind`. A hand-edited pack whose source moved is
  `behind` (re-adding shows the diff before it overwrites the edit).
- `project.rs` unit tests:
  - prettier's `\*` and `\_` in a cell canonicalise equal to the render;
  - `\|` vs a real column split stay different;
  - an escaped backslash `\\` in the render still equals itself;
  - `a\b` (not punctuation) is untouched;
  - the render `a\\*` (choice `a\*`) equals prettier's `a\\\*`;
  - `\<` and a backtick escape decode like `\*`.
- `heads.rs`: one round-trip from `compare()`: a projection with a
  `*` in a choice, prettier-escaped, is `Current`.

## Non-goals

- Bumping `AVAL_VERSION: 1.9.0` in the consumers' CI pins, or removing the two
  `.prettierignore` workarounds. Each rides that repository's next change.
- Re-vendoring 23 packs just to move the banner commit to `ee009f3`.
- Any change to `aval add` itself (`plan()` already compares declarations).

## Phases

- [x] 1. Implementation, tests, spec, changelog, version, in the `aval-wt-pack-content` worktree (relais run run-65e2c1db0b71d-24b4, sonnet@medium; review fixes by hand)
- [ ] 2. PR, merge on green, tag `v1.9.1`, install the released binary on this machine. Its evidence is the PR checks and the `v1.9.1` release workflow, not this file.

## Verification

Observed on 2026-10-19 with the branch binary (`target/debug/aval`) at 9d79a7a, before the push.

| Check | Input | Expected | Actual |
|---|---|---|---|
| Unit and integration | `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` | the new pack.rs tests and the project.rs tests pass; existing `behind` tests unchanged | 13 suites ok, 0 failed; clippy and fmt clean. New tests: 6 in pack.rs, 9 in project.rs, 1 in heads.rs |
| Hook end to end | `.claude/hooks/aval-heads.sh` in amont, fresh `XDG_CACHE_HOME`, branch binary first on PATH; then with 1.9.0 | no `BEHIND THEIR SOURCE` line | branch: 0 lines; 1.9.0: 1 line |
| Fetch fails after a move | pack.rs `a_move_that_cannot_be_compared_stays_behind`: a 40-hex `rev` absent from the source | `behind`, exit 1 | passes |
| Real fleet, budgeted, 269 KB pack | scratch consumer vendoring amont's 269,463-byte pack at the source's HEAD (unmoved), `time aval add --check --budget 5` | branch: `current`, no `unknown`, exit 0, < 5 s; 1.9.0: `unknown` after ~5 s | branch: `current`, exit 0, 2.25 s; 1.9.0: `unknown … has no aval.pack at 2296a63`, exit 0, 5.03 s (the pipe deadlock) |
| Real fleet, commit moved, content same | `time aval add --check --budget 5` in `~/Developer/Perso/amont` (vendors 99ac9a7; HEAD now 2296a63, docs-only commits since) | `current`, exit 0 | `current decisions @ 99ac9a7 (github:fredericrous/decisions; HEAD now names 2296a63, same declarations)`, exit 0, 2.9 s; 1.9.0: `behind`, exit 1 |
| Real fleet, content moved | the scratch consumer with the banner at 99ac9a7 and one key description edited | `behind`, exit 1 | `behind decisions has 99ac9a7, HEAD now names 2296a63`, exit 1 |
| Prettier round-trip | website-builder's `.adr.yaml`, `.adr`, `docs/adr` from HEAD (no `.prettierignore`), `prettier --write docs/adr/HEADS.md` (3.8.3, one `\*`), `aval heads --check` | `HEADS.md is current` | branch: `HEADS.md is current`; 1.9.0: `heads-fresh` |
| Content edit still caught | the same copy with `frozen` changed to `thawed` in one decision | `heads-fresh` finding | 2 `heads-fresh` findings (says / is missing) |

The branch binary runs from `target/`, never `cargo install`, so the
machine binary is replaced only by the release (memory: a branch binary
replaces the machine's one).

## Implementation review

approve-with-changes, then on the Delta approve-with-changes (its one remaining item is this commit).
Fixed: a race on the process-wide budget between two fetch.rs tests (the duplicate test was dropped; pack.rs covers the path); pipe read errors now surfaced; SEMANTICS paragraph split; a redundant assertion.
Fixed here: Actual column, Phase 1 ticked, Release moved to Phase 2's post-merge evidence.
Round 1: 55k tokens, 73 s; Delta: 59k tokens, 26 s.

<!-- panel: repos=aval reviewers=backend,lang:rust,tui,unix,codex body-sha=5b4f4ad2d4cc -->
