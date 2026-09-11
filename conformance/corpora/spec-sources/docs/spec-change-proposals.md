---
id: spec-change-proposals
status: accepted
decisions:
  - key: model.change-proposal-shape
    choice: A semantic diff over a branch copy
    first: true
    reason: A raw CRDT diff describes operations rather than changes
---
# Change proposals

A specification that carries its decision, named in `sources`. It keeps its
own filename, so the thirteen places that cite it by path still resolve.

It also carries more than one decision in real life; this one is enough to
prove the shape.
