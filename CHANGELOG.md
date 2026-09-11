# Changelog

## Unreleased

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
