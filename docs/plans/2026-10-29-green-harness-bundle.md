---
canonical: github:fredericrous/decisions/docs/plans/2026-10-29-green-harness-bundle.md
phases: [2]
status: done
branch: feat/grh-queries
---
# Green Harness bundle

Pointer. The plan and its review panel live in decisions, with ADR-0050
(harness.installer). This repository carries phase 2: `aval mcp install|uninstall` and `aval hook uninstall`, the published surfaces grh calls to place and remove aval's integration.

## Verification

| Check | Input | Expected | Actual |
|---|---|---|---|
| registered, real claude | `aval mcp install --check` (daa9223 build) | "registered", exit 0 | "registered", exit 0 |
| preview | `aval mcp install` | `would run: … <abs aval> mcp` on stdout, hint on stderr, exit 0 | as expected |
| uninstall check | `aval mcp uninstall --check` | exit 1 (still registered) | exit 1 |
| exclusive flags | `aval mcp install --write --check` | exit 2 | exit 2, "mutually exclusive" |
| hook uninstall, empty repo | `aval hook uninstall --check` | nothing installed, exit 0 | as expected |
| no claude on PATH | `PATH=/usr/bin:/bin aval mcp install --check` | stderr line, exit 3 | as expected |
| tests | `cargo test -p aval --test register` | pass | 13 passed |

## Implementation review

approve-with-changes after one delta (86k + 97k). Fixed: `--check` without `claude` exits 3, talk lines on stderr, a Homebrew link is registered instead of the Cellar path.
deliberate: the shared `hook::Status` enum serves install and uninstall reports; the install match stays exhaustive with no `_` arm.
deliberate: `registered_path` reads PATH itself (an I/O helper, not a calculation).
