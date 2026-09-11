---
id: ADR-0015
status: accepted
decisions:
  - key: topology.cloud-cluster
    choice: 3x OVH VPS-2 in Roubaix, Talos, all control plane, cluster id 4
    first: true

  - key: cni.routing-mode
    scope: cloud
    choice: Cilium VXLAN tunnel with WireGuard, MTU 1380, no KubeSpan
    first: true

  - key: secrets.unseal-anchor-location
    scope: cloud
    choice: own Shamir-sealed Vault, mode stored-key
    first: true

  - key: gitops.registry-mirror
    scope: cloud
    choice: ghcr.io directly, not the LAN zot
    first: true

  - key: dns.dynamic-record-updater
    retire: true
    replaces: [ADR-0011]
    reason: >-
      the home uplink moved behind CGNAT, leaving no public IPv4 to track;
      the cloud front door's external-dns owns the A records now
---

# 0015 — Standalone cloud cluster for customer-facing workloads

Fixture stub. Source: the homelab repository, docs/adr/0015-standalone-cloud-cluster.md.

Every cloud entry here is `first: true`, not an override. The document itself
reads the earlier ADRs as cluster-scoped: "homelab's native routing needs a
shared L2 segment", "ADR-0004 anchors homelab's and monitor's seal on the NAS
Vault". There is no global default being overridden.
