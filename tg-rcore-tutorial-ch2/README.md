# cg-tg-rcore-tutorial-t3l2

`cg-tg-rcore-tutorial-t3l2` is a publishable, reproducible Chapter 2 operating
system experiment crate based on the rCore tutorial. It demonstrates how a
batch-processing kernel can load multiple user programs one by one, switch
between privilege levels, handle traps, and use a custom syscall to assemble a
Tangram-style `OS` logo piece by piece on a VirtIO-GPU framebuffer.

This crate is meant for learning, not for building a general graphics stack.
The teaching focus is on the OS path:

1. one user program issues one `ecall`
2. the kernel traps into S-mode
3. the kernel draws one piece
4. the framebuffer is flushed immediately
5. the batch loader advances to the next user program

## Learning goals

By running and reading this crate, you can practice:

- batch loading and sequential execution of multiple user programs
- U-mode to S-mode trap transitions on RISC-V
- syscall ABI conventions (`a0-a5`, `a7`, return value in `a0`)
- why `fence.i` matters when multiple apps reuse one load address
- how kernel-owned graphics state can persist across process exits

## What the experiment does

- boots a Chapter 2 kernel on `qemu-system-riscv64`
- initializes a VirtIO-GPU framebuffer
- loads seven user programs in order
- each user program requests `draw_piece(piece_id)` through syscall id `1043`
- the kernel renders that piece and flushes the framebuffer immediately
- the screen is not cleared between apps, so the Tangram `OS` logo appears as
  a step-by-step animation

## Project layout

```text
.
├── .cargo/config.toml      # default RISC-V target and QEMU runner
├── Cargo.toml              # published crate metadata
├── Makefile                # convenience wrappers around cargo commands
├── README.md               # learning and reproduction guide
├── build.rs                # bundles chapter apps into the kernel image
├── docs/
│   └── tangram-design.md   # design notes for the experiment
├── src/
│   ├── graphics.rs         # VirtIO-GPU init and Tangram rendering
│   └── main.rs             # batch loader, traps, syscall handling
├── test.sh                 # non-interactive regression test
└── user-src/               # bundled user-program source templates for this crate
```

## Environment requirements

- Rust stable with edition 2024 support
- target: `riscv64gc-unknown-none-elf`
- `qemu-system-riscv64`
- `rust-objcopy` from `cargo-binutils`

The crate ships `rust-toolchain.toml` that pins the required components and
target. You still need QEMU installed on the host.

### Install prerequisites

```bash
rustup target add riscv64gc-unknown-none-elf
cargo install cargo-binutils
rustup component add llvm-tools-preview
```

On macOS:

```bash
brew install qemu
```

On Ubuntu/Debian:

```bash
sudo apt update
sudo apt install qemu-system-misc
```

## Build and run

### Option 1: `cargo run`

```bash
cargo run
```

The crate's `.cargo/config.toml` already sets:

- target: `riscv64gc-unknown-none-elf`
- runner: `qemu-system-riscv64 -machine virt -display none -bios none -device virtio-gpu-device -serial stdio -monitor none -kernel`

### Option 2: `make run`

```bash
make run
```

### Regression test

```bash
bash ./test.sh
```

The test checks that:

- the GPU framebuffer initializes
- piece `0` and piece `6` are drawn
- batch exit codes advance from `app0` to `app6`
- no unexpected trap kills a user program

## Reproduce from crates.io

If you use `cargo-clone`:

```bash
cargo clone cg-tg-rcore-tutorial-t3l2
cd cg-tg-rcore-tutorial-t3l2
cargo run
```

or:

```bash
cargo clone cg-tg-rcore-tutorial-t3l2
cd cg-tg-rcore-tutorial-t3l2
make run
```

## Reproduce from git

```bash
git clone https://github.com/cg24-THU/tg-rcore-tutorial.git
cd tg-rcore-tutorial/tg-rcore-tutorial-ch2
cargo run
```

or:

```bash
git clone https://github.com/cg24-THU/tg-rcore-tutorial.git
cd tg-rcore-tutorial/tg-rcore-tutorial-ch2
make run
```

## Expected console output

You should see a sequence similar to:

```text
[ INFO] VirtIO-GPU framebuffer ready: 1280x800
[ INFO] load app0 to 0x82000000
[ INFO] request draw piece: 0
[ INFO] gpu.flush() success
[ INFO] app0 exit with code 0
...
[ INFO] load app6 to 0x82000000
[ INFO] request draw piece: 6
[ INFO] gpu.flush() success
[ INFO] app6 exit with code 6
```

If you start QEMU with `-vnc :0`, the framebuffer will show the Tangram `OS`
logo assembling step by step.

## Version and tag mapping

- crate name: `cg-tg-rcore-tutorial-t3l2`
- crate version: `0.0.0`
- git tag: `cg-tg-rcore-tutorial-t3l2-v0.0.0`

## Key design choices

- The crate vendors `user-src/` templates, and `build.rs` materializes a tiny
  helper crate in `OUT_DIR` so that `cargo clone && cargo run` works without
  depending on unpublished local workspace edits.
- The kernel handles syscall id `1043` directly before delegating the rest of
  the syscall table to the published `tg-rcore-tutorial-syscall` crate.
- The framebuffer is initialized once and never cleared between apps, making
  the batch-processing effect visible.

The implementation notes live in
[`docs/tangram-design.md`](docs/tangram-design.md).

## Limitations and notes

- The graphics path is intentionally fixed-purpose. It draws a predefined set of
  Tangram polygons instead of exposing a generic graphics API.
- `cargo run` requires QEMU on the host. `cargo build` alone is not enough to
  observe the animation.
- The animation uses a busy-wait delay so each flush is human-visible. This is
  acceptable for Chapter 2, but later chapters could replace it with a timer or
  scheduler-driven delay.

## Repository

- Repository: <https://github.com/cg24-THU/tg-rcore-tutorial>
- Crate page: <https://crates.io/crates/cg-tg-rcore-tutorial-t3l2>
