---
id: ADR-0014
status: accepted
decisions:
  - key: forge.provisioning-topology
    choice: both gitea-teams and forgejo-teams registered for the migration
    first: true

  - key: forge.ssh-key-ownership
    choice: forge-native; Duro never stores keys and never syncs them
    first: true

  - key: identity.forge-username-claim
    choice: OIDC preferred_username, never externalId
    first: true
---

# 0014 — Dual forge provisioning during the Gitea to Forgejo migration

Fixture stub. Source: the homelab repository,
docs/adr/0014-forge-migration-dual-provisioning.md.

Three decisions, only one of which the Gitea decommission changed. This is the
partial-supersession case.
