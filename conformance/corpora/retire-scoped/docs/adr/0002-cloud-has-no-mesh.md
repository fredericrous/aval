---
id: ADR-0002
status: accepted
decisions:
  - key: mesh.data-plane
    scope: cloud
    retire: true
    first: true
    overrides: ADR-0001
    reason: >-
      the cloud cluster is fully standalone; a mesh there would exist only to
      reach services at home, which CGNAT makes unreachable
---
# 0002 — The cloud cluster runs no service mesh

The slot `(mesh.data-plane, cloud)` was never occupied, so there is no same-slot
predecessor to replace. This retirement opts the scope out of the inherited
default instead, which is the second branch of SEMANTICS section 6.1.

Without it, resolving `mesh.data-plane --scope cloud` would fall back to ADR-0001
and answer "Istio ambient" for a cluster that deliberately has no mesh.
