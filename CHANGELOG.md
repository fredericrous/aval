# Changelog

## Unreleased

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
anything outside it is rejected with a line number rather than guessed at. The
MCP server is specified but unbuilt.

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
