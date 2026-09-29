# Installing and gating

## Install

```sh
# Linux and macOS
curl -fsSL https://raw.githubusercontent.com/fredericrous/aval/main/install/install.sh | sh
```

```powershell
# Windows
irm https://raw.githubusercontent.com/fredericrous/aval/main/install/install.ps1 | iex
```

Either one downloads a release binary, verifies it against the published
`SHA256SUMS`, and puts it in `~/.local/bin`, saying so if that is not on your
`PATH`. Pin a version or move the destination with `AVAL_VERSION` and
`AVAL_BIN_DIR`:

```sh
curl -fsSL https://raw.githubusercontent.com/fredericrous/aval/main/install/install.sh | AVAL_VERSION=1.8.0 sh
```

Prebuilt binaries are published for Linux x86_64 (glibc and musl), Linux
aarch64 (glibc), macOS on both architectures, and Windows x86_64. Also:

```sh
brew install fredericrous/tap/aval
cargo install aval
npx aval-adr --version
```

The npm package carries the suffix because plain `aval` was taken in 2016 by an
unrelated property validator; the binary it installs is still `aval`.

## Gating a repository

Nothing is gated by installing. To gate a repository, one committed line in its
[`amont.conf`](https://github.com/fredericrous/amont):

```text
pre-commit    adr   *+.adr.yaml   block   aval check
```

The `+` keeps it inert in any repository without a `.adr.yaml`, and a missing
binary is reported as a gap rather than blocking a commit. A repository that
declares `areas:` can gate those too:

```text
pre-commit    traits   *+.adr.yaml   block   aval traits --check
```

Because a contributor without `aval` is waved through locally, CI should run
`aval check` as well. It includes the `HEADS.md` freshness check.

## Starting a corpus

Records live under `dir`, and a specification that carries decisions can be
named where it is rather than moved — see [the model](concepts.md). To adopt
decisions another repository already made, `aval add <source>` is the whole
setup: it writes a starter registry if there is none. See
[sharing](sharing.md). A directory of ADRs in an older, prose-bullet format is
assessed by `aval migrate <dir>`, which reports per file what a person still has
to supply, and writes nothing.
