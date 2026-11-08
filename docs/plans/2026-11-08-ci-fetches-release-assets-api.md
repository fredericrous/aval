---
canonical: fredericrous/adr-lint docs/plans/2026-11-08-ci-fetches-release-assets-api.md
phases: [2]
status: done
branch: fix/gh-api-downloads
repos: [aval]
---

# CI fetches GitHub release assets through the API, fleet-wide (pointer)

This repository's slice of the fleet plan: Phase 2. The plan, its review
and its recipe are canonical in adr-lint (pilot: fredericrous/adr-lint#53).

## Code review

- aval: backend, platform

## Outcome

- input: "Ensure aval" script, `AVAL_VERSION=1.10.0`, GH_TOKEN from `gh auth token`, on macOS → expected: fetched through gh, `bin/aval` extracted, GITHUB_PATH written → actual: as expected (the exec fails only on a Linux ELF).
- input: "install mdbook" script, `MDBOOK_VERSION=0.4.40` → expected: fetched through gh, `$HOME/.local/bin/mdbook` extracted → actual: as expected, exit 0. The build job inherits `contents: read`; only `deploy` overrides permissions.
- input: repository-wide grep → expected: only non-goals → actual: `install/install.sh`, `install/install.ps1`, docs and README install lines (product installers), and a fixture string in `scripts/test_bump_tap.py:51`.
- To observe after the merge: dispatch `docs.yaml` on main (it has `workflow_dispatch`).

## Implementation review

- aval implementation-review: approve-with-changes (medium: record outcome; low: book the docs dispatch; both done here); 51k, 37 s (opus).
- aval backend: approve, no findings; 54k, 39 s (opus).
- aval platform: approve, no findings; 57k, 46 s (opus).
