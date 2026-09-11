# Conformance

`resolve.json` is the battery. Each case names a corpus under `corpora/`, a
query, and the expected result. The harness rules are in the file's own
`description` header.

## The corpora

| Corpus | Exercises |
|---|---|
| `homelab-sample` | the real conversion: global and scoped keys, replacement within a slot, a sibling slot surviving it, retirement, partial supersession, draft inertness |
| `diamond` | two branches replacing one predecessor, then a reconciliation absorbing both |
| `contradiction` | the same corpus without the reconciliation, so competing heads are observable |
| `retire-scoped` | a scope opting out of an inherited default |

`homelab-sample` is converted from `homelab/docs/adr`. The frontmatter is a
faithful conversion of what each ADR decides; the bodies are stubs, because this
is a fixture corpus and not a copy of the documents. Three of its ADRs
(`0021`, `0022`, `0023`) do not exist upstream: they are the successors Phase 3
will write, needed here to exercise replacement, partial supersession and draft
inertness.

## What converting the real corpus taught us

**The corpus contains no scoped choice overriding a global default.** Every
apparent cloud override in ADR-0015 is a first decision in a slot nothing had
occupied, because the earlier ADRs were themselves cluster-specific. ADR-0015
says so in its own prose: "homelab's native routing needs a shared L2 segment",
and "ADR-0004 anchors homelab's **and monitor's** seal on the NAS Vault".

Two consequences:

1. `overrides` on a *choice* entry is advisory and currently has no instance in
   the corpus. Keep it, because it costs nothing and documents intent, but do
   not treat it as proven.
2. `overrides` on a *retirement* is load-bearing and has no substitute. It is
   the only way to say "this scope deliberately has none of the thing everyone
   else inherits". That case surfaced only because building `retire-scoped`
   found that a scoped retirement has no same-slot predecessor to name. See
   SEMANTICS section 6.1.

**Fallback runs one way.** `gitops.registry-mirror` is decided separately for
four clusters and globally for none, so resolving it without a scope is
`undecided`. A model that inferred a global answer from unanimous scoped ones
would have invented a decision nobody made.
