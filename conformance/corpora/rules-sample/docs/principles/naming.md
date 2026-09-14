---
adopts: ADR-0001
source: Clean Code (Robert C. Martin, 2008)
---

# Clean Code, restated

Prose before the first rule heading is for the reader. The parser ignores it.

## code.naming.reveal-intent [constraint]

Names reveal intention: an identifier says what it holds, in the vocabulary of
the domain, and a reader never decodes an abbreviation.

The book argues this from reading time. The argument that carries here is
narrower: an abbreviation is a private vocabulary, and a decision corpus exists
because private vocabularies do not survive their author.

### What this rules out

- single letters outside a two-line scope;
- a type name repeated in the identifier that already has the type.

## code.naming.one-word-per-concept [constraint]

One word per concept: `fetch`, `retrieve` and `get` are not three ideas, so a
codebase picks one and keeps it.

## code.arguments.few [heuristic]

A function takes no more inputs than it uses; related inputs travel as one
value, and none is hidden in ambient state to lower the count.

The book says zero is ideal. Under an explicit-dependency style an input is the
dependency declaration, so the count is not the goal — the hiding is what the
rule is against.
