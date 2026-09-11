---
id: ADR-0004
status: accepted
decisions:
  - key: secrets.unseal-anchor-location
    scope: homelab
    choice: NAS Vault transit key
    first: true

  - key: secrets.unseal-anchor-location
    scope: monitor
    choice: NAS Vault transit key
    first: true

  - key: secrets.unseal-orchestrator
    choice: vault-transit-unseal-operator; manual unseal forbidden
    first: true
---

# 0004 — Vault transit unseal via NAS Vault

Fixture stub. Source: the homelab repository, docs/adr/0004-vault-transit-unseal-via-nas.md.

The document says "both homelab Vault and monitor Vault auto-unseal against it",
which is two scoped entries, not one global one. The orchestrator is global.
