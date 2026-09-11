---
id: ADR-0001
status: accepted
decisions:
  - key: gitops.reconciler
    choice: Flux CD v2 with OCIRepository as the source kind
    first: true

  - key: gitops.registry-mirror
    scope: homelab
    choice: Zot mirror on the NAS
    first: true

  - key: gitops.registry-mirror
    scope: monitor
    choice: ghcr.io directly
    first: true

  - key: gitops.registry-mirror
    scope: nas
    choice: ghcr.io directly
    first: true
---

# 0001 — Flux + OCIRepository over ArgoCD

Fixture stub. Source: the homelab repository, docs/adr/0001-flux-over-argocd.md.

Note the shape this conversion forced: `gitops.reconciler` is genuinely global,
while the registry mirror was decided per cluster in the same document.
