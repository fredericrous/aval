# Conformance

`resolve.json` is the battery. Each case names a corpus under `corpora/`, a
query, and the expected result. The harness rules are in the file's own
`description` header.

`traits.json` is the third: the glob dialect of `areas:` and the
applicability table of SEMANTICS section 2.5, case by case, including the
`omitted` report every filtering surface carries.

`relevant.json` is the second one, for the ranking `aval relevant` produces.
It is a separate file because it asserts a different kind of thing: not what is
true, but what a *suggestion* says about it — that the order is reproducible
byte for byte, and that the verdict beside each ranked key is the resolver's
own, `undecided` and `contradiction` included. Its rules are in its own header.

## The corpora

| Corpus | Exercises |
|---|---|
| `homelab-sample` | the real conversion: global and scoped keys, replacement within a slot, a sibling slot surviving it, retirement, partial supersession, draft inertness |
| `diamond` | two branches replacing one predecessor, then a reconciliation absorbing both |
| `contradiction` | the same corpus without the reconciliation, so competing heads are observable |
| `retire-scoped` | a scope opting out of an inherited default |
| `rules-sample` | rules: two constraints and a heuristic under a live record, one under a draft, one under a replaced record — so every reason a rule can be inactive has an instance |
| `traits-sample` | traits: one rule per targeting shape (a trait, another trait, a trait no area names, `applies: []`, no `applies`) and one area per row of the applicability table — overlapping, `[]`, uncovered |
| `traits-none` | the same rules with no `areas`, so nothing is filtered and nothing reports that it was |
| `relevance-sample` | relevance: a replaced record whose title still says what the key is about, a record naming a literal path and a glob, a key decided only at a scope, a key declared and undecided, and a rule to rank beside them |

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
