---
id: ADR-0002
status: accepted
decisions:
  - key: storage.object-store
    choice: Ceph RGW, ceph-objectstore in rook-ceph
    replaces: [ADR-0001]
    reason: Garage sat on Ceph RBD, so every object was stored twice over
---

# 0002 — Ceph RGW over Garage for object storage

The bucket lives in `kubernetes/data-storage/ceph`, and every application that
claims one does it in `kubernetes/apps/*/values.yaml`.

Those two lines are the mention signal: a path a record names on purpose is
evidence no amount of vocabulary overlap can be.
