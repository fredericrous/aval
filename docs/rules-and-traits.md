# Rules and traits

## Rules: what a decision does not settle

A decision settles *what* is used. How the code that uses it is written gets
answered the way the first question used to be — a paragraph restated in six
`CLAUDE.md` files, one already wrong, and whatever a model remembers of a book.
So a **rule** is written once, in a markdown file whose headings are the
declarations, and it is **adopted by a record**:

```markdown
---
adopts: ADR-0011
source: Clean Code (Robert C. Martin, 2008)
---

## names.reveal-intent [constraint]

Names reveal intention: an identifier says what it holds, in the vocabulary
of the domain, and a reader never decodes an abbreviation.

The body is the translation — what this means here, and what it does not cover.
```

The registry names rule files under `rules:`, as literal paths. A rule has no
authority of its own: it is active exactly while the record that adopts it
still holds, so superseding that record retires its rules with it. There are no
per-rule supersession edges, because the graph already tracks the record — a
rule whose meaning changes gets a new id.

```console
$ aval rules --level heuristic
heuristic  architecture.business-rules                    An entity embodies critical business rules and data …
heuristic  architecture.draw-boundaries-deliberately      Boundaries are drawn where the axis of change lies, …
…
aval: hidden by traits (cli): 0 constraint(s), 4 heuristic(s); `aval rules --all-traits` lists them

$ aval rule names.reveal-intent
names.reveal-intent   constraint
  adopts: decisions:ADR-0011   (active)
  source: Clean Code (Robert C. Martin, 2008)
  …
```

A `constraint` is followed and a review blocks on it; the session hook prints
every active one. A `heuristic` is followed unless a reviewer argues why not,
in that place, and is fetched on demand. Precedence, which the hook also
prints: the decision at the scope asked, then the default-scope decision, then
these rules, then the book a rule cites — as explanation only. Remembered
advice from that book does not outrank a rule here.

`--adopted-by <record>` lists only the rules one record adopts, and `--all`
adds the inactive ones, each with the reason it is inactive.

## Traits: rules that only apply where the thing exists

Rules about command-line programs mean nothing in a repository that ships none.
A rule file says what it is about, the fleet corpus declares the vocabulary,
and each repository declares which of its parts have which traits:

```yaml
# a rule file's frontmatter
applies: [cli]

# the producer's .adr.yaml — travels in its pack
traits: [cli, ui]

# a consumer's .adr.yaml — its own, never in a pack
areas:
  "web/**": [ui]
  "cmd/**": [cli]
disclaims:
  "tools/**": [cli]
```

With `areas` declared, `aval rules` and the hook list only rules that apply to
the traits declared, and **say what they hid** — on stderr, in `omitted` on
every `--json` and tool payload, and in one hook line printed even when nothing
is hidden. A hidden rule is still active: `aval rule <id>` explains it and
`--all-traits` lists it. Without `areas`, nothing changes.

`aval traits --detect` proposes areas from the files git tracks, and `aval
traits --check` reports what the declaration misses, locally per package, plus
any glob that matches nothing. Detection is advisory and leans toward
reporting: a false positive costs one `disclaims` line, while a miss would hide
a constraint. It never filters anything on its own. `aval traits --summary` is
the one line the session hook prints:

```console
$ aval traits --summary
traits here: cli — hidden by traits: 5 constraint(s), 4 heuristic(s) (aval rules --all-traits)
```

Traits are applicability, never resolution: nothing about them changes a
verdict, whether a rule is active, or what `aval check` reports. §2.5 of
[`SEMANTICS.md`](https://github.com/fredericrous/aval/blob/main/SEMANTICS.md)
is the full definition.
