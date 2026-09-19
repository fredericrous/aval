# `aval` — normative semantics

Version 1.2.0. This document is the specification. Where an
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
  `HEADS.md` is written. REQUIRED unless `packs` is non-empty: a registry that
  only vendors keeps no records of its own and has nowhere to write a
  projection.
- `sources` — additional records, named outright (§2.2). Optional.
- `packs` — vendored declarations from other repositories (§2.3). Optional.
- `rules` — rule files, named outright (§2.4). Optional. A rule is a statement
  of practice adopted by a record, so listing the files here is still declaring
  vocabulary: what the rule says binds only while the record that adopts it
  holds, and the record is where that is reviewed.
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

### 2.3 Packs

A decision made once should be readable everywhere it applies. A registry MAY
vendor another repository's declarations:

```yaml
packs:
  - .adr/packs/decisions.pack
```

- Entries are **literal repository-relative paths**, for the reasons §2.2
  gives. A listed pack that is missing or unreadable is an error.
- The vendored file's extension is **`.pack`**, the one the producing
  repository's own published file carries. The reason is not taste: a consumer
  runs a formatter over every YAML file it can name, and a formatter that
  requotes a generated file turns it into a file that gets hand-edited.
  `.pack` is claimed by no formatter. Nothing reads the extension — a `.yaml`
  written by an earlier version keeps loading, so a listed path is never wrong
  for its extension — and `aval add` migrates such a file, moving it and
  rewriting the one registry line that names it.
- A pack file is written by `aval add` and MUST NOT be edited by hand. What it
  **declares** is what the producing repository published at the recorded
  commit, under a comment banner naming the source, the revision asked for,
  and the commit id that revision resolved to.

  **Bytes are not the property.** Byte-equality was the wrong one: a file a
  formatter rewrote had been edited by nobody and was unchanged for every
  purpose this tool has, yet it compared unequal — so `add` rewrote it on
  every run and a freshness check would have accused its author of an edit
  they did not make. Comparison is of parsed values, ignoring line numbers,
  comments, quoting and blank lines; the same move §12.2 makes for `HEADS.md`.
  What "MUST NOT be edited by hand" forbids is changing what the file
  *declares*, and `aval add --check` reports that as `edited`.
- The **pack name** is the filename without its extension. It MUST satisfy the
  slug grammar of §3.8 and MUST NOT contain `:`. Two packs MUST NOT share a
  name. The name is carried in the path rather than in a second field, because
  two fields that must agree are two fields that can disagree.

#### What a pack carries

A pack carries **declarations**: the scope vocabulary, the key definitions,
every record's frontmatter, and the rules those records adopt (§2.4). It MUST
NOT carry a projection, and it MUST NOT carry a record's body.

`HEADS.md` is derived state, and §10's `no-manual-index` exists because copied
derived state has no invariant behind it — nothing knows that a row which
should be there is missing. Vendoring a projection would spread that failure
across repositories instead of confining it to one. Vendoring declarations
means the consumer computes heads itself, from the same graph, with the same
code, so a superseded decision cannot survive the trip.

A pack MUST NOT re-export what it vendored. A consumer that publishes its own
pack publishes its own decisions only. Otherwise one consumer's copy of a
decision reaches another by a route neither chose, and the qualification below
stacks until nothing names the record it refers to.

#### Identity

A vendored record is referred to as `<pack>:<id>`. Qualification happens when
the pack is read, not when it is written, so the file on disk stays what the
producer published, as declarations.

Two repositories both numbering from `ADR-0001` is the ordinary case, not an
edge case, and an unqualified collision would report `id-unique` against a
corpus whose author wrote neither id.

References inside a pack — `replaces`, `overrides` — are qualified with it. A
reference that names nothing in the pack is left exactly as written, so the
resulting finding blames the name the producer used rather than one this tool
invented.

A local record MUST NOT reference a vendored record. Editing another
repository's graph from inside a consumer is not a supersession, and the way to
change a vendored decision is to change it where it was made.

#### What a consumer may and may not do

A consumer MAY **widen** a vendored key: re-declaring it locally with a
`scopes` list adds those scopes to the ones the pack declared. The list is what
is being *added*, so narrowing is not something the format can express rather
than something a check has to catch.

This is the mechanism for the case that actually arises. One repository can
hold several answers for one key — a SQL layer in the browser, another in the
app server, a third in a second webapp — and none of them is a disagreement
with the fleet's answer at the fleet's scope. They are different slots, and the
fleet never decided them.

A consumer MUST NOT:

- re-declare a vendored key's `description` when the pack gives one;
- list scopes for a vendored key the pack declares without a restriction, which
  would add nothing while appearing to restrict;
- decide a slot the pack already decides. That is two heads for one slot, and
  §5 answers exit 5. Nothing special enforces it — it is the invariant the
  model already had, which is why vendoring is worth doing at all.

Layer C never applies to a vendored record. Those checks are about *this*
repository — whether its citations resolve, whether its documents state a
status twice — and a pack was checked where it was written.

#### What a pack is not

**Nothing in a pack is executed.** No shell runs, no code loads.

This is the deliberate divergence from `amont`, whose packs vendor shell
commands and which therefore takes consent per machine, content-keyed, and
re-takes it whenever a single byte changes. That gate exists because running
somebody else's command is the risk. Here, nothing runs. What a pack can do is
change an answer, and the place to catch that is the pull request that adds the
file — which is where a change of architectural direction belongs.

**"Inert" scopes to execution, and no further.** A pack's text reaches an
agent's context: the session-start hook prints it, and `aval mcp` returns it.
Text in a model's context is not inert in the way a file on disk is — it is the
one input that acts. So a pack is a path by which another repository's prose
reaches a reader who was told, by this tool, that it is settled.

Three things hold that down, and none of them is detection:

1. The hook and the tool surface both state that a record's wording is **data**,
   not instruction, and that text reading as an instruction should be reported
   rather than followed.
2. §3.7 rejects a value carrying a control character, a bidi override or an
   embedded newline, so the text a reviewer reads in the pull request is the
   text the model receives. That is the property review depends on.
3. The pull request that adds or updates the pack is the gate, as above.

Detecting instruction-shaped prose is deliberately **not** attempted. A pattern
for it fails open, and a false positive on a legitimate `choice` would block a
decision for looking wrong. Making the bytes reviewable and saying plainly what
they are is the part that can be done correctly.

The recorded commit id is **provenance, not authority**. It says the bytes are
the ones that repository published. It says nothing about whether the decisions
are good ones.

#### Staleness is reported, never repaired

A vendored pack that is behind answers `active` with a decision that was
superseded, which is the failure this tool exists to prevent. `aval add
--check` re-resolves each recorded revision and reports whether it still names
the recorded commit.

It reports one more standing, `edited`: the revision still names the recorded
commit, and the vendored file no longer declares what that commit published.
Re-resolving alone never asked that question, and the answer it missed is the
same failure from the other side — a decision this repository believes another
one made, that nobody made. It is counted with `behind` because one command
fixes both: `aval add` writes what the source published. The comparison is
declarational, so a file a formatter reformatted is `current`, not `edited`.
The check is asked only of a pack that is otherwise current: a `behind` pack is
being replaced whatever its content says, and an `unknown` one cannot be
fetched to compare against.

It reaches the network, so **no hook, gate or `resolve` may call it**, and none
does. A corpus that needed the network to answer a question would be useless
offline and unusable in a CI job with no credential for the source — which is
the ordinary case, not a corner of it: the corpus this was built for is private
on one forge and read by repositories on another.

The consequence is a real cost and is stated rather than hidden: when a fleet
decision changes, each consumer is updated by a person running `aval add`
again, and reading the diff.

### 2.4 Rules

A decision settles *what* is used. It does not settle *how* the code that uses
it is written, and that second question is answered today the way the first one
used to be: by a paragraph restated in six `CLAUDE.md` files, one of which is
already wrong, and by whatever a model remembers of a book it read in training.
Both failures are the one this corpus exists to end, so the same treatment
applies — write it once, in one place, with something that can say whether it
still holds.

A **rule** is a one-line statement of practice with a stable id, a level, a
body that explains and translates it, and **the record that adopts it**.

**A rule has no authority of its own.** It is active exactly while the record
that adopts it is accepted and still holds — derived status `active` or
`partially superseded` (§8). When that record is superseded or retired, every
rule it adopted stops being active with it. This is the whole design: a rule
that could be authoritative on its own would need its own status, its own
supersession edges and its own review, and the second copy of "does this still
hold" is the copy that goes stale.

Two levels, and the difference is what a reviewer does with them:

- `constraint` — followed; a review blocks on it. The session hook prints every
  active constraint, because a constraint nobody is shown is one nobody
  follows.
- `heuristic` — followed unless a reviewer argues why not, **in that place**.
  Fetched on demand, never injected: a heuristic printed at every session start
  would spend context on advice that is right to break.

**Precedence**, highest first, stated here and printed by the hook:

1. a decision at the scope asked — `aval resolve <key> --scope S`;
2. the default-scope decision it inherits from (§5);
3. an adopted rule;
4. the source a rule cites — a book, a specification — as **explanation only**.

The fourth line is the one that has to be written down. A rule restates a
source in this estate's terms, and the restatement is what was agreed to; the
source is why. An agent invoking remembered book advice against a written rule
is not citing an authority, it is **contradicting a decision**.

#### Rule files

The registry lists rule files as literal repository-relative paths, exactly as
`sources` and `packs` are listed and for the same reason (§2.2): a pattern that
stops matching drops a rule silently, and nobody is told that a constraint left.

```yaml
rules:
  - docs/principles/clean-code.md
  - docs/principles/clean-architecture.md
```

A rule file is markdown with YAML frontmatter. It is one document, written for
a person, whose headings are also the declarations — because the alternative is
a list of rules beside a file explaining them, which is two files that drift.

```markdown
---
adopts: ADR-0011
source: Clean Code (Robert C. Martin, 2008)
---

# Clean Code, restated

Intro prose. Anything before the first rule heading is for the reader.

## names.reveal-intent [constraint]

Names reveal intention: an identifier says what it holds, in the vocabulary
of the domain, and a reader never decodes an abbreviation.

Body. Free markdown until the next `## ` heading, `###` sub-headings and code
included.
```

- Frontmatter carries `adopts` (REQUIRED; a record id local to this corpus,
  never a vendored `pack:ADR` id — a consumer does not state the fleet's rules)
  and `source` (optional, one line, printable per §3.7). An unknown field is a
  Layer A error, as in ADR frontmatter and for §3.2's reason.
- A rule starts at a line matching `## <id> [<level>]` **exactly**: two hashes,
  one space, the id, one space, the level in square brackets, nothing else.
  `<level>` is `constraint` or `heuristic`.
- `<id>` follows the key grammar of §1.1 — lowercase ASCII letters, digits and
  `-`, in `.`-joined segments, at least two. Rule ids and decision keys are
  **separate namespaces**; a rule id equal to a key is not an error, because
  the two are never looked up in the same place.
- The **statement** is the first paragraph under the heading: the first
  non-blank line and every line up to the first blank one, joined with single
  spaces. It MUST be non-empty and printable (§3.7). It is the one line the
  hook and `aval rules` print, so it is joined rather than kept as written — a
  hard wrap is typography, not part of what the rule says.
- The **body** is everything after that paragraph up to the next rule heading
  or end of file, trimmed. It may be empty, and it MUST be printable (§3.7):
  it travels in a pack and reaches an agent through `aval rule`, so the text a
  reviewer reads has to be the text a model receives.
- A `## ` line that does not match the grammar is a **parse error naming the
  line**, and so is a `# ` heading after the first rule. Neither may quietly
  become body text: a missing bracket or a misspelled level would fold a rule
  into the previous rule's body, where it is still perfectly readable prose, so
  a reviewer sees a rule and the tool has none. Fenced code is exempt from
  both — a `## ` inside a fence is an example, and a check that cried wolf on
  sample code would be switched off, and then it would catch nothing.
- A file listed in `rules` that declares no rule is an error.

#### History, and changing a rule

**A rule's text history is git.** §2.2 already makes this argument for a
specification rewritten in place: a document that carries its decisions has its
history in the commit log, and that is a legitimate shape rather than a lesser
one. A rule file is that shape.

**A rule whose meaning changes gets a NEW id, and the old id is removed.**
There are deliberately no per-rule supersession edges. A rule's authority is
its adopting record's, and that record is what the graph tracks — adding a
second lineage for rules would mean two graphs, and the failure the first one
exists to prevent is precisely two answers to one question. Changing what an id
means without changing the id is the one move that cannot be reviewed: every
consumer that vendored it keeps the old words under the new agreement.

A change of **level** is an edit, reviewed in the pull request and visible to
consumers in the pack diff. Promoting a heuristic to a constraint is a decision
about how much a review blocks, which is exactly what a pull request is for.

#### Rules in a pack

`aval pack --write` emits a `rules:` section after `records:`, sorted by id,
each rule carrying its statement **and its body**. A rule body is the one piece
of prose a pack carries, and it is not a counter-example to "a pack MUST NOT
carry a record's body": it is a declaration in its own right, it is printed
nowhere by default and fetched by id, and a rule a consumer can list but cannot
explain is one it cannot apply. The "inert" paragraph of §2.3 governs what
happens when that text reaches a model, exactly as it does for a `choice`.

On read, `adopts` is qualified with the pack name (`decisions:ADR-0011`)
exactly as `replaces` and `overrides` are, and only when it names a record
inside the pack. A vendored rule's file is the pack file, like a vendored
record's. A pack MUST NOT re-export vendored rules, for §2.3's reason.

A consumer MUST NOT re-declare a rule a pack declares. That is
`rule-id-unique`, and it is the same rule as keys: two statements of one
practice, with nothing keeping them in step.

#### What is checked, and what is not

Three Layer A checks, listed in §10. Each fires only where a registry declares
`rules`, so a corpus that has none cannot tell this version from the one
before it.

**Activity is not a check.** A rule adopted by a superseded record is
**inactive, not wrong** — being replaced with its record is the ordinary end of
a rule's life, and reporting it would ask an author to delete the history of
how the estate used to be written. `aval rules --all` shows it with the reason;
`check` says nothing.

Layer C's `links-resolve` runs over rule files: they are documents of this
repository, a body cites the code it is about, and a citation that rots there
misleads exactly as much as one in a record.

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

A value MUST be **printable text**. A control character, a bidi override
(`U+202A`–`U+202E`, `U+2066`–`U+2069`) or an embedded newline is a Layer A
error reported with its line.

The newline rule is §12.2's: a table row cannot carry one. The rest is §2.3's:
these values are printed into an agent's context, so the text a reviewer reads
in the pull request has to be the text the model receives. Both depend on the
printed form and the reviewed form being the same bytes.

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
| `pack-parses` | a vendored pack that is malformed, or two packs sharing a name (§2.3) |
| `pack-key-widens` | a local re-declaration of a vendored key that does anything but widen it (§2.3) |
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
| `rules-parse` | a rules file whose frontmatter, heading grammar or statement is malformed (§2.4) |
| `rule-id-unique` | one rule id declared twice, across every rules file and every vendored pack (§2.4) |
| `rule-adopts-resolves` | `adopts` naming no record of this corpus's own (§2.4) |

### Layer B — verdicts

| Check | Reports |
|---|---|
| `single-head` | a slot with more than one head |

### Layer C — hygiene

| Check | Reports |
|---|---|
| `heads-fresh` | `HEADS.md` not matching the projection after canonicalisation (§12.2) |
| `pack-fresh` | a published `aval.pack` not matching the declarations this corpus states (§2.3) |
| `links-resolve` | an unpinned citation that does not resolve (§11), in a record or in a rules file |
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
- **A workspace is a set of trees, and the set is ordered.** `read_dir` returns
  its entries in no defined order, so discovery (§14.1) sorts repositories by
  name and deduplicates them by canonical root before anything reads them.
  Without that, the map's key order, the text's heading order and a symlinked
  duplicate would all vary from one call to the next over the same disk.

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
| `aval hook install` | — | `0` · `1` write failed · `2` · `3` unreadable settings |
| `aval hook install --check` | — | `0` wired · `1` stale · `2` · `3` |
| `aval pack` | A | `0` · `2` · `3` |
| `aval pack --write` | A | `0` · `1` write failed · `2` · `3` |
| `aval pack --check` | A + C | `0` fresh or not publishing · `1` stale · `2` · `3` |
| `aval add` | A | `0` · `1` unreachable, ambiguous, or refused · `2` · `3` |
| `aval add --check [--quiet] [--budget S]` | A | `0` current or unknown · `1` behind or edited · `2` · `3` |
| `aval keys` | A | `0` · `2` · `3` |
| `aval rules` | A | `0` · `2` · `3` |
| `aval rule` | A | `0` found · `2` · `3` · `7` unknown |
| `aval repos` | — | `0` · `2` · `3` nothing found, or the directory unreadable |
| `--all-repos` on `resolve` / `keys` / `heads` / `history` / `rules` | A per member | `0` every member loaded, none exited 5 · `1` a member exited 5 or did not load · `2` with `--write`, `--check`, or on `show` or `rule` · `3` no member loaded |
| `aval mcp` | — at startup | `0` stdin closed · `1` transport failure · `2` |

`hook install` reads no corpus and touches no layer. It wires a session-start
hook that runs `heads`; whether the corpus resolves is that command's business,
and the generated script stays **silent** when it does not, because a session
must not fail over a tool the person who started it has not installed.

The generated hook also runs `add --check --quiet --budget 5`, at most once an
hour per repository, and prints what that reports only when it exits `1`. Its
`0` — current, or could not ask — prints nothing, which is the rule above
applied where it matters most: a hook that announced "could not ask" on every
flaky network would be ignored on the day a pack was behind. `--budget` makes
"could not ask" the answer for a remote that has not replied in S seconds; the
call is killed, the standing is `unknown`, and the exit code follows. Under a
budget no git call may prompt: `GIT_TERMINAL_PROMPT=0` always, and
`GIT_SSH_COMMAND` set to a non-interactive ssh unless the caller set one.

`aval mcp` never exits `3`. It reads no corpus at startup, because a registry
being edited must not take the surface away, and a corpus that will not load is
reported inside the tool result where the caller can see why. Its exit code
describes the transport and nothing else.

`aval add --check` reports `0` when a pack's standing cannot be determined —
offline, moved, access lost. Being unable to ask is not an answer, and a
verdict of "behind" that was really "I could not reach the remote" would teach
its caller to ignore the one that matters. It says so in words and counts it
separately. `edited` is an answer and counts with `behind`; when both hold of
one pack it reports `behind`, because re-vendoring settles both.

`check` deliberately reports `1` for any finding regardless of layer, because
its caller is a git hook, where amont's contract is that `0` passes and anything
else fails.

### 14.1 The tool surface

`aval mcp` serves the corpus over MCP. It is a second **surface** on the same
semantics, never a second implementation of them: it resolves through the same
graph and renders through the same producer as the CLI, and a payload it
returns is byte-equal to the corresponding `--json` invocation.

**A verdict is not an error.** MCP gives a tool result a single `isError` flag.
It MUST be `false` for all five verdicts, `contradiction` included, and MAY be
`true` only where no question was answered at all: a corpus that will not load,
or a name the corpus does not carry.

The reasoning is section 14's, one level up. A caller that is told the tool
failed retries it, or works around it. `contradiction` means *stop, do not pick
one* — the one verdict where working around it is the specific harm the corpus
exists to prevent — so reporting it as a malfunction inverts the tool. A caller
distinguishes verdicts by `state` and `exit` inside the payload, exactly as a
shell caller distinguishes them by exit code.

**A protocol error is for a call that could not be made.** An unknown tool, a
missing or mistyped argument, a malformed envelope: JSON-RPC errors, because no
reading of a tool result would help a client fix them. A well-formed call
naming something the corpus does not carry is a tool result, because it is an
answer, and it carries the advisory suggestion.

**The corpus is the working tree, read fresh for every call.** The surface
holds no cached graph. A caller edits records in the same session it asks
questions in, and an answer from a graph loaded earlier would describe a corpus
that no longer exists. There is deliberately no revision pinning: the revision
is the tree — or, in a workspace, the set of trees — and a decision that is
not written down yet is not decided.

**A workspace is several corpora, and answers for all of them.** A launch
directory with no registry above it and one or more directly beneath it is a
workspace. Discovery looks up first, exactly as `load` does, and only then one
level down: a repository keeps answering as itself, and a workspace root
answers for what it contains. The scan is repeated on every call, like the
load; `read_dir` order is unspecified, so the result is sorted by name and
deduplicated by canonical root, and both are load-bearing for §12.

In a workspace every tool accepts `repo`, a directory name. Named, a tool
answers for that corpus alone, and the payload is byte-equal to that
repository's own `--json`. Omitted, `aval_resolve` and `aval_history` answer for every repository at once:

```json
{ "repos": { "<name>": { …that repository's own payload… }, … },
  "worktrees_excluded": { "<name>": "<parent>" } }
```

That map is a **report**, not a verdict, and `--all-repos` on the CLI renders
the same one. Its exit follows `check`'s contract rather than `resolve`'s
(§14): `0` when every member loaded and none exited 5; `1` when a member
exited 5 or would not load — its error object stands where its answer would,
because a report does not fail when one member did; `3` when no member loaded.
`undecided`, `retired` and `unknown` members are answers. On the tool surface
`isError` is exactly exit 3: **no entry answered anything.** With a single
corpus the map has one member; the shape never depends on the count.

`aval_show` never aggregates. Record ids are corpus-local (§3.8), so the same
`ADR-0001` exists in every repository and "show it" across a workspace is
under-specified rather than unanswered — a protocol error naming the
repositories, not eight near-misses reported as answers.

`aval_rules` and `aval_rule` are the rule surface (§2.4). `aval_rules` lists
what is adopted, one line each and no bodies; `aval_rule` carries the body,
which is where a rule is narrowed to this estate and where the cases it
deliberately does not cover are written down. Their descriptions MUST state
whose authority a rule carries and where it sits in §2.4's precedence — a
caller that does not know it is weighing an adopted rule against a remembered
book, and the book is what it will pick. `aval_rule` needs `repo` in a
workspace because rule ids are corpus-local, as record ids are; `aval_rules`
needs it for the size reason below.

`aval_keys` and `aval_heads` need `repo` too, for a different reason. A tool
result is paid for in context, every repository's heads at once is tens of
kilobytes, and a caller that forgot the name must get a protocol error listing
the options rather than that payload by accident. The map is handed out from
the tool surface only where it is small and is the actual question. The CLI's
`--all-repos` renders all four: a flag is an explicit ask, and a terminal reads
nothing into a context window.

**A linked worktree is left out of the map only when its parent is in it.**
The exclusion is about duplication — the same corpus would otherwise answer
twice — not about being a worktree: one whose parent is not discovered is the
only representative of that repository and stays. Either way it remains
addressable by `repo`, and the omission is named in `worktrees_excluded`. A
worktree is recognised by its `.git` file naming `.git/worktrees/`; a file
naming `.git/modules/` is a submodule, a repository of its own.

**Resources never carry the map.** A resource is attached once and kept, and
every repository's heads is tens of kilobytes a client would then carry for
the whole session. A workspace offers `aval://repos` and one
`aval://<name>/heads`, `aval://<name>/keys` pair per repository, the name
percent-encoded — a directory may be called anything — and decoded back into a
lookup against the discovered names, never into a path. The bare `aval://heads`
and `aval://keys` keep meaning the launch directory's own corpus and are absent
where there is none, exactly as they errored before.

`aval repos` and the `aval_repos` tool report what discovery saw: `mode`
(`corpus` or `workspace`), each repository's canonical root, its worktree
parent if any, whether it is `shadowed` beneath an active corpus, and every
directory that carried a registry and could not be used, with the reason. In a
corpus that scan is diagnostic — a failure is a `warning` there and never a
reason for the active corpus to stop answering. In a workspace the scan is the
answer, so its failure is one: a permissions error reported as an empty
workspace would be a lie.

**The surface is read-only.** No verb that writes — `heads --write`, `pack
--write`, `add`, `hook install` — is exposed, and each tool declares
`readOnlyHint`. Resolving is answering a question; deciding is not something to
do on a caller's behalf.

Tools MUST carry the caller obligations this document states — that a
suggestion is advisory (section 5), that only an exact key resolves (section
12.1), that history is not authority (section 4) — in their descriptions. On a
shell surface those obligations are enforced by a caller that already knows
them. On a tool surface the description is where a caller learns them, so
omitting one silently removes it.

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

**1.2.0 is minor, and here is the argument.** It adds an optional registry
field (`rules`), two commands, two tools, three checks and a pack section.
Every one of the three checks needs a `rules:` in a registry to reach anything
at all, so on a corpus that declares none they cannot fire and no verdict
moves — which is the test the rule above actually sets. Two things a consumer
does have to do, both stated in the changelog: a producer re-runs `aval pack
--write` and each consumer re-runs `aval add`, and everybody re-runs `aval hook
install`, because the script's bytes are its version and `--check` reports it
stale until they do. One forward-compatibility limit is accepted rather than
worked around: a pack carrying `rules:` is unreadable by aval before 1.2 —
`pack-parses: unknown pack field` — which is the existing version gate doing
what it exists to do, refusing a file it cannot fully understand rather than
reading half of it.

**From 1.0 those words mean what semver says they mean.** Before it, a breaking
change shipped as a minor bump, which is the 0.x convention — and is why §3.7's
printable-value rule, which is breaking, is what made this release 1.0 rather
than another 0.x. From 1.0,
a change in the first list above is a new MAJOR version, and a caller may pin
`1` and expect the rest of this document to hold.

What that pins, precisely:

- the verdict set, their tokens and their exit codes (§14), and the per-command
  exit contracts beside them;
- the note strings, which are asserted by fixtures and are what a caller reads
  when it reports a non-answer;
- the field names of every `--json` payload, and the rule that `--json` writes
  the result object and nothing else (§12);
- the frontmatter dialect (§3) — what a record is allowed to say;
- the tool surface's mapping of verdicts to `isError` (§14.1);
- `aval-core`'s API to the extent §15.1 describes.

What it does not pin: the human text renderings beyond the machine token each
one leads with, the wording of findings, and anything this document calls
advisory.

### 15.1 The library is versioned too

`aval-core` ships to crates.io alongside the binary and shares its version
number, so one number now carries two contracts: what the tool *does*, above,
and what the library *exposes*.

They are not the same promise, and the difference decides where
`#[non_exhaustive]` belongs.

- **A struct may gain a field.** `Entry`, `Adr`, `KeyDef`, `Registry`,
  `Corpus`, `Rule` and `Finding` are `#[non_exhaustive]`, so a field can be added
  without breaking a downstream build, and a caller outside the crate starts
  one through a constructor.
- **An enum's variants are part of the specification.** `Verdict`, `Unknown`,
  `Status`, `Level`, `EntryKind`, `Lineage`, `Layer` and `DerivedStatus` are
  deliberately NOT marked. Section 14 enumerates the verdicts and their exit
  codes: adding one is a major change by the rule above, whatever the type
  system says. Marking them would buy flexibility this document has already
  refused, and would cost the exhaustiveness check that makes a new variant
  nobody rendered a compile error rather than a silent omission.
- `Slot` is the pair `(key, scope)` by definition and will not gain a field.
  `Json` is a JSON value; a new variant breaks every `match` regardless, and
  the ones inside this repository should fail to compile when it does.
