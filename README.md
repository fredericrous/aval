# aval

**Git is the history. ADRs are the evidence. `aval resolve` tells you what is
true now.**

Most ADR tooling helps you write and browse decision records. `aval` answers a
different question, the one a coding agent actually needs before it writes a
line of code: what is decided here, now. The records are the evidence, the
current architecture is computed from them, and the answer is a verdict with an
exit code:

```console
$ aval resolve storage.object-store --scope homelab
active   ADR-0002   Ceph RGW
  scope: homelab
$ echo $?
0
```

and, just as importantly, it refuses to guess when it should not:

```console
$ aval resolve data.realtime-processing
undecided   no accepted decision for data.realtime-processing
$ echo $?
4

$ aval resolve api.gateway
contradiction   2 heads for api.gateway; the corpus disagrees with itself
  ADR-0001  introduced in 8d68d23f
  ADR-0002  introduced in 7941cf9b
  Stop. Do not pick one; write the ADR that replaces both.
$ echo $?
5
```

An agent branches on a number. It never reads three documents and reasons its
way to a plausible wrong answer.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/fredericrous/aval/main/install/install.sh | sh
```

Or `brew install fredericrous/tap/aval`, `cargo install aval`, or
`npx aval-adr`. Windows, version pins and gating a repository with one line of
[amont](https://github.com/fredericrous/amont): [installing and gating](docs/install.md).

## Use

| | |
|---|---|
| `aval resolve <key> [--scope S]` | the authoritative lookup |
| `aval relevant [--path P]… [--text W] [--changed]` | which keys bear on what you are about to touch, ranked, each with its verdict. Advisory: it resolves nothing |
| `aval keys` | the vocabulary: every key, and where it is decided |
| `aval check` | every invariant; what the git hook and CI run |
| `aval heads [--write\|--check]` | the `HEADS.md` projection |
| `aval hook install` | put the heads in front of an agent at session start |
| `aval mcp` | the same answers as read-only MCP tools |
| `aval add <source>` | vendor another repository's decisions |

Every command, option and exit code: [commands](docs/commands.md).

## Documentation

- [Installing and gating](docs/install.md)
- [The model](docs/concepts.md) — keys, records, scopes, and the typed non-answers
- [Commands](docs/commands.md)
- [Which decisions bear on this change](docs/relevance.md) — `aval relevant` and its signals
- [In front of an agent](docs/agents.md) — the session hook and the MCP server
- [Rules and traits](docs/rules-and-traits.md) — what a decision does not settle
- [Sharing one decision across repositories](docs/sharing.md) — packs and vendoring
- [`SEMANTICS.md`](SEMANTICS.md) — the normative specification, and the place to
  start if you intend to write records against this
- [`CHANGELOG.md`](CHANGELOG.md)

The verdicts, their exit codes, the note strings, the `--json` field names and
the frontmatter dialect are stable since 1.0; §15 of `SEMANTICS.md` says exactly
what that covers.

## Building

`make check` runs what CI runs. No external dependencies, by design and
enforced in CI: `aval` runs on the pre-commit path, so it pulls in nothing.
More in [building](docs/building.md).

## License

MIT
