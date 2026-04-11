# cg-tg-rcore-tutorial-ch8

`cg-tg-rcore-tutorial-ch8` is a reproducible Rust learning crate built from the `tg-rcore-tutorial` chapter 8 experiment. It preserves the original chapter-8 teaching goals around threads, synchronization primitives, and deadlock detection, and it keeps the framebuffer / keyboard path that was later added for a user-mode DoomGeneric demo on QEMU.

This crate is prepared for two concrete use cases:

- learners reading and extending a non-trivial RISC-V teaching kernel
- TAs or instructors who need a crate that can be cloned, built, and run again with minimal hidden setup

## Project Intro

The kernel starts from the standard chapter 8 structure:

- `Process` is the shared resource container
- `Thread` is the execution unit
- blocking `Mutex`, `Semaphore`, and `Condvar` syscalls are wired into the scheduler
- chapter exercise mode adds deadlock detection for mutexes and semaphores

The current experiment snapshot also includes:

- VirtIO GPU framebuffer support
- VirtIO keyboard input support
- user-space DoomGeneric porting glue
- easy-fs packing of `doom1.wad` when a shareware WAD is available

## Learning Goals

This crate is meant to help you learn and practice:

- the difference between process-as-resource-container and thread-as-execution-unit
- thread creation, join, and blocking state transitions
- kernel synchronization primitives in a teaching OS
- deadlock detection with wait-for graph and safety-check style reasoning
- framebuffer output and keyboard event delivery in a small RISC-V kernel
- how to port a user-space graphical program onto a teaching kernel

## Features

- RISC-V64 `no_std` kernel for QEMU virt
- chapter 8 thread and synchronization support
- exercise-mode deadlock detection
- VirtIO block, GPU, and keyboard drivers
- integrated user app packaging through easy-fs
- bundled `tg-user` snapshot so `cargo clone && cargo run` does not depend on a drifting external user crate
- optional Doom shareware WAD auto-detection via `TG_DOOM_WAD`

## Project Structure

```text
tg-rcore-tutorial-ch8/
├── .cargo/config.toml   # target + QEMU runner
├── build.rs             # builds user apps and packs fs.img
├── Cargo.toml           # release metadata for crates.io
├── docs/reproduce.md    # reproducible setup notes
├── exercise.md          # chapter exercise description
├── report.md            # implementation notes
├── tg-user/             # bundled user-space app snapshot with DoomGeneric support
├── Makefile             # convenience wrappers
└── src/
    ├── main.rs          # kernel init, trap loop, syscall dispatch
    ├── process.rs       # Process / Thread split and deadlock state
    ├── processor.rs     # scheduler-side thread management
    ├── fs.rs            # file descriptor abstractions
    ├── virtio_block.rs  # block device driver
    ├── virtio_gpu.rs    # framebuffer support
    └── virtio_input.rs  # keyboard input support
```

## Environment Requirements

- Rust stable `>= 1.85`
- target `riscv64gc-unknown-none-elf`
- QEMU with `qemu-system-riscv64`
- a C toolchain for bundled Doom support:
  - `riscv64-unknown-elf-gcc`
  - `riscv64-unknown-elf-ar`
  - picolibc for `riscv64-unknown-elf`
- optional shareware WAD for Doom:
  - set `TG_DOOM_WAD=/path/to/doom1.wad`
  - or install `doom-wad-shareware`

Minimum Rust setup:

```bash
rustup toolchain install stable
rustup target add riscv64gc-unknown-none-elf
```

On macOS:

```bash
brew install qemu
```

On Debian / Ubuntu:

```bash
sudo apt update
sudo apt install qemu-system-misc gcc-riscv64-unknown-elf picolibc-riscv64-unknown-elf
```

## Build And Run

Base chapter run:

```bash
cargo run
```

Exercise mode:

```bash
cargo run --features exercise
```

Convenience wrappers:

```bash
make run
make run-exercise
```

After the kernel boots into `Rust user shell`, you can run normal chapter-8 user programs such as `threads`, `sync_sem`, and `test_condvar`. If a WAD has been packed into `fs.img`, you can also run:

```text
doom
```

## Reproduce

### Option 1: clone from crates.io

```bash
cargo clone cg-tg-rcore-tutorial-ch8
cd cg-tg-rcore-tutorial-ch8
cargo run
```

Or:

```bash
cargo clone cg-tg-rcore-tutorial-ch8
cd cg-tg-rcore-tutorial-ch8
make run
```

### Option 2: clone from git

```bash
git clone https://github.com/cg24-THU/tg-rcore-tutorial.git
cd tg-rcore-tutorial/tg-rcore-tutorial-ch8
cargo run
```

Or:

```bash
git clone https://github.com/cg24-THU/tg-rcore-tutorial.git
cd tg-rcore-tutorial/tg-rcore-tutorial-ch8
make run
```

## Test And Validation

Base checker:

```bash
bash ./test.sh base
```

The test script forces a headless QEMU runner, so it works in CI or Docker environments without SDL / X11.

Exercise checker:

```bash
bash ./test.sh exercise
```

Both:

```bash
bash ./test.sh all
```

## Example Output

Typical boot log:

```text
[ INFO] .text ----> 0x80200000..0x8026xxxx
[ INFO] MMIO range -> 0x10001000..0x10002000
Rust user shell
>> threads
```

If Doom support is active, you should also see:

```text
doom: framebuffer 1280x800 stride=5120 format=1
doom: first frame flushed
```

## Version And Tag Mapping

- crate version: `0.0.0`
- git tag: `v0.0.0`

Release mapping:

```text
v0.0.0 -> cg-tg-rcore-tutorial-ch8 0.0.0
```

## Included Learning Documents

- [`exercise.md`](./exercise.md): original chapter-8 exercise statement
- [`report.md`](./report.md): implementation notes and debugging record
- [`docs/reproduce.md`](./docs/reproduce.md): reproducible setup checklist for reviewers

## Notes

- this is a teaching kernel, not a production OS
- `cargo run` uses the default graphical QEMU runner with VirtIO GPU + keyboard enabled
- `bash ./test.sh ...` overrides the runner to a headless serial-only QEMU command for automated checking
- Doom is optional at build time; the kernel itself still runs without a WAD
- the bundled `tg-user` snapshot is included specifically so the published crate remains reproducible even when upstream user crates evolve
