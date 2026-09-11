# `aval` — normative semantics

Version 0.3.1. This document is the specification. Where an
implementation and this document disagree, this document is right and the
implementation is a bug.

`aval` answers one question: **what is the current decision for a given
decision key?** It derives that answer from a corpus of Architecture Decision
Records. It never stores the answer.

---

## 1. Model

### 1.1 Key

A **decision key** is a dotted lowercase identifier naming one architectural
question, for example `storage.object-store` or `cni.routing-mode`. Keys are a
controlled vocabulary: every key used by any ADR MUST be declared in the
registry. A key names the *question*, never the answer.

### 1.2 Scope

A **scope** is a named partition of the estate, for example `homelab` or
`cloud`, plus the distinguished **default scope** written `*`.

Scopes are **flat**. There is exactly one level of named scope and it does not
nest; `*` is not a parent that can be subclassed further. This is a deliberate
limit (§12.1).

### 1.3 Slot

A **slot** is a pair `(key, scope)`. Resolution answers questions about slots.
`(storage.object-store, homelab)` and `(storage.object-store, nas)` are
different slots and may hold different answers at the same time.

### 1.4 Entry

An **entry** is one decision that one ADR makes, identified by
`(adr_id, key, scope)`. Entries are the nodes of the decision graph.

An ADR MAY contain many entries. An ADR MUST contain **at most one entry per
slot**. This rule is what makes a bare `ADR-NNNN` cross-reference unambiguous
(§3.3), so it is structural, not stylistic.

An entry is one of two kinds:

- a **choice entry**, carrying `choice`: this is the answer for that slot.
- a **retirement entry**, carrying `retire: true`: the slot deliberately has no
  answer (§6).

An entry MUST NOT be both.

---

## 2. Registry

The registry declares vocabulary. It MUST NOT contain decisions, answers,
statuses, or retirements. Anything that changes what `resolve` returns belongs
in an ADR, because an ADR is where a decision is reviewed.

```yaml
dir: docs/adr
scopes: [homelab, monitor, nas, cloud]
keys:
  storage.object-store:
    description: Canonical S3-compatible object store
  cni.routing-mode:
    description: Cilium datapath routing mode
```

- `dir` — where numbered ADR files live, relative to the registry. Also where
  `HEADS.md` is written.
- `sources` — additional records, named outright (§2.2). Optional.
- `scopes` — the closed scope vocabulary. `*` is always valid and MUST NOT be
  listed.
- `keys` — the closed key vocabulary. `description` is for humans and for the
  did-you-mean index; it carries no semantics.

### 2.1 Per-key scopes

A key MAY declare the scopes it is decided along:

```yaml
scopes: [homelab, cloud, effect-stack]
keys:
  cni.routing-mode:
    description: Cilium datapath routing mode
    scopes: [homelab, cloud]
  stack.sql-layer:
    description: How application code reaches SQL
    scopes: [effect-stack]
```

This exists because a scope list can span more than one axis. Clusters,
landscapes and stack families are all scopes, and they are not interchangeable.
With one flat list, `scope-declared` accepts `cni.routing-mode@effect-stack`,
and §5 fallback then answers it from the default scope — an `active` verdict
about a different axis, which reads as agreement.

- A key with **no** `scopes` field accepts every declared scope. A registry
  written before this field existed keeps its meaning exactly.
- An **empty** list is not the same as absent. It says the key is decided at the
  default scope only.
- Every scope a key names MUST be declared in the registry's own `scopes`. A key
  MUST NOT list `*`.
- The **default scope is always admitted**, whatever the key declares.
  Restricting a key MUST NOT sever its own §5 fallback.
- An entry deciding a key at a scope the key does not admit is a Layer A error
  (`scope-applies`). A **query** at such a scope returns `unknown` (exit 7) and
  MUST NOT fall back; the question names the wrong axis rather than going
  unanswered.

### 2.2 Records outside `dir`

A decision is often stated in a document that is not an ADR file: a
specification, a plan, a design note. Renaming and renumbering such a document
to bring it into the corpus breaks every reference to it, so the registry may
name it where it is.

```yaml
dir: docs/adr
sources:
  - docs/spec-change-proposals.md
  - docs/plan-ia-coherence.md
```

- Entries are **literal repository-relative paths**. They MUST NOT be patterns.
  A pattern that stops matching removes a record silently — its entries leave
  the graph, whatever it superseded returns as a head, and §5 answers `active`
  with a decision that was replaced. A pattern slightly too wide has the
  opposite failure and captures unrelated frontmatter. A listed file that is
  missing or unreadable is an error instead, and that is the point.
- `sources` **supplements** `dir`. It never replaces it.
- A file reachable by both rules is read once. Deduplication happens before
  parsing, or one file becomes two records and `id-unique` reports it against
  itself. **The `dir` rule wins**, so mandatory frontmatter is never traded
  away by also listing a file.
- An absent `sources`, and `sources: []`, mean the same thing.
- With `sources` declared, `dir` need not exist on disk. `heads --write` needs
  it and says so.

Both shapes are first class and the choice is a repository's to make. A record
that lives in its own file is superseded rather than edited, so its history is
in the graph; a specification carrying its decisions is rewritten in place, so
its history is in git. Neither is wrong, and this document does not choose.

---

## 3. ADR format

### 3.1 Frontmatter

An ADR is a markdown file whose first line is `---`, followed by a YAML
frontmatter block, followed by the document body.

```yaml
---
id: ADR-0015
status: accepted
decisions:
  - key: topology.cloud-cluster
    choice: 3x OVH VPS-2 Roubaix, Talos, cluster id 4
    first: true

  - key: cni.routing-mode
    scope: cloud
    choice: VXLAN tunnel + WireGuard
    first: true
    overrides: ADR-0006

  - key: dns.dynamic-record-updater
    retire: true
    replaces: [ADR-0011]
    reason: WAN moved behind CGNAT; there is no public IP to update
---
```

Note the second entry carries **both** `first: true` and `overrides`. It is the
first entry in the slot `(cni.routing-mode, cloud)` — nothing there to replace —
while declaring that it knowingly diverges from the global default. §3.4 and
§3.5 explain why both are needed.

### 3.2 Fields

| Field | Where | Required | Meaning |
|---|---|---|---|
| `id` | document | yes | `ADR-NNNN` matching the filename's numeric prefix for a record under `dir`; a slug for one named in `sources` (§3.8). |
| `status` | document | yes | `draft` or `accepted`. See §7. |
| `decisions` | document | yes | a **list** of entries. May be empty. |
| `key` | entry | yes | MUST be declared in the registry |
| `scope` | entry | no | MUST be declared in the registry. Absent means `*`. |
| `choice` | entry | see §1.4 | short human answer, for `heads` and `resolve`. One line: §12.2. |
| `retire` | entry | see §1.4 | `true` to declare the slot deliberately empty |
| `first` | entry | see §3.4 | `true` when nothing precedes this in the slot |
| `replaces` | entry | see §3.4 | list of ADR ids (§3.3) |
| `overrides` | entry | no | one ADR id (§3.5) |
| `reason` | entry | no | free prose, carried into output |

`decisions` is a **list, not a map**, so one ADR can decide the same key at two
scopes. A map keyed by `key` could not: YAML forbids duplicate keys.

**`date` is deliberately absent from the model.** Ordering comes from edges,
never from dates. Dates remain in the document body for humans.

Unknown fields are a structural error (§9, Layer A), not a warning. Silently
ignoring a misspelled `replacess:` would drop a supersession edge.

### 3.3 Cross-references

A reference is a bare ADR id, `ADR-0006`.

- Inside `replaces`, it resolves to **that ADR's entry for the same key at the
  same scope as the referring entry**.
- Inside `overrides`, it resolves to **that ADR's entry for the same key at
  scope `*`**.

Both are unambiguous because an ADR holds at most one entry per slot (§1.4).
This is why that rule is structural.

A reference whose target entry does not exist is a structural error.

### 3.4 Predecessor declaration

Every entry MUST carry **exactly one** of:

- `first: true` — no accepted entry precedes this one in this slot; or
- `replaces: [...]` — a non-empty list of ADR ids.

Neither is not allowed, and both is not allowed.

This requirement is load-bearing rather than tidy. cluster-vision's
`app_dependencies` table accumulates monotonically because nothing ever deletes
an edge, so a wrong inference never expires. A decision graph in which
supersession is optional has precisely that failure mode. Requiring an explicit
`first: true` means "this is new" is a claim someone wrote down and a reviewer
saw, not the default that silence produces.

### 3.5 The two edges

This is the central distinction in the model, and getting it wrong breaks
correctness rather than tidiness.

**`replaces` operates within one slot.** Each listed ADR must hold an entry in
the *same slot* as the referring entry. Those entries stop being heads.
**`replaces` is the only edge that affects resolution.**

**`overrides` operates across slots.** It declares that a scoped entry
deliberately diverges from that ADR's entry at scope `*`. It creates no
supersession edge, removes no head, and has no effect on any resolution. It
exists so that divergence is explicit to a reviewer and checkable by a tool.

The reason for the split is concrete. ADR-0015 chooses VXLAN plus WireGuard for
the cloud cluster, while homelab must keep the native routing ADR-0006 chose. A
single supersession edge from the cloud entry to ADR-0006 would remove the
global head and silently change homelab's answer. Typing the two relations
separately makes that impossible to express by accident.

Two consequences worth stating, because both were open questions:

1. **A later change to the global default leaves scoped overrides standing.**
   Replacing ADR-0006's entry acts only on the slot `(cni.routing-mode, *)`. The
   cloud entry is a head of a different slot and is untouched.
2. **`overrides` never goes stale.** It references an entry, which remains a
   document fact forever, even once that entry is superseded.

`overrides` MUST NOT appear on an entry whose own scope is `*`, since there is
nothing broader to diverge from. It MAY appear on a retirement entry, and there
it is load-bearing rather than advisory (§6.1).

> **Empirically unproven on a choice entry.** Converting the homelab corpus
> found no case of a scoped choice diverging from a global default: the earlier
> ADRs were themselves cluster-scoped, so ADR-0015's cloud decisions are first
> entries in empty slots, not overrides. The field is kept because it documents
> intent at no cost, but its only demonstrated use is §6.1.

### 3.6 Reconciling divergent branches

`replaces` is a list so that a reconciliation ADR can absorb competing heads
without rewriting accepted history:

```yaml
  - key: api.gateway
    choice: Apache APISIX
    replaces: [ADR-0062, ADR-0071]
```

Parallel `git worktree` checkouts make competing heads routine here, so this is
an expected workflow, not a repair for a mistake.

---

### 3.7 The frontmatter dialect

Frontmatter is written in a restricted subset of YAML, not in YAML. The subset
is what a decision record needs and no more:

| Supported | Example |
|---|---|
| block mappings, nested by indentation | `keys:` then an indented `a.b:` |
| block sequences of mappings or scalars | `decisions:` then `  - key: a.b` |
| flow sequences of scalars | `replaces: [ADR-0009, ADR-0011]` |
| block scalars, `\|` and `>`, with `-` or `+` chomping | `reason: >-` |
| plain, single-quoted and double-quoted scalars | `choice: Ceph RGW` |
| `true` and `false` | `first: true` |
| comments, whole-line or trailing | `status: accepted  # for now` |

Everything else is **rejected with a line number**, never guessed at: anchors
and aliases, tags, nested flow collections, multiple documents, and tabs used
for indentation. A construct this parser misread would move a decision head
without saying so, which is the failure the whole tool exists to prevent, so
the parser refuses rather than approximates.

Three rules that differ from YAML proper, each deliberate:

- **Duplicate mapping keys are an error**, not last-wins. YAML's own last-wins
  rule is exactly how `decisions` would have silently lost an entry had it been
  a mapping rather than a list.
- **`#` opens a comment only after whitespace**, so `choice: build#42` keeps its
  hash.
- **Only the first `: ` splits a key from its value**, so a plain value may
  contain a colon (`choice: Kafka: the sequel` reads as written). Quoting works
  too, and is clearer.

Unknown fields are rejected too (§3.2). A misspelled `replacess:` that parsed
into nothing would drop a supersession edge.

### 3.8 Identity

How a record was **found** decides how its `id` is judged. Not whether its
filename starts with digits: an ordinary `docs/specs/2024-payments.md` would
otherwise be required to call itself `ADR-2024`.

- Found under `dir` — `id` MUST be `ADR-<digits>` matching the filename's
  numeric prefix.
- Named in `sources` — `id` is a slug: a letter, then letters, digits, `.`,
  `_` or `-`, at most 64 characters. It MUST NOT begin `ADR-`, which would
  name a numbered record that no numbered file backs and make §3.3's reference
  grammar stop describing reality.

Ids are unique across the corpus however they were formed, and every reference
resolves by id alone (§3.3). A record's **file** is identified by a
repository-relative path rather than a basename, because two sources may hold
the same filename.

---

## 4. Heads

Let *E* be the set of entries at a slot.

An entry is **accepted** when its document's `status` is `accepted`.

An entry *e* is **replaced** when some **accepted** entry in the same slot lists
*e*'s ADR in its `replaces`. A `replaces` edge on a *draft* entry is inert
(§7).

An entry is a **head** when it is accepted and not replaced.

A slot is **occupied** when it holds at least one accepted entry, head or not.
A slot holding only draft entries is **not** occupied.

> **Derived property.** A slot that is occupied always has at least one head.
> An occupied slot with zero heads requires a replacement cycle, which Layer A
> rejects. An implementation reaching that state MUST report a structural error
> (exit 3), never a verdict.

---

## 5. Resolution

```
resolve(key, scope):

  if key   ∉ registry.keys           → unknown        (exit 7)
  if scope ∉ registry.scopes ∪ {*}   → unknown        (exit 7)
  if ¬ key.admits(scope)             → unknown        (exit 7)   // §2.1

  H ← heads(key, scope)

  if |H| > 1                         → contradiction  (exit 5)
  if |H| = 1                         → H₀.retire ? retired (exit 6)
                                                     : active  (exit 0)

  // |H| = 0, so the slot is unoccupied (§4)
  if scope ≠ *                       → resolve(key, *)
  otherwise                          → undecided      (exit 4)
```

Three properties this pins down:

**An unknown scope is rejected, never absorbed.** Without the second guard,
`--scope clodu` silently returns the global default, which is exactly the
failure mode typed non-answers exist to prevent. The did-you-mean in the exit-7
payload covers both keys and scopes, and it is **advisory**: a caller MUST NOT
correct a rejected name and proceed. The third guard is the same argument one
level in: a scope can be perfectly well declared and still be the wrong axis for
the key being asked about, and absorbing that into the fallback would answer
with an unrelated decision. Its exit-7 payload carries `applies_to` — the axis
the caller should have asked on — in place of a did-you-mean.

**Retirement blocks fallback.** A retirement is a head, so a retired slot
returns at `|H| = 1` and never reaches the fallback branch. This is deliberate:
retiring a key for the cloud scope must not silently resurrect the global
answer there.

**Fallback requires an unoccupied slot, not merely an empty head set.** Since an
occupied slot always has a head, the fallback branch is reachable only when
nothing accepted has ever occupied the slot.

`resolve` reports which slot answered, so a caller can tell a scope-specific
answer from an inherited default.

---

## 6. Retirement

Retirement is an ordinary entry, not a registry flag:

```yaml
  - key: dns.dynamic-record-updater
    retire: true
    replaces: [ADR-0011]
    reason: WAN moved behind CGNAT; there is no public IP to update
```

Everything about retirement follows from that one decision:

- **Attribution is structural.** A retirement is an entry in an ADR, so it
  always has an owning ADR and a review. An unattributed retirement is
  inexpressible.
- **Retirement is scoped**, because an entry occupies a slot.
- **Retirement blocks fallback** (§5).
- **A retirement MUST name what it retires** (§6.1). You retire a standing
  decision. Where nothing stands, the answer is `undecided`, which is a
  different statement.
- **Reactivation is an ordinary choice entry** whose `replaces` names the
  retirement's ADR. This is not hypothetical:
  `homelab/docs/adr/0011-ddns-updater-operator.md` retires the DDNS operator and
  says in the same breath to revisit if a public IP returns.

### 6.1 What a retirement names

A slot can hold an answer in two ways: an entry of its own, or an inherited one
reached by fallback from `*` (§5). Retirement must be able to cancel either, so
the predecessor rule has two branches:

| Slot state | Required | Meaning |
|---|---|---|
| occupied | `replaces: [...]` | retires this slot's own standing decision |
| unoccupied, and `(key, *)` is occupied | `first: true` **and** `overrides: ADR-NNNN` | opts this scope out of the inherited default |
| unoccupied, and `(key, *)` is unoccupied | **rejected** | nothing stands; the answer is already `undecided` |

The second row is why `overrides` exists at all. Its advisory use on a choice
entry documents divergence; its use on a retirement is the only way to express
"this scope deliberately has none of the thing everyone else has", and there is
no other syntax for it.

`retired` is a distinct verdict from `undecided` because the fleet rule is that
a negative verdict requires positive evidence. `feed_probe.py` states it
directly: `FAIL` needs an explicit denial, and absence of evidence is always
`INCONCLUSIVE`. Here, a key is retired only when an ADR says so, never because
nothing decided it.

---

## 7. Draft and accepted

`status` records **whether the decision was approved**. It does **not** record
whether the decision was implemented, and an implementation MUST NOT infer one
from the other.

This is the declared/observed boundary that
`application-landscape/docs/adr-cluster-vision-federation.md` establishes for
this fleet: intent and evidence stay physically separated. Promoting an ADR
from draft to accepted because the work shipped imports observed state into the
declared model. Implementation progress belongs in the document body.

Draft behaviour:

- A draft entry is **never a head**, and is invisible to `resolve` and `heads`.
- A draft entry's `replaces` edges are **inert**. A draft cannot demote an
  accepted decision, so opening a proposal never changes the current answer.
- An accepted entry MUST NOT `replaces` a draft entry. There is nothing to
  replace, and allowing it would let acceptance order change resolution.

Nothing ever writes a status of `superseded`. That is derived (§8).

---

## 8. Derived status

Per entry: **head** or **superseded**, per §4.

Per document:

| Derived | Condition |
|---|---|
| `draft` | `status: draft` |
| `active` | every accepted entry is a head |
| `partially superseded` | at least one entry is a head and at least one is not |
| `superseded` | the document has accepted entries and none is a head |
| `empty` | the document has no entries |

**Partial supersession is the normal case, not an edge case.** ADR-0014 decides
dual forge provisioning, SSH-key ownership, and the forge username claim. Only
the first has changed. `show` MUST report per-entry status and MUST NOT collapse
a document to a single word when its entries disagree, because "superseding
ADR-0014" would otherwise silently discard two live decisions.

---

## 9. Three layers

These layers exist because conflating them makes verdicts unreachable. A
resolver that runs undifferentiated validation before answering can only ever
report a structural error, so `contradiction` would never be observable.

**Layer A — structural validity.** The graph cannot be built. See §10.

**Layer B — resolution verdicts.** Given a valid graph: `active`, `undecided`,
`retired`, `contradiction`. **Competing heads are Layer B, not Layer A.** The
graph is well formed; the corpus disagrees with itself, which is a fact about
the decisions, not about the files.

**Layer C — repository hygiene.** Facts about the repository around the graph.
**Layer C MUST NOT affect `resolve` or `heads --write`.** A stale `HEADS.md`
must not prevent regenerating `HEADS.md`, and a dead citation must not prevent
answering a question.

---

## 10. Invariants

### Layer A — structural

| Check | Rejects |
|---|---|
| `frontmatter-parses` | absent or malformed frontmatter, unknown field |
| `id-unique` | two documents claiming one id |
| `id-matches-filename` | `id` disagreeing with the filename prefix, or an unusable slug (§3.8) |
| `key-registered` | a key absent from the registry |
| `scope-declared` | a scope absent from the registry |
| `scope-applies` | a declared scope outside the key's own `scopes` list (§2.1) |
| `one-entry-per-slot-per-adr` | two entries for one slot in one document |
| `entry-kind-exclusive` | an entry both choosing and retiring, or neither |
| `predecessor-declared` | neither or both of `first` and `replaces` (§3.4) |
| `edge-resolves` | `replaces` or `overrides` naming a missing entry (§3.3) |
| `no-cycle` | a replacement cycle within a slot |
| `retire-names-predecessor` | a retirement not matching either branch of §6.1 |
| `no-accepted-replaces-draft` | an accepted entry replacing a draft entry (§7) |
| `overrides-well-placed` | `overrides` on a `*`-scoped entry (§3.5) |

### Layer B — verdicts

| Check | Reports |
|---|---|
| `single-head` | a slot with more than one head |

### Layer C — hygiene

| Check | Reports |
|---|---|
| `heads-fresh` | `HEADS.md` not matching the projection after canonicalisation (§12.2) |
| `links-resolve` | an unpinned citation that does not resolve (§11) |
| `status-single-source` | a prose status line claiming approval the frontmatter already owns |
| `no-manual-index` | a hand-maintained ADR index table |
| `override-undeclared` | a scoped entry diverging from a global head without `overrides` |

### 10.1 Two checks the model makes unnecessary

Recorded because both appeared in the design that preceded this spec, and
removing them is a property of the model rather than an omission.

**`no-orphan-key` is gone.** It guarded whole-document supersession, where
"ADR-0042 supersedes ADR-0017" silently dropped any key ADR-0017 decided that
ADR-0042 did not. With per-slot edges a replacement acts on exactly one slot and
every other entry in the replaced document remains a head. A key cannot lose its
decision silently, so there is nothing to check.

**`retired-attributable` is gone.** It verified that a registry `retired_by`
pointed at an ADR that really retired the key. Retirement now lives in the ADR
(§6), so an unattributed retirement cannot be written down.

### 10.2 `override-undeclared` is advisory

`overrides` cannot be mandatory. A scoped entry may legitimately predate the
global entry it now diverges from, and making it required would let a new global
decision retroactively invalidate existing documents. So divergence without an
`overrides` declaration is reported as hygiene, not rejected as structure.

---

## 11. Citations

An append-only historical ADR can **correctly** cite a component that was later
deleted. Requiring every citation to resolve against the current tree would
reward deleting the evidence, which inverts the purpose of the corpus.

Two kinds:

- **Live** — a bare repository-relative path. Checked against the tree.
- **Pinned** — a path carrying `@<rev>`. Recorded as historical; never checked
  against the tree.

**When a live citation goes dangling, the repair is to pin it, not to delete
it.** That converts link rot into provenance.

### 11.1 What is checked

Running this against a real corpus is what fixed the list. The first draft
reported roughly a hundred dangling links in a sixteen-document corpus, of which
six were real. A check with that ratio is switched off within a week, and then
it catches nothing at all, so the conservative half of this list carries as much
weight as the other.

**Checked:**

- A markdown link, `[text](target)`, whose target is repository-relative.
  Resolved against the **document**, per the markdown standard. Resolving these
  against the repository root instead made every sibling ADR cross-reference
  read as dangling.
- A backticked bare token containing `/` and no whitespace, **when its first
  segment is an existing top-level entry of the repository**. Resolved against
  the repository root.

**Not checked**, each for a reason a real citation supplied:

| Skipped | Example from the corpus |
|---|---|
| URLs and bare fragments | `https://…`, `#section` |
| globs, including brace expansion | `bootstrap/configs/{homelab,monitor,nas}.yaml` |
| anything carrying `@<rev>` | a pinned historical citation |
| absolute paths | `/livez`, `/var/lib/llamacpp-models` |
| CIDR blocks | `10.244.0.0/16` |
| host-rooted references | `ghcr.io/fredericrous/homelab/manifests:latest` |
| paths git ignores | `infrastructure/homelab/terraform.tfvars` |
| paths inside a gitlink | `vault-transit-unseal-operator/docs/…` |
| citations in a **draft** | a draft proposes files; that is intent, not evidence |

A trailing `:line` or `:line-line` addresses a place inside a file and is
trimmed before the file is checked. A trailing `#fragment` likewise.

The first-segment rule is what separates a repository path from a URL path, a
Vault path, a container path, a path into a *different* repository, or a forge
slug — all of which look identical to one. Its cost is a known blind spot: a
citation whose entire top-level directory was removed, `.github/workflows/x.yaml`
after CI moved to `.forgejo/`, is skipped rather than reported.

## 12. Determinism

- **The core is pure: no clocks, no I/O, no git.** It takes a parsed corpus and
  returns verdicts. The binary reads files and enriches provenance. This is what
  makes the conformance battery deterministic, and it is a second reason `date`
  is absent from the model.
- **A verdict carries a machine token and a stable human note.** Note strings
  are part of the contract and are asserted by fixtures, so output cannot drift
  silently. Changing a note string is a breaking change (§13).
- **`heads` output is sorted and carries no timestamp**, so it is a pure
  function of the graph. It renders only heads, so a superseded entry can never
  appear in it. On a Layer A failure it renders **nothing** rather than
  publishing a partial or false current state. What CI compares is the file
  after canonicalisation (§12.2), not its bytes.
- **`--json` writes the result object to stdout and nothing else.** Warnings go
  to stderr and are suppressed under `--json`.

### 12.2 Freshness is a claim about content

`HEADS.md` lives in repositories that run a markdown formatter. Both the file
and the projection are **canonicalised** before comparison, so a formatted
projection is current and a content-changing edit is not. Adjusting padding by
hand stays valid; that is the whole point.

Canonicalisation normalises exactly these, and nothing else:

| Normalised | |
|---|---|
| `CRLF` → `LF` | otherwise a checkout that converts line endings could never converge |
| a leading byte-order mark | |
| trailing whitespace on a line | |
| trailing blank lines at end of file | |
| padding inside a table cell | |
| the delimiter row's style, including alignment markers | |

Everything else survives into the comparison. A row added, removed, reordered
or altered is stale; so is an edited banner, an edited heading, or added prose.

This is a whitelist and MUST NOT be implemented as a parser that compares
modelled rows. A reader ignores what it does not model, so prose added under
the banner would compare equal and pass indefinitely.

**Rows compare as an ordered sequence.** The projection is sorted, and nothing
else would enforce that if a reordering counted as current.

Because the same function is applied to both sides, it never has to decode an
escape correctly. It follows that `choice` and `reason` MUST be single-line: a
table row has no escape for a newline, and one would split a record across
several rows. That is a Layer A error, reported with its line.

`heads --write` leaves a file alone when it already states the projection, so a
formatter's output is not undone on every run. **Anything not already current
is overwritten**, including a file that cannot be read as a projection at all,
so §9's guarantee holds: there is no state `--write` cannot repair.

### 12.1 Deliberate limits

- **Scopes do not nest.** The estate has four sibling clusters. Nesting would
  require a precedence rule between partial matches and buys nothing today.
- **No semantic search.** Discovery by similarity is useful and is not
  authority. Only an exact key resolves.
- **No drift detection, waivers, or expiry.** application-landscape owns those.
  Duplicating them here would fork the metamodel its federation ADR exists to
  protect.

---

## 13. Provenance

A contradiction should say which commit introduced each competing head. But a
staged ADR has no commit, a shallow checkout has no history, and an existing
document can gain an entry long after the file was created.

Therefore provenance:

- attributes **the entry**, not the file;
- is computed by the binary, never the core;
- has three explicit states: `committed` (with the commit that introduced the
  entry), `uncommitted`, and `unavailable`.

**Missing git history MUST NOT turn a detectable contradiction into a tool
failure.** The verdict and its exit code are unchanged; provenance is simply
reported as `unavailable`.

---

## 14. Exit codes

The governing rule, which every CLI in this fleet states in different words: **a
code meaning "I could not reach a verdict" never shares a range with a verdict.**
amont puts it as "a command that never ran has not judged anything".
`feed_probe.py` annotates its exit 3 as "*not a feed verdict*".

Low codes follow the duro CLI. Verdicts start at 4.

| Code | Kind | Meaning |
|---|---|---|
| 0 | verdict | `active` |
| 1 | failure | the tool itself failed |
| 2 | failure | usage error |
| 3 | failure | unreadable corpus or registry, or Layer A failure |
| 4 | verdict | `undecided` |
| 5 | verdict | `contradiction` |
| 6 | verdict | `retired` |
| 7 | verdict | `unknown` key, scope, or key-scope pairing (§2.1) |

**Each command has its own contract.** The table above is `resolve`'s.

| Command | Layers | Exit codes |
|---|---|---|
| `aval check` | A + B + C | `0` clean · `1` findings · `2` usage · `3` unreadable |
| `aval resolve` | A then B | the full table above |
| `aval heads --write` | A | `0` · `1` write failed · `2` · `3` |
| `aval heads --check` | A + C | `0` fresh · `1` stale · `2` · `3` |
| `aval show` | A | `0` · `2` · `3` · `7` |
| `aval history` | A | `0` · `2` · `3` · `7` |

`check` deliberately reports `1` for any finding regardless of layer, because
its caller is a git hook, where amont's contract is that `0` passes and anything
else fails.

---

## 15. Versioning

This document is versioned with the tool.

- Changing a verdict for an unchanged corpus, changing the meaning of an exit
  code, or changing a note string is **major**. **Adding a check is major too**
  when it can fire on a corpus that was clean, because `check` has no warning
  tier (§14) and its caller is a git hook: a new finding is indistinguishable
  from a break for everyone downstream. Measure a new check against real
  corpora before adding it.
- Adding a check, a command, or an optional field is **minor**.
- Clarifying wording without changing behaviour is **patch**.

A change to the conformance fixtures is the signal that behaviour moved.
