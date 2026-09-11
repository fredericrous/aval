#!/bin/sh
# `aval` runs on the pre-commit path, so it pulls in nothing.
#
# The frontmatter dialect is a subset we define (SEMANTICS 3.7), not arbitrary
# YAML, and JSON is small enough to write. Both are in-tree for that reason,
# not by accident, and this check is what keeps a convenience dependency from
# quietly arriving later.
set -eu
extra=$(grep '^name = ' Cargo.lock | sed 's/name = //; s/"//g' | grep -v '^aval$' | grep -v '^aval-core$' || true)
if [ -n "$extra" ]; then
    echo "aval must have no external dependencies, found:" >&2
    echo "$extra" >&2
    exit 1
fi
echo "no external dependencies"
