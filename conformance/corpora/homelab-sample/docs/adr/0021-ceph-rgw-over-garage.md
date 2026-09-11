---
id: ADR-0021
status: accepted
decisions:
  - key: storage.object-store
    scope: homelab
    choice: Ceph RGW, ceph-objectstore in rook-ceph
    replaces: [ADR-0009]
---

# 0021 — Ceph RGW over Garage for homelab object storage

Fixture stub. This ADR does not exist in the real corpus yet; writing it is a
Phase 3 task. It records the reason ADR-0009 never considered: Garage sat on
Ceph RBD, so every object was stored twice over.

Replacing only the homelab slot leaves `storage.object-store@nas` on Garage,
which is what actually happened: NAS Garage was demoted to backup mirror, not
decommissioned.
