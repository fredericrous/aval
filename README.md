# aval

**Git is the history. ADRs are the evidence. `aval resolve` tells you what is
true now.**

Most ADR tooling helps you write and browse decision records. `aval` answers a
different question, the one a coding agent actually needs before it writes a
line of code:

```console
$ aval resolve storage.object-store --scope homelab
active   ADR-0021   Ceph RGW
$ echo $?
0
```

and, just as importantly, refuses to guess when it should not:

```console
$ aval resolve data.realtime-processing
undecided   no accepted decision for this key
$ echo $?
4

$ aval resolve api.gateway
contradiction   two heads for api.gateway
  ADR-0062  introduced in 4f2ac1d
  ADR-0071  introduced in 9bd3e08
$ echo $?
5
```

An agent branches on a number. It never reads three documents and reasons its
way to a plausible wrong answer.

## The idea

ADRs are not the current architecture. **ADRs are the evidence from which the
current architecture is computed.**

- A **decision key** names one architectural question, from a controlled
  vocabulary. `storage.object-store`, not "the storage doc".
- An **ADR** is an event that changes one or more keys.
- **Superseded is never written down.** It is derived from supersession edges.
  A hand-written status is copied state, and copied state drifts.
- **`HEADS.md` is a projection**, regenerated and byte-compared in CI. Nobody
  edits it.

## Typed non-answers

The design is mostly about what happens when there is no clean answer, because
that is where an agent left to its own judgment does damage.

| Exit | Verdict | Means |
|---|---|---|
| 0 | `active` | here is the decision |
| 4 | `undecided` | the key exists, nothing decided it |
| 5 | `contradiction` | competing heads. Stop, do not pick one. |
| 6 | `retired` | an ADR deliberately retired this key |
| 7 | `unknown` | no such key or scope, or a key not decided on that axis |

## Which decisions bear on this change

`resolve` needs a key, which is a chicken-and-egg problem at the start of a
task: the way to learn that a decision governs the file you are about to edit
is to already know its name. The alternative is `HEADS.md`, which is every
decision the repository has ever made.

`aval relevant` ranks the vocabulary against what you are about to touch:

```console
$ aval relevant --path crates/aval/src/mcp.rs --text "release a new version by pushing a tag"
advisory   A ranking is a suggestion: it resolves nothing. Only `aval resolve <key>` answers.

   14.2104  release.trigger         active          ADR-0008   A pushed version tag, never a merge
   11.5524  release.version-scheme  active          ADR-0012   Semantic Versioning 2.0.0
    4.9182  ci.test-gate-stage      active          ADR-0009   pre-commit
    4.7105  forge.primary           undecided       no accepted decision for forge.primary
```

**This is retrieval, not resolution**, and the output says so on every run. The
order is a guess about attention. What is not a guess is the verdict on each
row: it comes from the same `resolve` the rest of the tool runs, which is what
makes the `undecided` row the interesting one — nobody decided it, and an agent
that quietly fills the gap is doing the thing the corpus exists to prevent.

### The signals

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
answer reports one. Rules (below) that match the same words are listed after
the keys, clearly apart.

Exit is **always 0**, usage errors aside. A ranking has no verdict to report,
and "I ranked and found little" must not share a code with "I could not look".

### For a router

`--json` carries a `dependencies` array — the same keys, compacted:

```json
{"dependencies":[
  {"key":"release.trigger","state":"active","exit":0,"adr":"ADR-0008","unresolved":false},
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

## In front of an agent

A corpus that resolves is half the point. The other half is that whatever is
about to write code starts from what was decided:

```console
$ aval hook install
  wrote  .claude/hooks/aval-heads.sh  (created)
  wrote  .claude/settings.json  (merged)
```

It writes a session-start hook that prints the current heads, and merges one
entry into `.claude/settings.json` without disturbing what else is there. The
hook is **silent** when `aval` is not installed, so committing it cannot fail a
colleague's session, and silent when there is no corpus to report.

Repo-specific caveats go in `.claude/aval-hook.local.md`. The hook appends that
file; installing again never touches it. `--check` is the drift detector for
CI: exit 0 wired, exit 1 stale.

The script's second line names the aval that wrote it:

```sh
#!/bin/sh
# aval-hook: written by aval 1.5.0
```

A script from a **newer** aval is not stale, and an older aval leaves it alone
rather than downgrading it, so a workstation that upgrades first does not
redden a CI job pinned to the release before. Both modes also print a `note`
for every `AVAL_VERSION:` pin under `.github/workflows` or `.forgejo/workflows`
that is behind or ahead of the running aval — advisory, never an exit code,
never an edit to the workflow.

Where the repository vendors packs, the hook also asks whether they are still
what their sources publish — at most once an hour per repository, under a
five-second budget that kills the remote call rather than waiting on it, and
speaking only when a pack is behind or edited:

```
VENDORED DECISIONS ARE BEHIND THEIR SOURCE. The fleet has decided something
this repository has not adopted yet, so the heads below may be superseded:
  behind    fleet has 1bb600e, main now names e73638d

1 pack(s) behind. Run `aval add <source>` for each, read the diff, then `aval heads --write`.
```

Offline, a slow remote, lost access: silence, on purpose. A notice that fired
on every flaky network would be the one nobody read on the day a decision
changed. The stamp that rates the hour lives under `$XDG_CACHE_HOME/aval`
(default `~/.cache/aval`), never in the repository. `aval add --check` by hand
always answers in full.

The hook pushes; **`aval mcp` lets an agent pull** — the same answers as native
tools, asked at the moment the question comes up rather than only at the top of
a session:

```console
$ claude mcp add aval -- aval mcp
```

Nine read-only tools: `aval_resolve`, `aval_relevant`, `aval_keys`,
`aval_heads`, `aval_show`, `aval_history`, `aval_rules`, `aval_rule` and
`aval_repos`. They resolve through the same graph the CLI does and return the
same bytes `--json` would, so the two surfaces cannot drift apart.

`aval_relevant` is the one whose result is **not** an answer, and its
description says so where a model will read it: the ranking is advisory, what
is authoritative is the verdict beside each key, and an `undecided` row is the
reason to have called it.

The distinction that matters: **a verdict is not an error**. `undecided`,
`retired`, `unknown` and `contradiction` all come back as ordinary results with
`isError: false`, because each is an answer. Reporting `contradiction` as a tool
failure would teach an agent to retry or work around the one verdict that means
*stop and ask a human* — so the flag is reserved for a corpus that would not
load, or a name it does not carry.

Nothing on that surface writes. Resolving answers a question; deciding is not
something to do on an agent's behalf.

**From the parent of all your projects**, where there is no corpus above and
several below, the same server is a **workspace**: every tool takes `repo` to name one.
`resolve` and `history` answer for all of them when it is omitted — a map
keyed by name, which is the cross-repository question in a single call —
while `keys`, `heads` and `show` ask for a name, because every repository's
heads at once is a lot of context to fetch by forgetting an argument.
`aval repos` says what was found, and `--all-repos` renders the same map from
the shell:

```console
$ aval resolve stack.sql-layer --scope effect-stack --all-repos
== decisions ==
active   ADR-0002   @effect/sql

== homelab ==
unknown   no such key `stack.sql-layer`
…
```

A linked worktree is detected and left out when its parent is also there, so
one corpus never answers twice; it stays addressable by name.

Codes `1`, `2` and `3` mean the tool failed, was misused, or could not read the
corpus. They never overlap a verdict, so "I could not look" is never mistaken
for "I looked and found nothing".

## Scopes

A decision can be global or partitioned. Choosing VXLAN for one cluster must not
silently change another, so scoped divergence and in-place replacement are
different edges:

```yaml
- key: cni.routing-mode
  scope: cloud
  choice: VXLAN tunnel + WireGuard
  first: true
  overrides: ADR-0006      # the global native-routing head STAYS a head
```

## Commands

| | |
|---|---|
| `aval resolve <key> [--scope S]` | the authoritative lookup |
| `aval relevant [--path P]… [--text W] [--changed] [--top N]` | which keys bear on what you are about to touch, ranked, each with its verdict. Advisory: it resolves nothing |
| `aval check` | every invariant; what the git hook and CI run |
| `aval heads [--write\|--check]` | the projection |
| `aval show ADR-0015` | derived status, including partial supersession |
| `aval history <key>` | the chain, labelled as history |
| `aval rules [--level L] [--all]` | the rules the decisions have adopted, one line each |
| `aval rule <id>` | one rule, with its translation and why |
| `aval hook install [--check]` | put the heads in front of an agent at session start |
| `aval pack [--write\|--check]` | publish this corpus's declarations for others to read |
| `aval add <source>… [--dry-run]` | vendor another repository's declarations |
| `aval add --check [--quiet] [--budget S]` | are the vendored packs still what their revisions name; `--quiet` speaks only when one is not, `--budget` kills a remote call that has not answered in S seconds |

## Rules: what a decision does not settle

A decision settles *what* is used. How the code that uses it is written gets
answered the way the first question used to be — a paragraph restated in six
`CLAUDE.md` files, one already wrong, and whatever a model remembers of a book.
So a **rule** is written once, in a markdown file whose headings are the
declarations, and it is **adopted by a record**:

```markdown
---
adopts: ADR-0011
source: Clean Code (Robert C. Martin, 2008)
---

## names.reveal-intent [constraint]

Names reveal intention: an identifier says what it holds, in the vocabulary
of the domain, and a reader never decodes an abbreviation.

The body is the translation — what this means here, and what it does not cover.
```

A rule has no authority of its own: it is active exactly while the record that
adopts it still holds, so superseding that record retires its rules with it.
There are no per-rule supersession edges, because the graph already tracks the
record — a rule whose meaning changes gets a new id.

```console
$ aval rules
constraint names.reveal-intent   Names reveal intention: an identifier says what it holds …
heuristic  functions.few-arguments   A function takes no more inputs than it uses …
```

A `constraint` is followed and a review blocks on it; the session hook prints
every active one. A `heuristic` is followed unless a reviewer argues why not,
in that place, and is fetched on demand. Precedence, which the hook also
prints: the decision at the scope asked, then the default-scope decision, then
these rules, then the book a rule cites — as explanation only. Remembered
advice from that book does not outrank a rule here.

## Sharing one decision across repositories

A decision made once should be readable everywhere it applies. `aval pack`
publishes a corpus's declarations; `aval add` vendors them into another
repository, which then resolves them as if they were its own.

```console
$ aval add github:acme/decisions
$ aval resolve stack.sql-layer --scope effect-stack
active   decisions:ADR-0002   @effect/sql
  vendored: from the `decisions` pack; change it there, not here
```

A consumer needs no corpus of its own — a registry with `packs:` and no `dir:`
is enough. Transport is git and only git, so a private repository and a forge
behind a client certificate both work with your own credentials and no token
issued to this tool.

The vendored file is `.adr/packs/<name>.pack`, with the extension the producer's
own published file carries and no formatter claims. A `.adr/packs/<name>.yaml`
written before 1.3 still loads, and the next `aval add` moves it and rewrites
its registry line. What makes it still the published pack is what it
*declares*, not its bytes — so a formatter that reformatted it has changed
nothing, while a hand-edited decision is reported by `aval add --check` as
`edited`.

What a consumer cannot do is quietly disagree. A local record deciding a slot a
pack already decides is two heads for one slot, which is exit 5 — the invariant
the model already had, and the reason vendoring is worth anything. What it
*can* do is answer the same key at a scope of its own: one repository may hold
a SQL layer in the browser and another in the app server without either being
a disagreement with the fleet's answer at the fleet's scope.

Nothing in a pack is ever executed, so there is no trust prompt to match
`amont trust`. The review gate is the pull request that adds the file.

A pack goes stale silently otherwise, so there are two ways to be told. The
session hook asks (see [In front of an agent](#in-front-of-an-agent)). And a
consumer's CI can ask, as an advisory job, where its runner holds a credential
that can read the source — a private corpus is reachable from a private
consumer's runner with a deploy key, not from a public one's without:

```yaml
  packs:
    name: vendored decisions are current (advisory, non-blocking)
    runs-on: ubuntu-latest
    continue-on-error: true
    steps:
      - uses: actions/checkout@v4
      - uses: webfactory/ssh-agent@v0.9.0   # or however the runner reaches the source
        with:
          ssh-private-key: ${{ secrets.DECISIONS_READ_KEY }}
      - run: |
          if ! aval add --check --quiet --budget 30 > packs.txt; then
            cat packs.txt
            echo "::warning::vendored decisions are behind or edited — run aval add"
          fi
```

Non-blocking for the same reason a dependency advisory is: a decision that
changed upstream is information about the fleet, not a defect in the change
under review.

That "inert" scopes to execution. A pack's text does reach an agent's context,
so the hook and the MCP tools both say that a record's wording is data rather
than instruction, and a value carrying a control character or a bidi override
is refused — the reviewer and the model must see the same bytes.

## Status

Published, and in use: the resolver, the invariants, the projection, the CLI,
vendoring and the tool surface all work, against real corpora rather than
fixtures. [`SEMANTICS.md`](SEMANTICS.md) is normative and is the place to
start if you intend to write records against this.

**1.0.** The verdicts, their exit codes, the note strings, the `--json` field
names and the frontmatter dialect are stable; §15 of [`SEMANTICS.md`](SEMANTICS.md)
says exactly what that covers, and what it deliberately does not.

## Install

    curl -fsSL https://raw.githubusercontent.com/fredericrous/aval/main/install/install.sh | sh

Pin a version or move the destination with `AVAL_VERSION` and `AVAL_BIN_DIR`.
Windows: `irm https://raw.githubusercontent.com/fredericrous/aval/main/install/install.ps1 | iex`.

Also `cargo install aval`, and `npx aval-adr` — the npm package carries the
suffix because plain `aval` was taken in 2016 by an unrelated property
validator; the binary it installs is still `aval`.

Nothing is gated by installing. To gate a repository, one committed line in its
[`amont.conf`](https://github.com/fredericrous/amont):

    pre-commit    adr   *+.adr.yaml   block   aval check

Records live under `dir`, and a specification that carries decisions can be
named where it is rather than moved:

```yaml
dir: docs/adr
sources:
  - docs/spec-change-proposals.md
```

Literal paths, not patterns: a listed file that goes missing is an error, where
a pattern that stops matching would drop the record and let a superseded
decision come back as the current one.

The `+` keeps it inert in any repository without a `.adr.yaml`, and a missing
binary is reported as a gap rather than blocking a commit.

## Building

    make check      # what CI runs: no-deps, fmt, clippy, tests
    cargo build --release

No external dependencies, by design and enforced in CI. `aval` runs on the
pre-commit path, so it pulls in nothing. `make check` uses rustup's shim when
one is present, because a Homebrew cargo earlier on `PATH` ignores the
toolchain pin and would lint with a different clippy than CI.

## License

MIT
