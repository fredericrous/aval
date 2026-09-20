---
id: ADR-0003
status: accepted
decisions:
  - key: ci.system
    scope: cloud
    choice: Forgejo Actions
    first: true
---

# 0003 — Forgejo Actions runs the cloud cluster's checks

Decided for one scope and never fleet-wide, so `ci.system` at the default scope
is `undecided` — and a ranking that stopped there would be useless. It says
where the key IS decided instead.
