# Homebrew's cargo is usually first on PATH and IGNORES rust-toolchain.toml,
# because the pin is a rustup feature rather than a cargo one. `make check` then
# lints with a different clippy than CI does, a clean local run means nothing,
# and the first you hear of it is a red build. Prefer rustup's shim, which reads
# the pin. Verified the hard way: 1.98 accepted a `nonminimal_bool` that the
# pinned 1.94.1 rejects.
CARGO := $(shell command -v rustup >/dev/null 2>&1 && test -x "$(HOME)/.cargo/bin/cargo" && echo "$(HOME)/.cargo/bin/cargo" || echo cargo)

.PHONY: all check lint test deps build fmt toolchain msrv

all: check

## Everything CI runs, in the order it runs it.
check: toolchain deps lint test msrv

## Say which toolchain is about to be used, so a mismatch is visible.
toolchain:
	@echo "using: $$($(CARGO) --version)  (pinned: $$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml))"

deps:
	@./scripts/check-no-deps.sh

lint:
	$(CARGO) fmt --all --check
	$(CARGO) clippy --all-targets -- -D warnings

test:
	$(CARGO) test

## The floor `rust-version` claims, actually compiled against.
##
## This was missing while the target above claimed to be everything CI runs,
## and the gap shipped: 1.94 extends a temporary's lifetime out of a match arm
## and 1.74 does not, so a clean local run went red on the `msrv` job. A
## promise a Makefile makes has to be one it keeps.
##
## Both spellings are tried: CI installs the `1.74` alias, while a machine that
## already had it is likely to hold `1.74.0`, and a false SKIP is worse than no
## target at all.
msrv:
	@v=$$(sed -n 's/^rust-version = "\(.*\)"/\1/p' Cargo.toml); \
	for t in $$v $$v.0; do \
		if rustup run $$t cargo --version >/dev/null 2>&1; then \
			echo "msrv: building with $$t"; exec rustup run $$t cargo build; \
		fi; \
	done; \
	echo "msrv: rust $$v is not installed — run 'rustup toolchain install $$v'"; \
	echo "msrv: SKIPPED, so this run does not prove the floor"

build:
	$(CARGO) build --release

fmt:
	$(CARGO) fmt --all
