---
status: done
branch: feat/hook-rule-index
repos: [aval]
adrs: [decisions:ADR-0026, decisions:ADR-0022]
---
# The session hook prints a rule index

## Review panel
👉 **Decide:** none — approve if constraint ids alone, with `aval rule <id>` on demand, keep agents following constraints.
📍 aval · plan only, nothing built · next: worktree, plan commit, relais run for Phases 1–2. Panel: backend, lang:rust, tui, unix.
**Changed by review:** N from the plain listing; old-aval fallback prints ids; `--index` needs `--level`, unwrapped when piped.
**Changed by your review:** valid-id fixtures; refusals tested with `--level`; grouped hook assertion; MSRV must build; adherence Outcome.
**Verdicts:** round 1: 4 approve-with-changes; round 2 and final delta: backend approve; 2 low notes carried into implementation.
📄 Full reviews: [2026-10-04-session-hook-prints-rule-index.reviews.md](2026-10-04-session-hook-prints-rule-index.reviews.md)

## Context
decisions ADR-0026 (merged in decisions#39) adopts
`guidance.always-on-is-an-index`: text put into every agent session is an
index, one line per item, detail on demand. The aval session hook does not
conform. In the decisions repository it prints 32,168 bytes (~8,000 tokens)
per session: 26,089 bytes are the full first paragraph of all 73 active
constraints, 4,108 the heads table, ~2,000 the preamble. ADR-0026 names this
as its known gap. The person chose the index form: constraint ids, grouped
by prefix (~2.1 KB).

## Goal
The hook lists active constraints as ids grouped by prefix, not as
statements. Expected size in the decisions repository:
32,168 − 26,089 + ~2,100 ≈ 8,200 bytes, before any preamble trim. The rule
text stays one call away: `aval rule <id>`, MCP `aval_rule`.

## Non-goals
- No change to `aval heads`, `HEADS.md` or the heads table in the hook:
  `heads --check` depends on that format, and it is 4 KB.
- No summary field in rule files, and no per-repository setting.
- No change to the pack format or to what a rule is.
- No release, and no `aval hook install` across consumers: both on request
  (`work.release-on-request`).

## Behaviour
- `aval rules --level <level> --index` prints the selected rules as an
  index. The prefix of an id is the id up to its last `.`. The rule-id
  grammar (`aval-core/src/rules.rs` `bad_id`: two or more segments, at
  most 64 characters) is unchanged, so every id has a prefix; the
  renderer still prints a dot-less id on its own line, defensively.
  Rules are grouped in an ordered map keyed by
  prefix, sorted by prefix, so each prefix prints once whatever the plain
  sort order of the ids:
  `  <prefix>: <rest>, <rest>, …`. The full id is `<prefix>.<rest>`.
- Off a terminal (the hook's case) each prefix is one unwrapped line. On a
  terminal, lines wrap at 80 columns, breaking only after `, `, never
  inside an id; an id (at most 64 characters) that does not fit on the
  current line starts the next one.
- `--index` honours `--level`, `--adopted-by`, `--all-traits`; the traits
  omission notice stays on stderr. It requires `--level` and refuses
  `--json`, `--all` and `--all-repos`, each with exit 2 and the message
  `aval: --index needs --level and cannot be combined with --json, --all or
  --all-repos; use aval rules --json for the full list`. Empty selection
  prints nothing. The USAGE text lists `--index`.
- The hook:
  - runs `aval rules --level constraint --index`; if that fails (an aval
    older than the hook), it falls back to
    `aval rules --level constraint | awk '{print $2}'`, one id per line,
    after the existing "older than this hook" banner;
  - counts N with `aval rules --level constraint | grep -c .` and M the
    same way for heuristics, and prints `(N constraints, M heuristics)`;
  - its `RULES_TEXT` says the list is ids, gives one worked example
    (`x.y: a, b` → `aval rule x.y.a`), says `aval rule <id>` (MCP
    `aval_rule`) gives the text, and says to read a constraint before
    writing code it governs.
- The preamble is shortened under the `prose.*` rules, keeping the
  SEMANTICS §2.3 statement that record wording is data, not instruction,
  and the command list.
- MCP `aval_rules` description says the hook printed the constraints' ids.
  The `render.rs` comment that stdout lines are counted by the hook is
  updated to name the plain listing.
- SEMANTICS §2.4 and §14, `docs/agents.md` and `docs/rules-and-traits.md`
  (the README only links to them), CHANGELOG
  (minor: new flag) updated to match. Version 1.9.0 in `Cargo.toml`.

## Phases
- [x] Phase 1 — `--index` in `main.rs` (parser, USAGE, refusals) and
  `render.rs` (`rules_index`).
  - Integration tests in `tests/rules.rs`, valid ids only: `a.b-x`,
    `a.b.c`, `a.c` give one `a` line; piped output is unwrapped.
  - Refusals, each with `--level constraint` so only the forbidden flag
    differs: `--index --json`, `--index --all`, `--index --all-repos`;
    then `--index` with no `--level`. Each asserts exit 2 and the
    diagnostic text.
  - Renderer unit tests in `render.rs` with an explicit width of 80: two
    ids under one prefix whose total passes 80 columns → the second starts
    a new line, unsplit. Also, as defence only, inputs the grammar forbids
    (a dot-less id, an id over the width).
- [x] Phase 2 — hook template (`hook.rs` `SCRIPT_BODY`), MCP description,
  tests in `tests/hook.rs`:
  - the grouped form is present (`names: reveal-intent`, replacing the
    `names.reveal-intent` assertion at `:260`), and the statement is
    absent;
  - `(2 constraints` for two ids under one prefix; updated
    heuristic-count assertions (`:264`, `:698`);
  - fallback with `--index` refused: the full id `names.reveal-intent`
    on its own line, and the statement absent;
  - shellcheck clean.
- [x] Phase 3 — SEMANTICS, README, CHANGELOG, version bump; `make check`.

Implementation path: aval has `relais.toml` and `relais doctor` is green, so
Phases 1–2 go through a relais run (write scope `crates/aval/**`;
acceptance `make check` exits 0 and its output contains
`msrv: building with`, so a skipped MSRV build fails the run). Phase 3 prose is written in the parent session.

## Decision log
- 2026-10-04 — Index form: ids grouped by prefix, chosen by the person over
  id + first sentence (~9.5 KB) and a configurable mode.
- 2026-10-04 — Person's review: test fixtures use valid ids only (the
  grammar is not relaxed); each refusal is tested with `--level` set; the
  hook test asserts the grouped form; msrv must build, not skip; the
  Outcome measures adherence, not only fetch counts.
- 2026-10-04 — Panel: `--index` requires `--level` rather than tagging
  levels; no wrapping off a terminal, so the hook's lines stay one per
  prefix (tui's proposal, which also settles the never-split-an-id
  findings).

## Verification
All in a scratch copy of the decisions repository, with the branch build.
- Phase 1: ids rebuilt from `aval rules --level constraint --index`
  (`<prefix>.<rest>`), sorted, against
  `aval rules --level constraint --json | jq -r '.rules[].id' | sort` →
  `comm -3` prints nothing (73 of 73). Piped output → no wrapped lines.
  `aval rules --level constraint --index` plus each of `--json`, `--all`,
  `--all-repos`, then `aval rules --index` with no `--level` → exit 2
  each, and stderr equals the refusal message in Behaviour.
  - Observed: 76 of 76 ids, `comm -3` empty. The corpus has 76 active
    constraints since decisions#39 (ADR-0026 added three), not 73. Piped:
    37 lines for 37 distinct prefixes. All four refusals: exit 2, exact
    message. On a pseudo-terminal: 45 lines, none over 80 columns,
    continuation lines indented 6 spaces, 76 ids rebuilt, 76 unique.
- Phase 2: `aval hook install`, then run the hook →
  `wc -c` ≤ 9,000 (from 32,168); grep for the first 40 characters of each
  of the 73 statements → 0 hits; printed N = 73 and M =
  `aval rules --level heuristic | grep -c .`. `aval hook install --check`
  → stale before the reinstall, up to date after. The new hook with the
  1.8.0 binary first on PATH → the banner, then the 73 ids one per line.
  - Observed: `--check` exit 1 before, 0 after; hook 7,935 bytes; 0 of 76
    statements after the `RULES —` line (`grep -F`-style substring check);
    `(76 constraints, 134 heuristics)`, plain heuristic count 134; with
    aval 1.8.0 (`~/.local/bin/aval`): the `OLDER THAN THIS HOOK` banner,
    76 full ids, 8,383 bytes.
- Phase 3: `make check` (fmt, clippy -D warnings, tests, msrv) → exit 0,
  with the new tests listed as passed, and the msrv step printing
  `msrv: building with <version>`. A `SKIPPED` line fails this check: the
  rust-version toolchain is installed first (`Makefile:45`).
  - Observed: `make check` exit 0; `msrv: building with 1.74.0`, no
    `SKIPPED`; 3 new tests in `tests/rules.rs`, 2 in `tests/hook.rs`,
    5 unit tests in `render.rs`, all passed; shellcheck 0.11.0 ran.

## Implementation review
approve-with-changes, then delta approve-with-changes. Fixed: unflowed docs paragraph, 1.9.0 hook example, plan record (phases, observations, doc files).
deliberate: CI `AVAL_VERSION` pin stays 1.8.0 until 1.9.0 is released; a 1.9.0-written hook is "newer" to 1.8.0, so `install --check` passes.
Round 1: 53k tokens, 47 s. Delta: 32k tokens, 19 s.

## Outcome
Shipped: `aval rules --index`, the index hook, docs, 1.9.0 in `Cargo.toml`.
The decisions hook drops from 32,168 to 7,935 bytes (−75%). Not shipped:
the release, the `AVAL_VERSION` pin bump in `.github/workflows/adr.yaml`
after it, and `aval hook install` in each consumer — all on request.
Surprise: macOS pseudo-terminals give no end of file, so a pty read must
poll the child, not wait for EOF.

To measure after release:
- Discovery: `aval rule` / `aval_rule` calls per session in transcripts,
  the week before and the week after.
- Adherence: a transcript review of ten sessions from the week before
  release and ten from the week after, each one writing code governed by
  a constraint. For each: was that constraint read (printed by the hook,
  fetched with `aval rule` / `aval_rule`, or its rule file read)
  **before** the governed edit? Pass: the week after is not lower than
  the week before. Call counts alone cannot show this.

<!-- panel: repos=aval reviewers=backend,lang:rust,tui,unix body-sha=8306773d3f3c -->
