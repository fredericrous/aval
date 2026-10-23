---
status: active
branch: feat/governs-full
repos: [aval, decisions, relais, dotfiles]
adrs: [ADR-0026, ADR-0039, ADR-0042]
---
# Code rules reach the agent that writes and the agent that reviews

## Review panel

📄 Full reviews: [2026-10-23-code-rules-reach-writers-reviewers.reviews.md](2026-10-23-code-rules-reach-writers-reviewers.reviews.md)
👉 **Decide:** none — approve if pushing the full code rules into writer and reviewer prompts (~14k tokens per worker) is worth stopping violations like single-letter names.
📍 decisions, aval, relais, dotfiles · planned, nothing built · next: aval `governs` + `--full`. Panel: backend (opus-5-5), lang:rust (opus-5-5), tui (opus-5-5), unix (opus-5-5), codex.
**Changed by review:** one `--full --json` spawn split on `governs`; fallback keeps all ids on an old aval or stale pack; deletions and scoped overrides count in review.
**Verdicts:** 4 approve-with-changes in round 1; backend approve-with-changes ×4 in round 2, one blocker fixed; codex approve-with-changes; 9 findings carried into implementation.

## Context

`names.reveal-intent` is an active constraint in every fleet repo ("no
unexplained single-letter names … `i` only in a tiny, conventional
scope"), yet relais alone carries roughly 800 single-letter bindings
(`const r = store.router` ×8 in `claude-plugin/hooks/routing.ts`,
`let a`/`let b` in `crates/relais/src/money.rs`). The rule is decided; it
never reaches the place where code is written or judged:

- **Worker** (relais `runner/prompt.rs:143-153`): gets constraint *ids*
  plus "run `aval rule <id>`" through Bash. Pull is optional and evidently
  skipped.
- **relais reviewer** (`runner/review/mod.rs:481-511`): gets each
  constraint's *statement* only — the first paragraph. For
  `names.reveal-intent` the single-letter clause is in the body, so the
  reviewer never sees it. Its prompt (`review/engine.rs:306-381`) frames
  findings against acceptance criteria, not against rules.
- **implementation-review at push** (`~/.claude/agents/implementation-review.md`,
  brief from `~/.claude/skills/worktree-task/SKILL.md:183`): same — the
  brief carries `aval rules --level constraint`, one line per rule.
- No gate checks code against rules: `aval check` checks the corpus only.

### Decisions taken (person said "decide for me" on 1)

1. **Push the full text; MCP as the pull channel.** Measured on this
   machine since 2026-10-01: 194 worker transcripts carried the "run
   `aval rule <id>`" instruction and **0** ran it; the ~20 workers that had
   the aval MCP tools listed made 7 aval calls. A typed tool gets used where
   a Bash instruction does not, but pull stays rare either way. So the
   rules that govern code are pushed in full (statement + body) into the
   worker and reviewer prompts, and workers also get the read-only aval MCP
   tools (`aval_rule`, `aval_resolve`, `aval_relevant`) for everything not
   pushed (process rules stay an id index).
2. **No LLM inside aval, no LLM gate per commit.** aval's spec forbids a
   model (SEMANTICS §12.1 "no model", §5.1 "no network"); amont gates are
   deterministic commands, and a per-commit model call costs tokens and
   latency on every relais attempt. The LLM review with the rules already
   has two seats — the relais reviewer (every relais run) and
   implementation-review (every planned push). They get the full code
   rules and block on a violation. aval's part is deterministic: say which
   rules govern code and serve their full text.

## Approach

### Phase 1 — decisions: mark which rules govern code (repo `decisions`)

- New ADR (next number) "A reviewer reads the full text of the rules that
  govern code": rule files gain `governs: code`; such constraints are
  pushed in full to writers and reviewers, and a violation in a diff is a
  blocking finding citing the rule id. It states its relation to ADR-0026
  (`guidance.session-load`, `guidance.always-on-is-an-index`: session text
  is an index, detail fetched on demand): that rule keeps governing hooks,
  `CLAUDE.md` and `AGENTS.md`; a task dispatch to a writer or reviewer is
  not session text, and on-demand fetch was measured to fail there (0 of
  194 workers ran `aval rule`). Catches line (ADR-0042): "a decided
  code rule broken in a diff that a reviewer passed".
- Add `governs: code` to the frontmatter of `clean-code.md`,
  `functional.md`, `clean-architecture.md`, `reliability.md`, `tests.md`,
  `refactoring.md`, `change.md`, `cli.md` (keeps `applies: [cli]`).
  Lands after aval Phase 2 is released (unknown field is a Layer A error) and after each repo's aval pin (hooks, CI) is bumped before its pack refresh.
- `aval pack --write`, `aval check`, regenerate HEADS.

### Phase 2 — aval: `governs` and full text (repo `aval`)

- `crates/aval-core/src/rules.rs`: accept `governs` in `FILE_FIELDS`
  (values: `code`; absent = not code); `model.rs:153` `Rule.governs`.
  Pack serialisation carries it (`aval pack`/`aval add`).
- `aval rules --governs code` filter; `--full` adds the body (text: rule
  as `aval rule` prints it, rules separated by a blank line and a `---`
  line; JSON: `body` and `governs` fields on every rule). `--full` with `--index` is refused (E_USAGE,
  message names `aval rules --full`); plain `aval rules` stdout stays one
  line per rule (the session hook counts it). Same params on MCP
  `aval_rules` (`crates/aval/src/mcp.rs`). Accept `--governs=code` as well;
  an unknown `--governs` value and `--full` with `--index` are usage errors
  (exit 2, message on stderr). Update the usage and flag help in `main.rs`
  (:63-68, :100-106).
- Still applies the trait filter, so `cli.*` appears only where the repo
  declares `cli`.
- SEMANTICS §2.4 + `docs/rules-and-traits.md`, conformance corpus
  `conformance/corpora/rules-sample` gains a `governs: code` file.
- Release via `tag-release`; `aval add` refresh in each repo vendoring the
  `decisions` pack (relais `.adr/packs/decisions.pack`, …) after Phase 1.

### Phase 3 — relais: workers and reviewer get the code rules (repo `relais`)

- `context/guidance.rs`: `rules_args` becomes `aval rules --level constraint --full --json`
  (one spawn); `code_rules` are those with `governs: code`, the rest feed
  the index. New `#[serde(default)] Guidance.code_rules` records ids +
  body hash in the manifest (full text only in the prompt). Fallback, so a
  fetch problem never removes a rule from the prompt — every case keeps all
  constraint ids in the index and records a gap that names the fix:
  exit 2 (`E_USAGE`, old aval) → retry the current argv, gap "aval lacks
  `--full`/`governs`, needs ≥ <version>: `brew upgrade aval`"; exit 1/3 or
  timeout → gap as today; exit 0 with no `governs: code` rule while rules
  exist (pack not yet refreshed) → gap naming the stale pack.
- Worker prompt (`runner/prompt.rs`): a fenced "code rules in force" block
  with full text, placed before `[relais task]` (prompt.rs:12) as a stable
  prefix; the cache saving stays unknown until the Cost measurement settles it; the id index keeps only the non-code constraints. Own budget
  (64 KiB; measured 55 KB for relais's 52 code constraints); over budget →
  whole rules dropped, via `fit_statements`, in a stated order, with a gap naming them, never cut mid-rule.
- Reviewer (`runner/review/mod.rs` `reviewer_guidance`,
  `REVIEW_CONSTRAINTS_BUDGET_BYTES` → 96 KiB): full text of code rules,
  statements for the rest; `review/engine.rs` prompt adds: "a code-rule
  violation introduced by this diff, including deletions, is a finding:
  cite the rule id and `file:line` (use the old-side location for a deletion);
  it blocks like an unmet acceptance criterion. Do not report unchanged
  pre-existing violations.", and the
  preamble at `engine.rs:315` changes "the violated acceptance criterion" to
  "the violated acceptance criterion or code rule id" (prompt test asserts it).
- Worker agents (`claude-plugin/agents/relais-worker-*.md`, `tools:`):
  add `mcp__aval__aval_rule`, `mcp__aval__aval_resolve`,
  `mcp__aval__aval_relevant`; the index instruction names the MCP tool
  first, the CLI as fallback. First confirm how Claude Code resolves an
  `mcp__*` entry in a subagent's `tools:` list when the parent session has
  the aval server (claude-code-guide); if the server is absent the worker
  keeps the CLI line. The aval MCP root is the parent session's cwd (`mcp.rs:65`), not the task worktree: the instruction passes `repo: <task repo>`, and Verification runs a worker whose repo differs from the session's.
- Preserve aval SEMANTICS §2.4 precedence in the worker prompt and both
  review channels: applicable scoped decision, inherited default-scope
  decision, adopted rule, explanatory source. Carry applicable resolved
  decisions and their scope with the rules (source: `aval relevant --json --top <n>` with one `--path` per file of the
  committed task diff (`git diff --name-only <base>...HEAD`; `--changed` sees only
  uncommitted files and is empty after the attempt commits) for reviewers, `--path`
  over the task's declared paths for the worker; each returned key then goes
  through `aval resolve`, and the resolved verdict with its scope is what is carried,
  since `relevant` is a suggestion, never a verdict; inside the code-rules budget, dropped whole with a gap like rules); reviewers must check this
  evidence before declaring a rule violation. Missing decision evidence
  is an uncertainty to report, not permission to invent an exception.
  Verify a patch covered by an explicit overriding decision produces no
  finding against the overridden rule.
- SPEC §7 (`docs/SPEC.md:282-284`) updated to match.
- Tests: extend the guidance tests in `context/guidance.rs` and the
  worker/reviewer prompt tests in `runner/tests/mod.rs` (≈6704-6880,
  8077): full text present, process rules stay ids, budget drops whole
  rules with a gap, old-aval fallback; the JSON fixture is captured from
  the released aval.

### Phase 4 — implementation-review gets the same text (chezmoi-managed `~/.claude`)

- `skills/worktree-task/SKILL.md:183`: brief carries `aval rules --level
  constraint --governs code --full --json` plus `aval rules --level constraint`
  for the rest, plus the applicable decisions: `aval relevant --json --top <n>` with one
  `--path` per file of `git diff --name-only origin/main...HEAD` (not `--changed`,
  which is empty on a committed branch), each key resolved with `aval resolve`.
- `agents/implementation-review.md`: a code-rule violation introduced by
  the diff, including deletions, is a blocking finding with rule id and
  `file:line` (old-side location for a deletion); unchanged pre-existing
  violations are outside this check. Verify both review channels with a
  deletion-only patch that removes a required timeout. The finding blocks (verdict `rework` or blocking `approve-with-changes`, as the ADR states). Edit in the chezmoi source, apply only those two
  files.

## Non-goals

- Renaming relais's existing single-letter names (follow-up: relais tasks
  per module, once reviewers block on new ones).
- A mechanical naming lint (`clippy::min_ident_chars`, oxlint `id-length`)
  — follow-up plan; this plan fixes the channel all rules travel through.
- An LLM call inside aval or inside an amont commit gate.

## Verification

- aval: `cargo test`, conformance suite; `aval rules --level constraint
  --governs code --full --json` in relais lists 52 rules with bodies;
  in a repo without `cli` trait, no `cli.*`.
- relais: `cargo test`, `make plugin`. Then a live check: a relais run in a
  scratch branch asked to add a helper; inspect the dispatched worker
  prompt (manifest/receipt) for the `names.reveal-intent` body. Seed a
  patch with `const r = store.router` through a relais review and confirm
  a finding citing `names.reveal-intent` and a non-pass outcome.
- Negative control: a clean patch through relais review → no finding citing a code rule; over the 20 runs after release, repairs caused by rule findings counted.
- implementation-review: run on a branch adding a single-letter binding;
  verdict cites the rule.
- Pull rate: re-run the transcript count (worker `tool_use` of
  `mcp__aval__*` vs Bash `aval rule`) over the first 50 workers after
  release; baseline 0 Bash / 7 MCP; expected: at least 1 MCP call per 10 workers.
- Cost: compare relais ledger cost per run for the 20 runs before and
  after (uncached: 55 KB/4 ≈ 14k tokens per worker attempt, plus ≈24k per reviewer; expected cost increase ≤ 15% per run; check `cache_read_input_tokens` on the second worker in one repo); record in the ADR's
  revisit note.

<!-- panel: repos=decisions,aval,relais,dotfiles reviewers=backend,lang:rust,tui,unix,codex body-sha=adee1394a250 -->
