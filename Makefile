.PHONY: all check lint test deps build fmt

all: check

## Everything CI runs, in the order it runs it.
check: deps lint test

deps:
	@./scripts/check-no-deps.sh

lint:
	cargo fmt --all --check
	cargo clippy --all-targets -- -D warnings

test:
	cargo test

build:
	cargo build --release

fmt:
	cargo fmt --all
