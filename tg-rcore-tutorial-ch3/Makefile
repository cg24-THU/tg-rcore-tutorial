.PHONY: run run-coop run-exercise test-base test-exercise fmt check build package dry-run

run:
	cargo run

run-coop:
	cargo run --features coop

run-exercise:
	cargo run --features exercise

test-base:
	bash ./test.sh base

test-exercise:
	bash ./test.sh exercise

fmt:
	cargo fmt

check:
	cargo check

build:
	cargo build

package:
	cargo package

dry-run:
	cargo publish --dry-run
