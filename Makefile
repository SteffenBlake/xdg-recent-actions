.PHONY: build test check docs install uninstall
build:
	cargo build --release --locked
test:
	cargo test --locked
check:
	cargo fmt --check
	cargo clippy --all-targets --locked -- -D warnings
docs:
	cargo doc --no-deps --document-private-items
install:
	@./scripts/install.sh
uninstall:
	@./scripts/uninstall.sh
