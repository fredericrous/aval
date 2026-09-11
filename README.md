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
| `aval check` | every invariant; what the git hook and CI run |
| `aval heads [--write\|--check]` | the projection |
| `aval show ADR-0015` | derived status, including partial supersession |
| `aval history <key>` | the chain, labelled as history |

## Status

Pre-alpha, and not yet published. The resolver, the invariants, the projection
and the CLI work; the corpus it was built against is a fixture, not a
production one. [`SEMANTICS.md`](SEMANTICS.md) is normative and is the place to
start.

Still to come: converting a real corpus, the git-hook gate, the agent surfaces,
and a 1.0.

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
