.PHONY: build test check install uninstall
build:
	cargo build --release --locked
test:
	cargo test --locked
check:
	cargo fmt --check
	cargo clippy --all-targets --locked -- -D warnings
install:
	@./scripts/install.sh
uninstall:
	@./scripts/uninstall.sh
