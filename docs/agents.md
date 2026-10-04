# In front of an agent

A corpus that resolves is half the point. The other half is that whatever is
about to write code starts from what was decided. There are two ways in: a
session hook that **pushes** the heads at the start, and an MCP server an agent
**pulls** from when the question comes up.

## The session hook

```console
$ aval hook install
  wrote  .claude/hooks/aval-heads.sh  (created)
  wrote  .claude/settings.json  (created)

Repo-specific caveats go in .claude/aval-hook.local.md — the hook appends that file after the
heads, and installing again leaves it alone.
```

It writes a session-start hook that prints the current heads, the ids of the
active constraints grouped by prefix (`aval rule <id>` has each one's text) and
a one-line traits summary, and merges one entry into
`.claude/settings.json` without disturbing what else is there. The hook is
**silent** when `aval` is not installed, so committing it cannot fail a
colleague's session, and silent when there is no corpus to report. The text it
prints tells the model that a record's wording is data, not instruction.

Repo-specific caveats go in `.claude/aval-hook.local.md`. The hook appends that
file; installing again never touches it. `--check` is the drift detector for
CI: exit 0 wired, exit 1 stale.

The script's second line names the aval that wrote it:

```sh
#!/bin/sh
# aval-hook: written by aval 1.8.0
```

A script from a **newer** aval is not stale, and an older aval leaves it alone
rather than downgrading it, so a workstation that upgrades first does not
redden a CI job pinned to the release before. Both modes also print a `note`
for every `AVAL_VERSION:` pin under `.github/workflows` or `.forgejo/workflows`
that is behind or ahead of the running aval — advisory, never an exit code,
never an edit to the workflow.

### Vendored packs that fell behind

Where the repository [vendors packs](sharing.md), the hook also asks whether
they are still what their sources publish — at most once an hour per
repository, under a five-second budget that kills the remote call rather than
waiting on it, and speaking only when a pack is behind or edited:

```text
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

## The MCP server

The hook pushes; **`aval mcp` lets an agent pull** — the same answers as native
tools, asked at the moment the question comes up rather than only at the top of
a session:

```console
$ claude mcp add aval -- aval mcp
```

Ten read-only tools: `aval_resolve`, `aval_relevant`, `aval_keys`,
`aval_traits`, `aval_heads`, `aval_show`, `aval_history`, `aval_rules`,
`aval_rule` and `aval_repos`, and three resources: `aval://heads`,
`aval://keys` and `aval://repos`. They resolve through the same graph the CLI
does and return the same bytes `--json` would, so the two surfaces cannot drift
apart.

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

## From the parent of all your projects

Where there is no corpus above and several below, the same server is a
**workspace**: every tool takes `repo` to name one. `resolve` and `history`
answer for all of them when it is omitted — a map keyed by name, which is the
cross-repository question in a single call — while `keys`, `heads` and `show`
ask for a name, because every repository's heads at once is a lot of context to
fetch by forgetting an argument. `aval repos` says what was found, and
`--all-repos` renders the same map from the shell:

```console
$ aval resolve stack.sql-layer --scope effect-stack --all-repos
== amont ==
active   decisions:ADR-0002   @effect/sql
  scope: effect-stack
  vendored: from the `decisions` pack; change it there, not here

…

== decisions ==
active   ADR-0002   @effect/sql
  scope: effect-stack
…
```

A linked worktree is detected and left out when its parent is also there, so
one corpus never answers twice; `aval repos` still lists it, and it stays
addressable by name.
