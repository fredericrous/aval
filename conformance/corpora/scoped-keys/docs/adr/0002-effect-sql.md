---
id: ADR-0002
status: accepted
decisions:
  - key: stack.sql-layer
    choice: "@effect/sql"
    first: true
---
# 0002 — @effect/sql, not a query builder

Declared at the default scope even though the key is restricted to
`effect-stack`. The default scope is the inheritance root, so restricting a key
must never sever its own fallback.
