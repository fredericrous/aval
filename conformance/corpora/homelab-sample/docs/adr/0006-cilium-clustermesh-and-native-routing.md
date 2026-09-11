---
id: ADR-0006
status: accepted
decisions:
  - key: cni.cross-cluster-connectivity
    scope: homelab
    choice: Cilium ClusterMesh to monitor, LoadBalancer on port 61279
    first: true

  - key: cni.routing-mode
    scope: homelab
    choice: native routing, ipv4NativeRoutingCIDR 10.244.0.0/16
    first: true

  - key: cni.socket-lb-scope
    scope: homelab
    choice: socketLB.hostNamespaceOnly true
    first: true
---

# 0006 — Cilium ClusterMesh + native routing + socketLB.hostNamespaceOnly

Fixture stub. Source: the homelab repository,
docs/adr/0006-cilium-clustermesh-and-native-routing.md.

The decision text says "native routing for pod-to-pod traffic **on homelab**"
and "socketLB.hostNamespaceOnly: true **on homelab**". Converting honestly makes
these homelab-scoped, not global.
