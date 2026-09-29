# aval

**Git is the history. ADRs are the evidence. `aval resolve` tells you what is
true now.**

Most ADR tooling helps you write and browse decision records. `aval` answers the
question a coding agent needs before it writes a line of code — what is decided
here, now — as a verdict with an exit code, and refuses to guess when the
records do not settle it.

It is published and in use: the resolver, the invariants, the projection, the
CLI, vendoring and the tool surface all run against real corpora rather than
fixtures. These pages are the manual. The
[README](https://github.com/fredericrous/aval#readme) is the short version.

- [Installing and gating](install.md) — the binary, and the one line that gates
  a repository.
- [The model](concepts.md) — keys, records, scopes, and the typed non-answers.
- [Commands](commands.md) — every command, option and exit code.
- [Which decisions bear on this change](relevance.md) — `aval relevant`, how it
  ranks, and what a router reads from it.
- [In front of an agent](agents.md) — the session hook and the MCP server.
- [Rules and traits](rules-and-traits.md) — what a decision does not settle, and
  where a rule applies.
- [Sharing one decision across repositories](sharing.md) — packs, vendoring, and
  knowing when one fell behind.
- [Building](building.md) — working on aval itself.

Outside the book, at the repository root:

- [`SEMANTICS.md`](https://github.com/fredericrous/aval/blob/main/SEMANTICS.md) —
  the normative specification. Start there to write records against this.
- [`CHANGELOG.md`](https://github.com/fredericrous/aval/blob/main/CHANGELOG.md) —
  what changed in each release, and what a consumer has to do about it.
