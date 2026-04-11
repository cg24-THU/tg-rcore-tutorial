CARGO ?= cargo
QEMU ?= qemu-system-riscv64
CRATE_NAME := cg-tg-rcore-tutorial-t2l3
KERNEL := target/riscv64gc-unknown-none-elf/debug/$(CRATE_NAME)

.PHONY: build check run run-gui run-headless test package publish-dry-run

build:
	$(CARGO) build

check:
	$(CARGO) check

run: run-headless

run-gui:
	$(CARGO) run

run-headless: build
	timeout 3s $(QEMU) \
		-machine virt \
		-bios none \
		-display none \
		-device virtio-gpu-device \
		-serial stdio \
		-monitor none \
		-kernel $(KERNEL) || true

test:
	bash test.sh

package:
	$(CARGO) package

publish-dry-run:
	$(CARGO) publish --dry-run
