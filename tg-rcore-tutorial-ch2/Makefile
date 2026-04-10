CARGO ?= cargo

.PHONY: build run test fmt check package dry-run

build:
	$(CARGO) build

run:
	$(CARGO) run

test:
	bash ./test.sh

fmt:
	$(CARGO) fmt

check:
	$(CARGO) check

package:
	$(CARGO) package

dry-run:
	$(CARGO) publish --dry-run
