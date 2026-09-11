# Homebrew's cargo is usually first on PATH and IGNORES rust-toolchain.toml,
# because the pin is a rustup feature rather than a cargo one. `make check` then
# lints with a different clippy than CI does, a clean local run means nothing,
# and the first you hear of it is a red build. Prefer rustup's shim, which reads
# the pin. Verified the hard way: 1.98 accepted a `nonminimal_bool` that the
# pinned 1.94.1 rejects.
CARGO := $(shell command -v rustup >/dev/null 2>&1 && test -x "$(HOME)/.cargo/bin/cargo" && echo "$(HOME)/.cargo/bin/cargo" || echo cargo)

.PHONY: all check lint test deps build fmt toolchain

all: check

## Everything CI runs, in the order it runs it.
check: toolchain deps lint test

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

build:
	$(CARGO) build --release

fmt:
	$(CARGO) fmt --all
