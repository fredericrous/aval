# Changelog

## Unreleased

## v1.3.2

### Fixed

- **A closed stdout ends the process quietly.** Rust starts with SIGPIPE
  ignored, so `aval add … | head -1` answered the reader leaving with a panic
  and forty lines of backtrace. Every print in the binary now goes through one
  writer: a closed pipe ends the process with status 141, the code the shell
  would have shown had the kernel done it, and any other write failure is a
  tool failure that says so. (Restoring the signal's default disposition is
  the usual cure; it needs an `unsafe` call this workspace forbids.)

## v1.3.1

Five defects, found by a review that reproduced each against a temporary
repository. None changes a verdict for a corpus that loads; two of them changed
verdicts for corpora that should not have loaded at all.

### Fixed

- **An unreadable records directory is an error, not an absence.** With
  `sources` or `packs` declared, the loader tolerated a missing `dir`; it
  tolerated every other read error too. A directory whose permissions were
  removed dropped every numbered record from the graph, whatever they
  superseded came back as heads, and `resolve` answered `active` with a
  replaced decision — exit 0. Only "not found" is the allowance now; anything
  else is exit 3 naming the directory.
- **Two sources landing on one pack name are refused before anything is
  written.** `aval add /a/fleet /b/fleet` planned both as `new`, wrote the
  same path twice, and the second silently replaced the first. The command now
  stops with both sources named and asks for `--as`.
- **An inline `packs:` list is edited as a list.** `packs: []`, the shape a
  fresh consumer starts with, was read as a block-list heading and an
  indented item was appended under it: `add` exited 0 and every later read
  exited 3 with "unexpected indentation". An inline list stays inline and
  gains its item; a block list stays a block; a scalar is refused.
- **A revision may contain slashes.** `repo@feature/test` was read as a path
  called `repo@feature/test` at HEAD, because the `@` was looked for only past
  the last `/`. The last `@` now separates when what precedes it is a
  complete source and what follows it can be a ref; userinfo in an ssh or
  https URL still is not a revision, and a spec that exists on disk as written
  is a path.
- **A full commit id resolves to itself.** `ls-remote` takes its last argument
  as a ref pattern, so pinning the very commit id the ambiguity error told
  you to use matched nothing. Forty (or sixty-four) hex digits now name
  themselves; whether the remote hands that commit over is `fetch`'s question.

## v1.3.0

A vendored pack is now `.adr/packs/<name>.pack`, and whether it is still the
pack the producer published is answered by comparing **declarations** rather
than bytes.

Both follow from the same mistake. A vendored pack lives in repositories whose
pre-commit hook runs a formatter over every YAML file, and the first pack
carrying a quoted word was rewritten by prettier in three consumers in one
afternoon. 1.2.1 answered by emitting the quoting prettier would choose —
which is the wrong layer, because a second formatter would disagree, and it
defended a property that was never the point. What matters is that the
reviewer reads in the pull request what the model receives, and that the
vendored file declares what the producer published. Neither needs bytes. So
the file takes an extension no formatter claims, the one the producer's own
`aval.pack` already has, and equality is declarational the same way §12.2's
freshness already was for `HEADS.md`.

**Nothing to do.** The next `aval add` in each consumer moves the `.yaml` to
`.pack` and rewrites the one registry line that names it; until then, the
`.yaml` keeps loading, because a `packs:` entry is a literal path and nothing
reads the extension. A repository that put `.adr/packs/` in its
`.prettierignore` can drop the entry once that run has happened.

### Added

- **`.adr/packs/<name>.pack`.** `aval add` writes it, and migrates a
  same-source `<name>.yaml` written by an earlier version: the file moves, the
  registry line moves with it — textually, so every comment and every other
  entry stays as it was written — and `--dry-run` reports the move without
  making it. A `.yaml` carrying another source's banner, or no banner, is
  still a collision and still stops the command.
- **`edited`, a standing of `aval add --check`.** The revision still names the
  recorded commit, and the vendored file no longer declares what that commit
  published. Re-resolving alone never asked that, and what it missed is the
  failure this tool exists to prevent, seen from the other side: a decision
  this repository believes another one made, that nobody made. Counted with
  `behind` (exit 1); `aval add` restores the file. Asked only of a pack that
  is otherwise current — a behind one is being replaced anyway, an unknown one
  cannot be fetched to compare against.
- **`aval_core::yaml::same_values`.** Whether two documents declare the same
  thing, ignoring line numbers, comments, quoting and blank lines. Sequences
  compare in order; mappings by key set and value, because the dialect forbids
  a duplicate key.

### Changed

- `aval add` reports `unchanged` by declaration rather than by byte, and
  leaves such a file **exactly** as it is. Rewriting it would undo the
  formatter on every run, which is the loop this release ends.
- `pack --write`'s quoting is unchanged and is now a courtesy to formatters
  rather than the thing holding a vendored file's identity together.

## v1.2.1

A pack's quoting is now the style prettier would choose: double quotes,
unless a value holds more double quotes than single ones, in which case
single quotes with `'` doubled. The first pack that carried a rule statement
with a quoted word inside it was rewritten by the formatter in three
consumer repositories in one afternoon — every one of them runs prettier at
pre-commit — and a generated file that a formatter rewrites is a file that
gets hand-edited. Emitting the formatter's own choice makes it a no-op.

Content is unchanged: every value reads back byte-for-byte, and a consumer on
1.2.0 reads the new pack as before. A producer re-runs `aval pack --write`
(the version stamp moves anyway) and consumers re-run `aval add`, after which
prettier has nothing to say about `.adr/packs/`.

### Fixed

- `aval pack --write` chooses single quotes for a value holding more double
  quotes than single ones. The four statements in the fleet corpus that were
  being reformatted are the case in point.

## v1.2.0

Rules. A decision settles *what* is used; it never settled how the code that
uses it is written, and that second question was being answered the way the
first one used to be — a paragraph restated in six `CLAUDE.md` files, one of
them already wrong, and whatever a model remembered of a book it read in
training. A rule is now written once, in one file, adopted by a record, and it
stops being active when that record does.

**Two things to do after upgrading**, both because a format grew:

- a producer re-runs `aval pack --write` and each of its consumers re-runs
  `aval add`. A pack carrying `rules:` is unreadable by aval before 1.2 —
  `pack-parses: unknown pack field` — which is the existing version gate
  refusing a file it cannot fully understand rather than reading half of it, so
  upgrade consumers before publishing rules to them;
- everybody re-runs `aval hook install`. The session-start script gained the
  rules block; its bytes are its version, and `aval hook install --check`
  reports it stale until it is reinstalled.

Neither is urgent for a corpus that declares no rules: every check added here
needs a `rules:` in a registry to reach anything at all, so such a corpus
cannot tell this version from the last one. That is what makes this minor.

### Added

- **Rules.** A registry may list rule files:

  ```yaml
  rules:
    - docs/principles/clean-code.md
  ```

  A rule file is markdown with frontmatter naming the record that `adopts` it,
  and headings that are the declarations: `## <id> [constraint]` or
  `## <id> [heuristic]`, the first paragraph under the heading being the one
  line that gets printed, the rest being the translation. Literal paths, as
  `sources` and `packs` are, so a rule file that stops being listed is an error
  rather than a constraint that quietly stopped being printed.

  A rule carries the authority of its adopting record and no authority of its
  own: it is active while that record is accepted and still holds, and inactive
  — **not wrong** — once it is superseded, retired or still a draft. There are
  deliberately no per-rule supersession edges; a rule whose meaning changes
  gets a new id. SEMANTICS §2.4 is normative.

- **`aval rules` and `aval rule <id>`.** `rules` lists what is adopted, one
  line each, constraints before heuristics, `--level`, `--adopted-by` and
  `--all` to filter; exit 0 even when the list is empty, because an empty list
  is an answer. `rule` prints one rule with its body and the derived status of
  the record that adopts it; exit 7 with an advisory did-you-mean for an id the
  corpus does not declare, the same shape `show` uses.

- **`aval_rules` and `aval_rule` MCP tools**, both read-only, both requiring
  `repo` in a workspace. Their descriptions carry what a caller cannot infer:
  that a rule is adopted by a decision and carries its authority, and where it
  sits against the source it cites — a decision at the scope asked, then the
  default-scope decision, then the rule, then the book, as explanation only.

- **The session hook prints the constraints**, under the heads, with that
  precedence and a count of the heuristics beside them. Heuristics are fetched
  on demand rather than injected: advice that is right to break does not belong
  in every session's opening context. Silent when a corpus has no rules.

- **Packs carry rules**, with their bodies, sorted by id. A rule body is the
  one piece of prose a pack carries, and SEMANTICS §2.3's "a pack MUST NOT
  carry prose" is now "MUST NOT carry a record's body": a rule body is a
  declaration, printed nowhere by default and fetched by id, and a rule a
  consumer can list but cannot explain is one it cannot apply. `adopts` is
  qualified with the pack name on read, exactly as `replaces` is. A pack never
  re-exports vendored rules.

- **Three Layer A checks** — `rules-parse`, `rule-id-unique`,
  `rule-adopts-resolves` — and Layer C's `links-resolve` now runs over rule
  files too, since a rule body cites the code it is about.

### Fixed

- **A literal block scalar lost blank lines and any line beginning with `#`.**
  The lexer drops blank lines and whole-line comments, which is right
  everywhere except inside a block scalar, where both are content: a markdown
  body carried through `body: |` came back with its paragraphs run together and
  its `###` sub-headings deleted, silently, because the value still parsed.
  Folding now treats a blank line as a paragraph break rather than a double
  space, and `|` emits no trailing newline for an empty block.

## v1.1.0

A workspace of corpora. `aval mcp` started from a directory with no corpus
of its own — the parent of every project, say — used to answer every call
with "no `.adr.yaml` in … or any parent directory". It now discovers the
corpora one level down and answers for them.

### Added

- **Workspaces.** With no registry above the launch directory, discovery
  scans its direct children; each that carries a `.adr.yaml` is a
  repository, named by its directory. Every tool takes an optional
  `repo`. Named, it answers for that corpus alone, byte-equal to that
  repository's own `--json`. Omitted, `aval_resolve` and `aval_history` answer for every repository
  at once — a map keyed by name, which is the cross-repository question
  answered in one call. `aval_show` requires `repo`: record ids are
  corpus-local, and "show `ADR-0001`" across nine repositories is
  under-specified. `aval_keys` and `aval_heads` require it too: every
  repository's heads at once is tens of kilobytes of context, and a
  forgotten name must be an error listing the options, not that by
  accident. The CLI's `--all-repos` renders all four; a flag is an
  explicit ask.

  The map is a report, not a verdict (§14.1). `isError` is true only
  when no member answered anything; a member that would not load stands
  in the map as its error object. In-repo behaviour is byte-identical —
  a corpus found by walking up still wins, as it always did.

- **`aval repos`, and the `aval_repos` tool / `aval://repos` resource** —
  what discovery saw and why each directory is or is not answering:
  canonical root, worktree parent, `shadowed`, and every directory that
  looked like a corpus and could not be used, with its reason.

- **`--all-repos`** on `resolve`, `keys`, `heads` and `history`: the same
  map the server returns, rendered by the same code, so the two cannot
  drift. Exit `0` clean · `1` a member exited 5 or would not load · `3`
  none loaded · `2` with `--write`, `--check` or on `show`.

- **Per-repository resources.** A workspace lists `aval://<repo>/heads`
  and `aval://<repo>/keys` for each repository, the name percent-encoded.
  It deliberately does not offer the map as a resource: a resource is
  attached once and kept, and every repository's heads at once is tens
  of kilobytes to carry for a whole session.

- **Worktrees are detected, not folded in.** A linked worktree — its
  `.git` is a file naming `.git/worktrees/` — is left out of the map
  when its parent is also discovered, since the same corpus would answer
  twice, and is named in `worktrees_excluded`. One whose parent is not
  discovered stays in. A submodule is a repository of its own. No git
  subprocess is involved: the `.git` file is read.

### Fixed

- **A relative `-C` reported every contradiction's provenance as
  `unavailable`.** The root was used as typed, and the pathspec handed to
  `git -C root blame` no longer resolved after git's own chdir. The
  loader canonicalises the root it found, so the CLI and the server are
  fixed by the one change; which registry answers is unchanged, since the
  walk itself stays lexical. `heads --check --json` from a relative `-C`
  now reports the file's absolute path for the same reason.

- `{"repo": 123}` is a protocol error, not a query across every
  repository: `repo`, when present, must be a non-empty string.

## v1.0.0

The first stable release, and an agent-surface one: what the tools cost to
call, and what their text is.

**1.0 because this release breaks something.** §3.7's printable-value rule
is the kind of change §15 calls major, and shipping it as another 0.x
minor would have been the third release in a row where "breaking" and
"minor" were the same number. From here the words in §15 mean what semver
says: a caller may pin `1` and expect the verdicts, their exit codes, the
note strings, the `--json` field names and the frontmatter dialect to
hold. §15 lists exactly what that covers and what it does not.

### Upgrading

- **Bumping a repository's aval pin now needs `aval hook install` in the
  same commit.** The hook's preamble changed — it says the heads table is
  data rather than instruction, and names the MCP tools — and the script's
  own bytes are its version, so a repository that moves to 1.0.0 while
  carrying a hook written by an older one fails its own
  `aval hook install --check` step.

  This is the same coupling `aval.pack` has, and it fails the same way:
  one half moved and the other did not. A repository that stays on its
  current pin is unaffected, which is why nothing broke when 1.0.0 was
  published.

- A repository that publishes `aval.pack` must regenerate it, as at every
  release: the pack records the version that wrote it.

### Added

- **`aval://heads` and `aval://keys` as MCP resources.** A resource is
  context a client attaches once; a tool result is paid for on every
  call, and the session-start hook already prints the heads. Offering
  both lets a client pay for one.

- **`aval keys --names`**, and `detail` on the `aval_keys` tool. Both
  levels have a CLI counterpart deliberately: a tool the CLI cannot
  answer is a tool that can drift.

### Changed

- **A tool result carries its payload once.** It used to carry the JSON a
  model reads *and* a human rendering it does not need; on a 58-key
  corpus that duplicate was 27% of the bytes.

- **`aval_keys` defaults to names.** It is the call a caller makes
  *because* it does not know a key name, and it was the most expensive
  thing the server did: 29 KB, of which names were 1 KB and `decided` a
  third. Measured on that corpus: `aval_keys` **−56%**, `aval_resolve`
  −16%, `tools/list` −9%.

- **Both surfaces now say a record's wording is DATA**, and that text
  reading as an instruction should be reported rather than followed. The
  hook's preamble also names the tools, so a session that already has the
  heads resolves a key instead of fetching them again.

- **§2.3's reasoning is corrected.** It argued that a pack needs no
  consent gate because it is inert — true of execution, and not of text
  that reaches a model's context. "Inert" now scopes to execution, and
  the section says what actually holds the risk down: the surfaces name
  the text as data, §3.7 keeps the reviewed bytes and the printed bytes
  identical, and the pull request is the gate. Detecting
  instruction-shaped prose is deliberately not attempted — it fails open,
  and a false positive would block a legitimate decision.

### Breaking

- **A value carrying a control character or a bidi override is now a
  Layer A error** (§3.7). Per §15 a new check is major even when nothing
  it can fire on exists today: measured against all six conformance
  corpora and six real ones, every one stays clean.

## v0.7.0

Nothing a corpus resolves to changes. Every command's output is
byte-identical to 0.6.0 — verified across the six conformance corpora and
four real ones, text and JSON, exit codes included. This release is about
the shape of the code and of the published API.

### Fixed

- **A failure could be reported as an answer.** `Verdict::Internal`
  carried exit 3 — a *failure* code — inside an enum whose every other
  variant is a verdict, so "the tool broke" and "here is what was
  decided" were the same type. Every caller had to remember a variant
  meaning the opposite of its siblings, and the MCP surface did not: an
  inconsistent graph would have come back `isError: false`, against the
  rule §14.1 states. `resolve` returns `Result<Verdict, Inconsistent>`
  now, and the old shape is unrepresentable.

  Unreachable on a corpus that loaded, since Layer A rejects the
  replacement cycles that cause it — which is the argument for keeping it
  out of the success type, not for trusting the caller.

- **A hand-edited pack could declare both `first` and `replaces`.** Pack
  parsing never checked, and carried the contradiction into the graph.
  There is nowhere to put it now, so the pack is refused with the rest.

### Changed

- **`aval-core`'s API is versioned deliberately** (§15.1). Structs that
  may gain a field are `#[non_exhaustive]` with constructors; the enums
  are not, because §14 enumerates the verdicts and their exit codes, so
  adding one is major regardless — and marking them would cost the
  exhaustiveness check that makes an unrendered variant a compile error.

- **An entry's lineage is a sum type.** `first: bool` beside
  `replaces: Vec<_>` admitted four states where the model has two; the
  parser already rejected the other two, so the loose shape carried
  ruled-out states into everything that reads an entry.

- **A record id is `AdrId`, not `String`.** The qualification rule of
  §2.3 lived in a free function callers had to remember; it is on the
  type now.

- **`LoadError` renders itself.** Two callers matched its variants to
  build the same message, and a third was about to.

- **Lints moved into `[workspace.lints]`**, so an editor, a bare
  `cargo clippy` and CI see one set. `unsafe_code` is now *forbidden*
  rather than merely absent.

- `heads()` makes one pass with a set instead of two allocations and a
  linear scan per candidate; `migrate` no longer panics on a path ending
  in `..`; twenty public types gained `Debug`.

## v0.6.0

### Added

- **`aval mcp` — the corpus as MCP tools.** Five read-only tools over
  newline-delimited JSON-RPC on stdio: `aval_resolve`, `aval_keys`,
  `aval_heads`, `aval_show`, `aval_history`. Register it with

  ```
  claude mcp add aval -- aval mcp
  ```

  An agent could already run `aval resolve`; the session hook prints the
  commands. What this adds is **discoverable** access — tools a client
  lists without being told they exist, arguments checked as a schema
  rather than assembled into a command line, results as data rather than
  parsed back out of stdout, and descriptions that put the caller's
  obligations in front of the model at the moment it calls. "A suggestion
  is advisory" and "a contradiction means stop" are enforced on the shell
  by a caller that already knows them; here they are where a caller reads
  them.

  **Every verdict is `isError: false`, contradiction included.** MCP gives
  a tool result one error flag, and raising it on a non-zero exit code
  would report *stop, do not pick one* as a malfunction — teaching a
  caller to retry the one verdict where routing around it is the specific
  harm the corpus exists to prevent. `isError` means no question was
  answered: a corpus that will not load, or a name it does not carry.

  The corpus is reread on every call, because an agent edits records in
  the same session it asks questions in. Startup reads nothing, so a
  registry mid-edit does not take the surface away. Nothing writes.

- **`aval keys` — the decision vocabulary.** Every key, its description,
  the scopes it is answerable at and where it is already decided. Nothing
  could enumerate keys before, and §12.1 rules out finding one by
  similarity, so a caller had to already know the name it was looking for.

  Direct occupancy only: "decided at this scope" and "answers at this
  scope" are different questions, and merging them would rebuild the
  fallback ambiguity §5 exists to prevent. A key declaring no scopes
  (`null`, every declared scope) stays distinct from one declaring an
  empty list (fleet-wide only).

### Upgrading

- **A repository that publishes `aval.pack` must regenerate it.** The pack
  records the version that wrote it, so every release makes a published
  pack stale and `aval check` reports `pack-fresh` until `aval pack
  --write` runs. This is not new in 0.6.0 — any version bump does it — but
  it is the step to take alongside the upgrade, in the producing repository
  only. Consumers vendor the file and regenerate nothing.

### Fixed

- **The JSON reader accepted invalid JSON.** It ended in a catch-all that
  mapped an unknown escape to itself and pushed any character at all, so
  `"\q"` read as `q` and a raw control character passed silently. It also
  decoded each `\u` alone, which rejects the *valid* surrogate pair JSON
  uses to spell an astral code point. Integers now keep their exact value,
  so a JSON-RPC id past 2^53 survives the round trip that MCP requires.

  Existing `--json` payloads are unchanged — 47 across the six conformance
  corpora compare byte-identical, exit codes included.

- **`heads` and `history` ignored `--json`.** Bare `aval heads` printed the
  markdown table instead of a result object, and `aval history` printed
  both of its rejections to stderr and returned 7 with nothing at all on
  stdout. §12 says `--json` writes the result object and nothing else.

  `heads --json` reports every **occupied** slot, deliberately a superset
  of `HEADS.md`: the projection keeps only single-head slots, so a
  contradicted corpus renders as "Active: None" — empty rather than
  conflicted. That is fine in a document a person reads beside the corpus
  and wrong as an answer to a caller.

## v0.5.0

Vendoring, so a decision made once can be read from the repositories it
applies to.

Nine fleet decisions lived in one repository and were readable from exactly
that repository. Six others restated a subset of them in prose, and one of
those copies had already diverged. That is the duplication this tool was built
to end, reproducing itself while the tool sat somewhere none of them could
read.

**`aval pack`** publishes a corpus's declarations — the scope vocabulary, the
key definitions, and every record's frontmatter — to `aval.pack` at the
repository root. Not the projection: `HEADS.md` is derived state, and
`no-manual-index` exists precisely because copied derived state has no
invariant behind it. A consumer computes the heads itself, from the same graph,
with the same code, so a superseded decision cannot survive the trip. The nine
fleet records come to 3.7 KB.

**`aval add <source>`** vendors one into `.adr/packs/<name>.yaml` and lists it
under a new registry key, `packs:`. `dir` is now optional when `packs` is
present, so a repository can read the fleet corpus without starting one of its
own — which is the cheapest possible adoption and the only one four of those
six were ever going to make.

Records arrive namespaced: `ADR-0002` from the `decisions` pack is
`decisions:ADR-0002`. Two repositories both numbering from one is the ordinary
case, and an unqualified collision would report `id-unique` against a corpus
whose author wrote neither id. Qualification happens on read, so the vendored
file stays byte-equal to what the producer published.

**Divergence, and the shape it actually takes.** A consumer may *widen* a
vendored key — re-declaring it locally with a `scopes` list adds those scopes.
The list is what is being added, so narrowing is not something the format can
say. This is the case that arises in practice: one repository holds several SQL
layers, one in the browser and one in the app server, and neither disagrees
with the fleet's answer at the fleet's scope. They are different slots.

Deciding a slot the pack already decides is two heads for one slot — exit 5,
through the invariant the model already had. Nothing new enforces it, which is
the reason vendoring is worth doing.

**Transport is git and nothing else**, structured after `amont`'s and for its
stated reason: this binary links no crates, has no TLS stack, and runs on the
pre-commit path. git is already a hard dependency and is content-addressed, so
`@v1` is resolved to a commit id before anything is fetched and whatever
arrives is refused unless it hashes to that id. It also answers the credential
question by not asking it — the fleet corpus is private on one forge and read
by repositories on another that sits behind a client certificate, and shelling
out to `git` means both work with the user's own credentials and none of this
tool's. No token was minted for any of it.

**No trust prompt, deliberately.** `amont add` vendors commands, so it takes
consent per machine and re-takes it on every byte that changes. A pack is inert
data; nothing in it is ever executed. What it can do is change an answer, and
the review gate for that is the pull request that adds the file.

**Staleness is reported, never repaired.** `amont` does not check whether a
vendored block is behind, because its rows are commands and being behind is the
safe direction. Here it is the opposite: a stale pack answers `active` with a
decision that was superseded, which is the failure the whole tool exists to
prevent. `aval add --check` re-resolves each recorded revision and says whether
it still names the recorded commit. It reaches the network, so no hook, gate or
`resolve` calls it, and none can — the consumers' CI has no credential for the
source. The fanout after a fleet decision changes is manual, per repository,
and is now written down as a cost rather than left to be discovered.

Also in this release:

- `pack-fresh`, a Layer C check, so a producer cannot publish declarations that
  disagree with its own records. It fires only where an `aval.pack` exists, so
  no corpus that was clean starts failing.
- A pack never re-exports what it vendored. Otherwise one consumer's copy of a
  decision reaches another by a route neither chose.
- `resolve` says which pack an answer came from, in text and in `--json`. A
  consumer has to be able to tell a decision it can change from one it cannot.
- **Fixed: a quoted value containing ` # ` was silently truncated.** The YAML
  comment stripper did not respect quotes, so `choice: "Kong # the gateway"`
  parsed as `Kong` — no check fired, and the projection stated a decision
  nobody wrote. Quoting is the documented way to protect a value, so it now
  actually protects it. Reachable in frontmatter all along; the pack format,
  which quotes every value, is what made it certain.

## v0.4.1

One paragraph added to the session hook's preamble, and it is there because of
a specific mistake made while building this:

> What is deployed is not what was decided. Two implementations both running is
> not evidence that both were chosen — it is equally consistent with one having
> replaced the other and the code not having caught up. Ask someone; do not
> infer a decision from what is running.

Two sync transports were live in one repository, each documented as current,
and the conversion recorded them as one decision per mode. They were not: one
had replaced the other and the code had not caught up. The repository's own
evidence was equally consistent with both readings, so reading it harder would
not have helped — the fix is to ask, and the preamble is where that instruction
reaches an agent at the moment it matters.

Deliberately unconditional rather than triggered. The shape is detectable — a
key with entries at two non-default scopes, both `first`, no edge between them
— and it does not discriminate: sre-agent's `lane.product` has exactly that
shape and is correct. A check on it would fire on clean corpora, and `check`
has no warning tier.

**This changes the generated script, so `aval hook install --check` reports
stale in every repository that has one until `aval hook install` is run
there.** That is the upgrade lever working as designed, not a fault.

## v0.4.0

**`aval hook install`** — a session-start hook that prints the current decision
heads, so whatever is about to write code starts from what was decided rather
than from what it can infer. That is the failure this tool was built for: the
same design-system rule copied into six files, one of them already wrong.

Modelled on `duro hook install` deliberately, because that pattern is proven
across several repositories and a second shape would be a second thing to
learn. Two differences, both because this payload is local rather than fetched:
there is no cache, no expiry and no staging file, and nothing to add to
`.gitignore`.

- Writes `.claude/hooks/aval-heads.sh` and merges one entry into
  `.claude/settings.json`. Unrelated keys and other tools' hooks survive; a
  settings file that does not parse stops the install with the repository
  untouched.
- A variant spelling of the command is rewritten in place rather than joined by
  a second entry that does the same thing on every session start.
- **Silent when `aval` is absent**, so committing the hook cannot fail a
  colleague's session, and silent when there is no corpus to report — saying so
  is the gate's job.
- `.claude/aval-hook.local.md` is appended by the hook and never written by
  install, so repo-specific caveats survive regeneration.
- `--check` is the drift detector: exit 0 wired, exit 1 stale, and it writes
  nothing.
- Warns when git ignores the files it just wrote, which would leave the hook
  working for whoever ran the command and for nobody else.

The settings merge re-serialises the file, which sorts its top-level keys. That
is one-time and stable. The alternative was making every `--json` key order
depend on construction order instead, which is a worse trade for a contract
that machines read.

`Json::write_pretty` is added beside `write`, which stays compact because it
serves `--json` where byte-stability is the contract.

## v0.3.1

Fixes a false positive introduced in 0.3.0, found by running the new binary
against homelab rather than by a test — which is why there is now a test.

0.3.0 gave each document its own base for resolving citations, which was the
point: a specification in `docs/` must not have `[x](./diagram.svg)` read as
pointing inside `docs/adr/`. But the repository-relative path handed to the
gitignore and gitlink checks was computed from that base unconditionally,
while the existence check still resolved a bare backticked token from the
repository root. The two disagreed.

The effect: every citation into a git submodule was reported dangling. In
homelab, `vault-transit-unseal-operator/README.md` became
`docs/adr/vault-transit-unseal-operator/README.md`, matched no submodule
prefix, and so was never skipped.

There is now one function deciding the base, and all three consumers use it.

## v0.3.0

Two defects that four real corpora exposed. **Breaking**, and the two reasons
are on opposite sides: a formatted `HEADS.md` goes from a finding to clean, and
`id-matches-filename` relaxes, so a corpus that failed can now pass.

### The projection survives a markdown formatter

`heads-fresh` was exact string equality, and prettier pads table cells and
rewrites the delimiter row, so a formatted projection read as stale forever.
Eighteen repositories in the fleet this serves run prettier over their
markdown; excluding the file from formatting does not scale to eighteen.

Both sides are now canonicalised before comparison. Line endings, a byte-order
mark, trailing whitespace, trailing blank lines, cell padding and the delimiter
row's style are normalised away. Nothing else is.

It is a whitelist, not a parser, and that distinction is the design. A reader
that compared modelled rows would ignore what it does not model, so a paragraph
added under the banner — or a banner rewritten to say the file is maintained by
hand — would compare equal and pass indefinitely. Rows compare as an ordered
sequence, because the projection is sorted and nothing else would enforce that.

Findings now name the row that differs. `heads --write` leaves a file alone
when it already states the projection, so the formatter's output is not undone
on every run; anything not already current is overwritten, including a file
unreadable as a projection, so there is still no state it cannot repair.

Two render bugs had to go first. `cell()` escaped `|` and not `\`, so the
choice `A\|B` emitted `A\\|B` and produced a five-column row. And `choice`
accepted a YAML block scalar, so a newline split one record across several
rows; that is now a Layer A error with its line.

### Specifications can carry decisions

A decision stated in `docs/spec-change-proposals.md` was invisible: one flat
directory, and every filename without a numeric prefix skipped.

```yaml
dir: docs/adr
sources:
  - docs/spec-change-proposals.md
```

Literal repository-relative paths, and patterns are refused with the reason. A
pattern that stops matching drops a record silently — its entries leave the
graph, whatever it superseded returns as a head, and `resolve` answers `active`
with a decision that was replaced. A listed file that is missing is an error
instead. A pattern slightly too wide fails the other way and captures unrelated
frontmatter.

`sources` supplements `dir`; a file reachable both ways is read once and the
`dir` rule wins, so mandatory frontmatter is never traded away. How a file was
found decides how its id is judged, so an ordinary `2024-payments.md` is not
required to call itself ADR-2024. A listed record carries a slug, which may not
begin `ADR-`.

`Adr.file` became a repository-relative path, which silently broke two things,
both fixed here: provenance joined the ADR directory to a name that already
contained it, and citations resolved every document against the ADR directory
rather than its own.

### Also

- `status-single-source`: a document claiming approval in prose while its
  frontmatter already owns it. Narrow by construction — a status line, in the
  header block, in the four shapes real documents use, with one of four
  approval words. Rollout prose is silent. Zero findings across four real
  corpora before shipping.
- The projection's third column is headed `Record`, not `ADR`, since it holds
  slug ids too.
- `heads --json` writes its object to stdout as section 12 always required. It
  previously printed nothing under `--write`, nothing on a clean `--check`, and
  findings to stderr.
- SEMANTICS said "Version 0.1.0-draft" while the tool was at 0.2.0, and §14
  omitted the exit code `heads --write` actually returns on a write failure.
- The five conformance projections were never asserted against anything, and
  the battery carried its own copy of corpus discovery. Both fixed, and there
  are now CLI tests that run the binary.

## v0.2.0

**A key can declare which scopes it is decided along.**

```yaml
scopes: [homelab, cloud, effect-stack]
keys:
  cni.routing-mode:
    scopes: [homelab, cloud]
```

A scope list can span more than one axis. Clusters, landscapes and stack
families are all scopes and none of them are interchangeable, so a single flat
list made `scope-declared` unable to reject `cni.routing-mode@effect-stack` —
and §5 fallback then answered it from the default scope. That is an `active`
verdict about a different question, which reads as agreement. This is the change
the fleet corpus needs before it can hold decisions from more than one axis.

- An entry deciding a key outside its declared scopes is a new Layer A check,
  `scope-applies`.
- A query at such a scope returns `unknown` (exit 7) and never falls back. Its
  JSON payload carries `applies_to`, the axis the caller should have asked on,
  in place of a did-you-mean.
- The default scope stays admitted whatever a key declares, so restricting a key
  cannot sever its own fallback.
- A key with no `scopes` accepts every declared scope, so every existing
  registry keeps its meaning. An empty list is not the same as absent: it says
  the key is decided fleet-wide only.

Spec: SEMANTICS section 2.1. Four conformance cases and a `scoped-keys` corpus.

## v0.1.1

Fixes two release-workflow bugs that stopped v0.1.0 publishing anything. No
change to the tool itself.

**The audit gate failed on a clean dependency tree.** Its exit-code logic is
written deliberately without `set -e` so it can tell "found vulnerabilities"
apart from "could not check" — but GitHub's default shell adds `-e`, so the
step aborted before any of that ran. It died on the good case: `grep` exits 1
when it matches nothing, `pipefail` propagates that into the assignment, and
`-e` killed the step. A tree with no advisories was the one input that could
not pass. This is latent in amont too, hidden only because its tree carries two
warning-class advisories, so its grep always matches.

**Two cross-compilation targets timed out on apt.** The step pinned
archive.ubuntu.com because amont had seen the Azure mirror hang; this time
archive.ubuntu.com was the one hanging. It no longer picks a winner: it tries
the runner's own mirror, falls back to the canonical archive, and says which
one answered.

## v0.1.0

First release. The resolver, the invariants, the projection and the CLI, proved
against a real corpus rather than a synthetic one: `homelab/docs/adr`, eighteen
decision records, `aval check` clean.

**What it does.** `aval resolve <key>` answers what is decided now, with a typed
answer and typed non-answers, so a caller branches on an exit code instead of
reading three documents and reasoning its way to a plausible wrong one. Exit 0
active, 4 undecided, 5 contradiction, 6 retired, 7 unknown. Codes 1, 2 and 3
mean the tool failed, was misused, or could not read the corpus, and never
share a range with a verdict.

**What is normative.** `SEMANTICS.md`, which ships in the release archive. Read
that before the README if you intend to write ADRs against this.

**Known limits of this release.** There is no cross-corpus resolution: one
registry, one directory, no shared vocabulary between repositories. The
frontmatter dialect is a defined subset of YAML rather than all of it, and
anything outside it is rejected with a line number rather than guessed at. There
is no tool surface: an agent reaches the corpus by running the CLI.

*(Both limits were later retired — cross-corpus resolution by packs in v0.5.0,
the tool surface by `aval mcp` in v0.6.0. This paragraph said "the MCP server is
specified but unbuilt" until v0.6.0, which was wrong on the first word: no
specification existed anywhere, only three tool names in a plan that had already
been rewritten without them.)*

Sections below record how it was built, and are kept because each names a
mistake worth not repeating.

### Phase 0 — specification

- `SEMANTICS.md`: the normative specification. Decision keys, flat scopes, slots
  and entries; the `replaces` / `overrides` edge split; retirement as an ordinary
  entry; draft semantics; derived and partial supersession; the three validation
  layers; per-command exit contracts; determinism and provenance rules.
- `README.md`, MIT `LICENSE`.

### Phase 1 — converted sample

- `conformance/resolve.json`: 26 cases over four corpora, with a `min_cases`
  guard.
- `conformance/corpora/homelab-sample`: ten ADRs converted from
  `homelab/docs/adr`, plus the three successors Phase 3 will write.
- `conformance/corpora/{diamond,contradiction,retire-scoped}`: the cases the
  real corpus does not contain.

Two spec changes that building the fixtures forced:

- **Removed `no-orphan-key` and `retired-attributable`.** Per-slot supersession
  makes an orphaned key structurally impossible, and retirement living in an ADR
  makes an unattributed retirement inexpressible. Both checks guarded a model
  this one does not have. Recorded in SEMANTICS section 10.1.
- **Added SEMANTICS section 6.1**, the two branches of what a retirement names.
  A scoped retirement of an *inherited* default has no same-slot predecessor, so
  it declares `first: true` with `overrides:` instead. `overrides` on a
  retirement is consequently load-bearing, and the earlier ban on it was wrong.

### Phase 2 — core and CLI

- `aval-core`: a restricted-YAML frontmatter parser (SEMANTICS 3.7) with
  line-numbered rejections, the model, every Layer A and Layer B invariant, the
  resolver, the `HEADS.md` projection, and a JSON reader and writer. Pure: no
  clocks, no I/O, no git.
- `aval`: `resolve`, `check`, `heads`, `show`, `history`, with the per-command
  exit contracts of SEMANTICS section 14. Filesystem loading, the Layer C
  `links-resolve` and `no-manual-index` checks, and git provenance.
- No external dependencies, enforced by `scripts/check-no-deps.sh` in CI. The
  binary runs on the pre-commit path, where amont's fleet rule is that the hook
  pulls in nothing.
- Toolchain pinned; `make check` runs what CI runs.

Two spec corrections the implementation forced:

- **Tags were silently accepted.** `a: !!str x` parsed into the string
  `"!!str x"` while SEMANTICS claimed tags were rejected. The parser now rejects
  them with a line number, matching the document.
- **A plain value may contain a colon.** SEMANTICS claimed quoting was required;
  only the first `: ` splits a key, so `choice: Kafka: the sequel` reads as
  written. The document was wrong, not the parser.

### Phase 3 — the real corpus

Backfilled `homelab/docs/adr`: eighteen documents, `aval check` clean. Landed in
that repository as `docs/adr-frontmatter-backfill`.

- `aval migrate <dir>` reads the legacy prose-bullet format and reports what a
  conversion needs, per document and for the corpus. It never writes: which
  decision keys a document carries, and whether it was ever approved, are
  judgments a rewrite cannot make.
- `HEADS.md` is written beside the corpus rather than at the repository root.

**`links-resolve` was almost useless and the real corpus proved it.** The first
version reported roughly a hundred dangling citations in a sixteen-document
corpus, of which six were real. Every fix below came from a citation that
actually exists in that corpus, and SEMANTICS 11.1 now states the whole list:

- Markdown links resolve against the **document**, not the repository root.
  Resolving them at the root made every sibling ADR cross-reference dangle.
- A backticked path is only checked when its first segment is an existing
  top-level entry of the repository. That is what separates a repository path
  from a URL path, a Vault path, a container path, a path into a different
  repository, and a forge slug.
- Brace expansion is a glob; CIDR blocks and host-rooted references are not
  files; `:line` suffixes address a place inside a file.
- Gitignored paths and paths inside a gitlink are skipped: both are legitimately
  absent from a fresh worktree.
- A draft's citations are not checked. A draft proposes files, and asking an
  author to pin a future is nonsense.

One bug in that work is worth naming on its own. `git check-ignore --stdin`
**aborts the entire batch** with exit 128 and an empty stdout when a single
input path sits inside a submodule or outside the repository. Reading that as
"nothing is ignored" turned every genuinely ignored path into a false report. A
failed command now falls back to asking one path at a time rather than being
mistaken for an answer.
