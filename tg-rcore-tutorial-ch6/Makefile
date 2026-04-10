.PHONY: run run-exercise build fmt check test-base test-exercise package publish-dry-run

run:
	cargo run

run-exercise:
	cargo run --features exercise

build:
	cargo build

fmt:
	cargo fmt --all

check:
	cargo check

test-base:
	bash ./test.sh base

test-exercise:
	bash ./test.sh exercise

package:
	TG_SKIP_USER_APPS=1 cargo package

publish-dry-run:
	TG_SKIP_USER_APPS=1 cargo publish --dry-run
