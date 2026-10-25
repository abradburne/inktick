# inktick: `make check` is what every change must pass (CONTRIBUTING.md).

.PHONY: check fmt clippy test shell

check: fmt clippy test shell

fmt:
	cargo fmt --all -- --check

# Each system inktick builds for, with and without DSlot's part.
clippy:
	cargo clippy --workspace --all-targets -- -D warnings
	cargo clippy -p inktick --all-targets --features dslot -- -D warnings
	cargo clippy -p inktick --all-targets --no-default-features --features stock -- -D warnings
	cargo clippy -p inktick --all-targets --no-default-features --features desktop -- -D warnings

test:
	cargo test --release --workspace
	cargo test --release -p inktick --features dslot

shell:
	shellcheck -x tools/*.sh
