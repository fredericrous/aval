# Building

```sh
make check              # what CI runs: toolchain, no-deps, fmt, clippy, tests, msrv
cargo build --release
```

No external dependencies, by design and enforced in CI by
`scripts/check-no-deps.sh`. `aval` runs on the pre-commit path, so it pulls in
nothing.

The toolchain is pinned in `rust-toolchain.toml`, and `make check` uses rustup's
shim when one is present, because a Homebrew cargo earlier on `PATH` ignores the
pin and would lint with a different clippy than CI. `make msrv` builds with the
`rust-version` that `Cargo.toml` declares; it says SKIPPED, loudly, when that
toolchain is not installed.

The workspace is two crates: `aval-core` (the model, the parser, the graph and
the text index; it touches neither the filesystem nor a process) and `aval`
(the CLI, loading and git, the ranking, the hook and the MCP server). `conformance/` holds the battery that pins
resolution, ranking and trait matching byte for byte; a change there is the
signal that behaviour moved.

## The documentation

These pages are an [mdBook](https://rust-lang.github.io/mdBook/) with its
source in `docs/` itself. A page that is not listed in `docs/SUMMARY.md` is not
in the book.

```sh
mdbook build docs       # renders into target/book
mdbook serve docs       # the same, live, on localhost:3000
```

The normative specification,
[`SEMANTICS.md`](https://github.com/fredericrous/aval/blob/main/SEMANTICS.md),
and the
[`CHANGELOG.md`](https://github.com/fredericrous/aval/blob/main/CHANGELOG.md)
stay at the repository root: the release archives ship the first, and the
release notes are cut from the second.
