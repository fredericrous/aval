---
id: ADR-0009
status: accepted
decisions:
  - key: storage.object-store
    scope: homelab
    choice: Garage, served from the NAS cluster
    first: true

  - key: storage.object-store
    scope: nas
    choice: Garage
    first: true
---

# 0009 — Garage over MinIO for S3 storage

Fixture stub. Source: the homelab repository, docs/adr/0009-garage-over-minio.md.

Two scopes, one technology. ADR-0021 later replaces only the homelab slot, which
is the case that whole-document supersession could not express.
