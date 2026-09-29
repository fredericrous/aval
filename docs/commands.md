# Commands

`aval --help` is the authoritative list; this page is the same thing with room
to say what each one is for.

## Asking

| | |
|---|---|
| `aval resolve <key> [--scope S]` | the authoritative lookup: what is decided, and nothing else |
| `aval relevant [--path P]… [--text W] [--changed] [--top N] [--scope S]` | which keys bear on what you are about to touch, ranked, each with its verdict. Advisory: it resolves nothing. See [which decisions bear on this change](relevance.md) |
| `aval keys [--names]` | the vocabulary: every key, where it is answerable, where it is decided; `--names` leaves out where it is decided |
| `aval show <ADR-NNNN>` | derived status of one record, including partial supersession |
| `aval history <key> [--scope S]` | the chain, labelled as history rather than authority |
| `aval heads [--write\|--check]` | the projection; `--write` regenerates `HEADS.md`, `--check` says whether it is stale |
| `aval rules [--level L] [--adopted-by R] [--all] [--all-traits]` | the rules the decisions have adopted, one line each. See [rules and traits](rules-and-traits.md) |
| `aval rule <id>` | one rule, with its translation and why |
| `aval traits [--summary\|--detect\|--check]` | what this repository says it is, and what its tracked files suggest |

## Gating

| | |
|---|---|
| `aval check` | every invariant; what the git hook and CI run |
| `aval migrate <dir>` | report what converting a legacy ADR directory needs. It writes nothing |

## Agents

| | |
|---|---|
| `aval hook install [--check]` | put the heads in front of an agent at session start; `--check` is the drift detector for CI |
| `aval mcp` | serve the corpus as read-only MCP tools on stdio, until stdin closes |
| `aval repos` | what is answering from here: this corpus, or every corpus one level down when there is none above |

Both are described in [in front of an agent](agents.md).

## Sharing

| | |
|---|---|
| `aval pack [--write\|--check]` | publish this corpus's declarations for others to read |
| `aval add <source>… [--as NAME] [--dry-run]` | vendor another repository's declarations |
| `aval add --check [--quiet] [--budget S]` | are the vendored packs still what their revisions name; `--quiet` speaks only when one is not, `--budget` kills a remote call that has not answered in S seconds |

A source is `github:owner/repo`, `forgejo:host/owner/repo`, a git URL or a
path, each optionally `@<rev>`; the commit id is what gets recorded. See
[sharing one decision across repositories](sharing.md).

## Options every command takes

| | |
|---|---|
| `--json` | machine output on stdout, warnings suppressed |
| `-C <dir>` | run as if started in `<dir>` |
| `--all-repos` | on `resolve`, `keys`, `heads`, `history` and `rules`: ask every corpus `aval repos` lists, and report them keyed by name |

## Exit codes

```text
resolve  0 active · 4 undecided · 5 contradiction · 6 retired · 7 unknown
relevant 0 always · 2 usage, including an undeclared --scope
rule     0 found · 7 unknown
traits   --check: 0 nothing to report · 1 findings · 3 could not inspect
others   0 ok · 1 findings or stale
always   1 tool failure · 2 usage · 3 unreadable or invalid corpus
mcp      0 stdin closed · 1 transport failure · 2 usage. Never 3: a
         corpus that will not load is reported in the tool result.
repos    0 · 2 usage · 3 nothing found, or the directory unreadable
```

`--all-repos` exits `0` when every member loaded and none contradicts itself,
`1` when one does or would not load, and `3` when none loaded. The full
per-command contract is §14 of
[`SEMANTICS.md`](https://github.com/fredericrous/aval/blob/main/SEMANTICS.md#14-exit-codes).
