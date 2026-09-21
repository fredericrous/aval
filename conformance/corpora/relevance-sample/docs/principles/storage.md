---
adopts: ADR-0002
source: the estate's own operating practice
---

# Object storage, restated

## storage.one-bucket-per-app [constraint]

An application that stores objects gets a bucket of its own, with its own
credential, and never reads another application's.

A shared bucket makes every application's blast radius the union of all of
them, and there is no way to withdraw one consumer's access without breaking
the others.
