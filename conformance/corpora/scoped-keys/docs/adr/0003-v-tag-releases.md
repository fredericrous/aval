---
id: ADR-0003
status: accepted
decisions:
  - key: release.trigger
    choice: An annotated v-tag
    first: true
---
# 0003 — releases ship on a v-tag, never on merge

`release.trigger` declares no `scopes:`, so it stays answerable on every axis.
That is the compatibility guarantee for registries written before the field
existed.
