---
id: plan-ia-coherence
status: accepted
decisions:
  - key: ia.navigation-policy
    choice: Six destinations under three verbs
    first: true
  - key: ia.mobile-write-policy
    choice: Read-only on touch
    first: true
  - key: ia.mobile-write-policy
    scope: storm-board
    choice: Full editing parity on touch
    first: true
    overrides: plan-ia-coherence
---
# Information architecture

Two decisions, and a scoped divergence from one of them recorded in the same
document — which the model allows, because `overrides` names an entry at the
default scope and a document holds at most one entry per slot.
