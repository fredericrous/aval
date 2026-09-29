# The model

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

The registry, `.adr.yaml`, declares the vocabulary — keys, scopes, where the
records live — and never an answer. Anything that changes what `resolve`
returns belongs in a record, because a record is where a decision is reviewed:

```yaml
dir: docs/adr
scopes: [homelab, monitor, nas, cloud]
keys:
  storage.object-store:
    description: Canonical S3-compatible object store
  cni.routing-mode:
    description: Cilium datapath routing mode
```

A specification that carries decisions can be named where it is rather than
moved:

```yaml
dir: docs/adr
sources:
  - docs/spec-change-proposals.md
```

Literal paths, not patterns: a listed file that goes missing is an error, where
a pattern that stops matching would drop the record and let a superseded
decision come back as the current one.

## Typed non-answers

The design is mostly about what happens when there is no clean answer, because
that is where an agent left to its own judgment does damage.

| Exit | Verdict | Means |
|---|---|---|
| 0 | `active` | here is the decision |
| 4 | `undecided` | the key exists, nothing decided it |
| 5 | `contradiction` | competing heads. Stop, do not pick one. |
| 6 | `retired` | a record deliberately retired this key |
| 7 | `unknown` | no such key or scope, or a key not decided on that axis |

```console
$ aval resolve api.gateway
contradiction   2 heads for api.gateway; the corpus disagrees with itself
  ADR-0001  introduced in 8d68d23f
  ADR-0002  introduced in 7941cf9b
  Stop. Do not pick one; write the ADR that replaces both.
$ echo $?
5
```

Codes `1`, `2` and `3` mean the tool failed, was misused, or could not read the
corpus. They never overlap a verdict, so "I could not look" is never mistaken
for "I looked and found nothing". Every other command's contract is in
[the command reference](commands.md#exit-codes), and all of them in §14 of
[`SEMANTICS.md`](https://github.com/fredericrous/aval/blob/main/SEMANTICS.md#14-exit-codes).

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

A question asked at a scope that decides nothing falls back to the default
scope, `*`. A key may declare the scopes it is decided along, so that asking it
at a scope from another axis — a cluster name for a question about stack
families — answers `unknown` instead of falling back to an answer about
something else.

## Stability

The verdicts, their exit codes, the note strings, the `--json` field names and
the frontmatter dialect are stable since 1.0; §15 of
[`SEMANTICS.md`](https://github.com/fredericrous/aval/blob/main/SEMANTICS.md#15-versioning)
says exactly what that covers, and what it deliberately does not.
[`SEMANTICS.md`](https://github.com/fredericrous/aval/blob/main/SEMANTICS.md)
is normative and is the place to start if you intend to write records against
this.
