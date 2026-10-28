---
status: done
branch: feat/in-scopes
repos: [aval, decisions, relais, dotfiles, activity-vision, application-landscape, customer-vision, duro-app, kb-vision, social-planner, website-builder, duro-design-system]
adrs: [ADR-0043, ADR-0039]
---
# A repository declares the scopes it is in

## Review panel

📄 Full reviews: [2026-10-27-repo-declares-its-scopes.reviews.md](2026-10-27-repo-declares-its-scopes.reviews.md)
👉 **Decide:** none — approve if repo-declared `in_scopes` (your pick) should make every aval caller answer at the repository's stack scopes, exit 5 when two declared scopes both answer one key.
📍 aval, decisions, relais, dotfiles + 8 fleet `.adr.yaml` lines · planned · next: aval 1.11.0. Panel: architect, backend, po, lang:rust, unix (opus-5-5), codex. The 8 fleet repos get one registry line each, so their UI/TS seats were not run.
**Changed by review:** `relevant` stops dropping scoped keys; only own-slot answers count, distinct entries contradict; contradiction keeps `heads` + `by_scope`.
**Verdicts:** 5 approve-with-changes (round 1); backend approve after 3 re-binds; codex approve-with-changes; 9 findings carried into implementation.

## Context

The fleet's stack decisions are scoped: `stack.*` at `effect-stack`
(ADR-0001–0005, "Effect is the effects runtime", "@effect/sql not Kysely", …)
and `ui.*` at `duro-stack` (ADR-0006, 0007, 0016, 0027, "Duro is the design
system", "every measure is a token", …). None of those keys has a default-scope
(`*`) entry, and nothing records which repository is on which stack. Every
caller asks at `*`: relais (`context/guidance.rs` `relevant_args`,
`decisions_args`, `rules_args`), amont-agent `review-brief`
(`review_brief.rs:531,562`), the session hook, the plan-review and interface
skills. So in customer-vision, duro-app or kb-vision those keys resolve
`undecided`, relais drops them, and no worker or reviewer is told the stack
decisions at all — the decision layer of ADR-0043 is blind exactly where the
fleet's oldest decisions live. The only scoped calls today are hand-typed
(`application-landscape/CLAUDE.md:22`: `--scope duro-stack`).

The person chose the model (2026-10-27): **a repository declares the scopes it
is in**, and aval answers at those scopes by default, so every caller gets
scoped answers with no per-tool change.

Also recorded by this plan (done 2026-10-27, before it): the **override check**
of the code-rules plan. One patch (`fixture_formula.rs` with `n`, `s`, `d`)
reviewed twice by relais 0.34: without a decision, `names.reveal-intent`
flagged `n`, `s`, `d`; with a scratch-branch decision ADR-0012
`code.formula-notation` overriding the rule for that file, all four candidates
were ruled `fine` citing the decision's precedence, and no naming finding was
raised. Scratch branches never pushed; runs abandoned; candidate fed back
`reverted`.

## Behaviour

- **New consumer-local registry field** `in_scopes: [effect-stack, duro-stack]`
  in `.adr.yaml`. Each name must be a scope of the vocabulary in force
  (own `scopes` ∪ vendored packs' scopes) and never `*`; else a Layer A error
  `in-scopes-declared`. Like `areas`, it is consumer-local: never published in a
  pack, and a pack carrying it is refused (`pack-parses`). Absent = today's
  behaviour exactly.
- **Precedence**, stated once in SEMANTICS §5: the `--scope` flag, then
  `.adr.yaml` `in_scopes`, then `*`. JSON answers (CLI and MCP) carry
  `scope_source: flag|in_scopes|default`.
- **Default query scope.** When `resolve` or `relevant` (and MCP
  `aval_resolve`, `aval_relevant`) get no `--scope`, a key is resolved at each
  declared scope that admits it (§2.1). Only scopes that answer **from their
  own slot** count — a scope that merely inherits the `*` head, or is
  undecided, does not (a key without a `scopes` restriction admits every
  scope, `model.rs:390`, so this is what keeps a `*` default plus one scoped
  override from reading as a disagreement):
  - no declared scope answers from its own slot → resolve at `*`, as today;
  - exactly one answering entry → that answer;
  - several distinct answering entries → `contradiction` (exit 5), even if
    their ADR IDs or choice text match: entry identity includes scope (§1.4).
    Do not deduplicate entries by ADR ID before determining the verdict. The existing
    `Verdict::Contradiction` gains `by_scope: [{scope, adr}]`, `heads` is the
    union of those ADRs (relais counts `heads`, `context/mod.rs:102-108`),
    `matched_scope` is null. SEMANTICS §5 says the remedy is the repository's
    `in_scopes` (narrow it) or deciding the key at one scope — not the corpus.
  An explicit `--scope S` (including `--scope '*'`) behaves exactly as today.
- `history` with no `--scope` prints one chain per declared scope that admits
  the key, labelled, exit 0 (history is not authority); `--all-repos` uses
  each repository's own `in_scopes`.
- `relevant`: with no flag, the ranking keeps a key that `*` or any declared
  scope admits (today `relevant.rs:525` drops every scope-restricted key
  before resolving); each ranked key carries `scope` (the slot that answered);
  with `in_scopes`, the top-level `scope` is the list.
- `aval check` warns when more than one declared scope answers one key from
  its own slot (the same condition as exit 5, identical choice text included),
  so a repository's PR catches it before a query does; a conformance case
  covers identical choice text.
- `aval traits --summary` (the session hook's line) also names the declared
  scopes, so a session sees "this repository is on effect-stack, duro-stack".
- `rules` is unchanged (rules are not scoped; trait filtering stays the
  mechanism for applicability).

## Phases

### [x] Phase 1 — aval 1.11.0 (repo `aval`)

- `crates/aval-core/src/parse.rs`: add `in_scopes` to `REGISTRY_FIELDS`
  (`:20-32`), parse like `scopes` (`:682`, reject `*`); `model.rs` Registry gains
  `in_scopes: Vec<String>` (consumer-local, beside `areas` `:433`).
- `crates/aval-core/src/graph.rs`: Layer A `in-scopes-declared` beside
  `scope-declared` (`:474`), against the merged vocabulary (`load.rs:660-682`
  `merge_declarations`); a `resolve_default(key)` beside `resolve` (`:255`)
  implementing the rule above; `Verdict::Contradiction` gains `by_scope`
  (token and exit 5 unchanged, existing matches untouched); the `check`
  warning.
- `crates/aval-core/src/pack.rs`: refuse `in_scopes` in a pack (`:379-390`
  pattern); `PACK_FIELDS` unchanged.
- `crates/aval/src/main.rs` (`resolve` `:458`, `history` `:1515`, `relevant`
  `:506`, and the `--all-repos` branches `:447-452`, `:1515-1520`),
  `relevant.rs` (`Query.scope` becomes `Option<String>`, the admits filter
  `:525`, `verdict_of` `:629`, JSON `:690`, `:796`), `mcp.rs` (`:833`, `:924`,
  `:1060`): the missing scope stays `None` down to the graph and means
  `resolve_default`; `--scope S` unchanged. One test each for CLI, MCP and
  relevant showing that no flag differs from `'*'`.
- `traits.rs` summary line names `in_scopes`.
- SEMANTICS §2 (registry fields), §2.1, §5 (default query scope), §2.5
  (consumer-local fields), exit codes; `docs/commands.md`, `docs/agents.md`.
- Conformance corpus `conformance/corpora/in-scopes` + `resolve.json` cases:
  declared scope answers; a scope the key does not admit is skipped; fallback
  to `*`; `*` default plus one scoped override → the override, exit 0; a
  decision at one declared scope only → that decision, exit 0; two declared
  scopes with identical choice text → exit 5; one ADR deciding the same key
  at two scopes with different choices → exit 5, retaining both scoped entries
  in `by_scope` even though `heads` contains one ADR; two ADRs disagreeing →
  exit 5 with `by_scope` and `heads` (JSON-shape cases); explicit `--scope`
  unchanged; absent field unchanged;
  `relevant` with no flag ranks a key admitted only at a declared scope;
  `--all-repos`. `tests/conformance.rs` `run_resolve` calls `resolve_default`
  when `query.scope` is absent, and explicit-`*` cases carry `"scope": "*"`
  (today `:53-54` turns absent into `*`, so the cases would pass without the
  feature).
- Release 1.11.0 (`tag-release`), install.
- Same PR: record the override check in the code-rules plan's Outcome
  (`docs/plans/2026-10-23-code-rules-reach-writers-reviewers.md`).

### [x] Phase 2 — decisions ADR-0046 (repo `decisions`)

- ADR-0046 `guidance.scope-membership`: a repository declares the scopes it is
  in with `in_scopes`, and aval answers at them by default; who sets it (the
  repository, reviewed like any `.adr.yaml` change); relation to ADR-0043 (the
  decisions block now carries stack decisions) and §2.1's axis warning (the
  fleet scopes are stack families; a repo's local `scopes:` vocabulary is a
  different axis and stays; `in_scopes` names stack-family scopes only, and a
  reviewer refuses a local lane/mode scope there, since `in-scopes-declared`
  cannot tell the axes apart). Key declared in `.adr.yaml`.
- CI pin `AVAL_VERSION` → 1.11.0 first; `aval pack --write`, HEADS, hook.

### [x] Phase 3 — fleet declarations (one PR per repository)

`in_scopes` per the dependency survey (root `package.json`):

| repo | in_scopes |
|---|---|
| activity-vision | [effect-stack] |
| application-landscape | [effect-stack, duro-stack] |
| customer-vision | [effect-stack, duro-stack] |
| duro-app | [effect-stack, duro-stack] |
| kb-vision | [effect-stack, duro-stack] |
| social-planner | [effect-stack, duro-stack] |
| website-builder | [duro-stack] |
| duro-design-system | [duro-stack] |

Before the first PR: `aval --version` is 1.11.0 for every caller (PATH,
relais, amont-agent, baked hooks) and the aval MCP servers are restarted — an
older aval refuses `in_scopes` (unknown registry field, exit 3). Rollback
order: revert these lines before any aval downgrade.

customer-vision lands first, and the relais and review-brief checks in
Verification run on it before the other seven open. Each PR: bump the repo's
aval pin (CI and hook) to 1.11.0, refresh the already-vendored decisions pack,
add the line, drop any hand-typed `--scope` (application-landscape
`CLAUDE.md:22`), `aval check`, and record before/after of
`aval resolve ui.design-system` / `stack.effects-runtime`. A one-line change per
repository: no pointer plan, no implementation review (ceremony scales with
size). `monorepo` / `single-package` are not declared: no record decides at them.

### [x] Phase 4 — callers say where the scope comes from (relais, dotfiles)

- relais: the reviewer's decisions block renders the answering scope
  (`key (record, scope): choice`); the `holds-until:` at
  `runner/review/mod.rs` (default-scope only) is removed — its condition is met;
  SPEC §7 says aval answers at the repository's declared scopes. Tests for the
  rendering. No release of its own (rides the next relais release).
- dotfiles: the interface/layout/writing/accessibility/interface-review skills'
  "resolved at its applicable scope" and worktree-task step 2b's "at the diff's
  scope" say: `aval resolve <key>` answers at the repository's declared
  `in_scopes`; no hand-typed `--scope`. Patch the live `~/.claude` copies (the
  laptop is the source).

## Non-goals

- Path-level scopes (`areas` mapping paths to scopes, for monorepos with mixed
  stacks).
- Scoped rules (`rules --scope`); rule applicability stays traits.
- Detecting scopes from dependencies (`aval traits --detect` style).
- Deciding `packaging.package-manager` or anything at `monorepo` /
  `single-package`.

## Verification

- aval: `cargo test`, conformance suite green; in a scratch copy of
  customer-vision with `in_scopes: [effect-stack, duro-stack]`:
  `aval resolve ui.design-system` (no flag) → before `undecided` (exit 4),
  after `active` ADR-0006 `matched_scope: duro-stack`; `aval resolve
  stack.effects-runtime` → active ADR-0001 at effect-stack; `--scope '*'` →
  `undecided` as before; a fixture with two declared scopes deciding one key
  differently → exit 5 naming both; a pack carrying `in_scopes` → refused.
- decisions: `aval check`, `heads --check`, `pack --check`, `hook install
  --check` all exit 0 on 1.11.0.
- Fleet: per repo, `aval check` exit 0 and the two resolves above answer
  `active`; CI green.
- relais: `cargo test`; a relais review run in customer-vision (isolated, a
  trivial UI patch) → the reviewer prompt's decisions block lists
  `ui.design-system (ADR-0006, duro-stack)`. `amont-agent review-brief` in the
  same repo lists ADR-0006 resolved active.
- dotfiles: the skill texts name `in_scopes`; live copies match.
- relais: a fixture where two declared scopes disagree → the blocked message
  names both heads and both scopes (not "0 heads").

## Measure after release

- Share of relais reviews in the 8 declared repositories whose decisions block
  names a `ui.*`/`stack.*` key: 0 before, target 100% within 7 days.
- Unflagged `contradiction` (exit 5) verdicts fleet-wide: target 0. One that is
  not a real corpus disagreement → revert that repository's `in_scopes` line and
  fix aval before redeclaring.

## Verification record

Phase 1 (aval, branch binary; `cargo test` green through the pre-commit gate):
- customer-vision's corpus copied to a scratch dir, `aval resolve
  ui.design-system` → undecided before `in_scopes` → exit 4, undecided.
- Same with `in_scopes: [effect-stack, duro-stack]` → active ADR-0006 at
  duro-stack → active decisions:ADR-0006, `matched_scope: duro-stack`,
  `scope_source: in_scopes`.
- `aval resolve stack.effects-runtime` → active ADR-0001 at effect-stack →
  active decisions:ADR-0001, scope effect-stack.
- `--scope '*'` → unchanged → undecided, exit 4.
- `aval check` → clean → 41 records, no findings.
- `relevant --json` → top-level `scope` stays a string, `scopes` lists the
  declared scopes, scoped keys ranked → `scope "*"`, `scopes [effect-stack,
  duro-stack]`, `stack.sql-layer` active at effect-stack.
- `traits --summary` → names the scopes → "scopes here: effect-stack,
  duro-stack (aval resolve answers at them)".
- Two declared scopes deciding one key differently → exit 5 naming both →
  conformance case `in_scopes_two_records_disagreeing_contradict` passes
  (`by_scope` effect-stack/ADR-0009, duro-stack/ADR-0010).
- A pack carrying `in_scopes` → refused → test
  `a_pack_carrying_in_scopes_is_refused` passes (exit 1, "a pack carries no
  `in_scopes`").
- A declared scope's own contradiction → `scope_source: in_scopes` → test
  `a_declared_scopes_own_contradiction_is_sourced_from_in_scopes` passes, and
  fails with the `render.rs` arm removed (mutation run).

## Decision log

- 2026-10-27 — Phase 1 ran as relais tasks: 1A stopped `scope_exceeded` on
  the conformance corpus `.adr.yaml` (protected configuration) and was
  salvaged by hand; 1B and two review-fix rounds; the orchestrator added the
  tests the workers skipped and renamed single-letter names the reviewers
  caught (`names.reveal-intent`).
- 2026-10-27 — aval's own CI pinned 1.9.0, which cannot read `governs` in the
  refreshed pack; pinned 1.10.0 (reviewed as a Delta). The pin moves to 1.11.0
  before aval's own `.adr.yaml` ever declares `in_scopes`.
- 2026-10-27 — the pin bump also needed the session hook regenerated by 1.10.0
  (`hook install --check`); reviewed as a Delta. Pin and hook header move to
  1.11.0 together after the release.

## Implementation review

- aval (Phase 1): round 1 approve-with-changes (105k, 145 s) — 3 blocking
  rule findings (banners, an output-parameter helper, one-letter names), all
  fixed; Delta approve-with-changes (117k, 35 s), its two lows done here.
- Next: release aval 1.11.0, then Phase 2 (decisions ADR-0046).

## Outcome

Shipped 2026-10-27/28:
- Phase 1 — aval 1.11.0 (aval#46, release aval#47), installed.
- Phase 2 — decisions ADR-0046 `guidance.scope-membership` (decisions#102),
  CI pinned to 1.11.0.
- Phase 3 — `in_scopes` declared in all eight repositories, each with its aval
  pin moved to 1.11.0, pack and hook refreshed: customer-vision#26 first
  (Forgejo), then activity-vision#9, application-landscape#334, kb-vision#53,
  social-planner#63, website-builder#224 (Forgejo), duro-app#155,
  duro-design-system#149 (GitHub).
- Phase 4 — dotfiles#51 (skill and agent texts), relais#323 (decisions block names
  the scope, cross-scope contradiction keeps heads and `by_scope`, guidance
  budget fix).

Fleet record (aval 1.11.0, before → after, all eight before were `undecided`
on both keys):

| repo | in_scopes | ui.design-system | stack.effects-runtime |
|---|---|---|---|
| activity-vision | effect-stack | undecided (correct) | active ADR-0001 |
| application-landscape | effect-stack, duro-stack | active ADR-0006 | active ADR-0001 |
| customer-vision | effect-stack, duro-stack | active ADR-0006 | active ADR-0001 |
| duro-app | effect-stack, duro-stack | active ADR-0006 | active ADR-0001 |
| kb-vision | effect-stack, duro-stack | active ADR-0006 | active ADR-0001 |
| social-planner | effect-stack, duro-stack | active ADR-0006 | active ADR-0001 |
| website-builder | duro-stack | active ADR-0006 | undecided (correct) |
| duro-design-system | duro-stack | active ADR-0006 | undecided (correct) |

`aval check` clean in each; CI green in each (application-landscape's first CI
run failed on a relay integration test that timed out under load and passes
alone; the re-run was green). In customer-vision, `aval relevant` for a UI file
ranks `ui.design-system`, `ui.measures`, `ui.policy-enforcement`,
`ui.mockup-handoff` (duro-stack) and `stack.web-framework` (effect-stack)
active, and `amont-agent review-brief` on a scratch UI change carries
ADR-0006 at duro-stack. application-landscape's hand-typed `--scope
duro-stack` (`CLAUDE.md`) is gone.

Found during Phase 4 and fixed in relais: the worker's `aval relevant --top 20`
(ADR-0043 work) pushed the ranked block past the 16 KiB guidance budget, and
`fit_budget` dropped it whole, so a worker got no decisions at all. Whole
entries now leave from the end, rules before keys, named in a `relevant
partial` gap; worker lines name the answering scope.

Not observed yet:
- The relais end-to-end review in customer-vision (decisions block naming
  `ui.design-system (ADR-0006, duro-stack)`): two runs waited out the
  30-minute lease budget while other sessions held the heavy slots, and the
  rendering needs the next relais release. Runs after that release.
- The aval MCP server of a session started before 1.11.0 still runs the old
  binary and refuses a repository with `in_scopes`; a new session picks it up.
- Measure after release (above): due about 2026-11-04.

<!-- panel: repos=aval,decisions,relais,dotfiles reviewers=architect,backend,po,lang:rust,unix,codex body-sha=62783cdbc00f -->
