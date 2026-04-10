# cg-tg-rcore-tutorial-ch8

`cg-tg-rcore-tutorial-ch8` is a reproducible and publishable Rust crate for chapter 8 of the `tg-rcore-tutorial` operating-system labs. It packages a RISC-V teaching kernel focused on concurrency: threads, synchronization primitives, and deadlock detection.

This crate is designed to be both:

- a learning-oriented OS kernel artifact for students
- a reproducible submission artifact for TAs and instructors

## Project Introduction

This experiment turns the earlier single-threaded process model into a thread-aware kernel:

- `Process` is the resource container
- `Thread` is the execution unit
- PID/TID relations are maintained by `PThreadManager`
- blocking synchronization is provided through mutexes, semaphores, and condition variables
- the chapter exercise adds deadlock detection for mutex and semaphore requests

The current crate includes the completed exercise result for chapter 8.

## Learning Goals

This crate helps learners study and practice:

- the difference between process resources and thread execution state
- thread creation, exit, and join in a kernel
- blocking synchronization and wake-up paths
- how trap handling, syscalls, and the scheduler interact
- deadlock detection strategies in a teaching OS kernel

## Functionality

- RISC-V64 no-std kernel startup
- user ELF loading and easy-fs image packing
- thread-aware scheduling and PID/TID bookkeeping
- blocking `Mutex`, `Semaphore`, and `Condvar` syscalls
- chapter 8 deadlock detection with `enable_deadlock_detect`
- packaged exercise statement and implementation report for study and grading

## Project Structure

```text
tg-rcore-tutorial-ch8/
├── .cargo/config.toml
├── Cargo.toml
├── Makefile
├── README.md
├── build.rs
├── exercise.md
├── report.md
├── rust-toolchain.toml
├── test.sh
└── src/
    ├── fs.rs
    ├── main.rs
    ├── process.rs
    ├── processor.rs
    ├── virtio_block.rs
    ├── virtio_gpu.rs
    └── virtio_input.rs
```

## Environment Requirements

- Rust stable
- target `riscv64gc-unknown-none-elf`
- `qemu-system-riscv64`
- `cargo-clone`

Recommended setup:

```bash
rustup toolchain install stable
rustup target add riscv64gc-unknown-none-elf
cargo install cargo-clone
```

Install QEMU:

```bash
# macOS
brew install qemu

# Debian / Ubuntu
sudo apt update
sudo apt install qemu-system-misc
```

## Build And Run

Run the normal chapter image:

```bash
cargo run
```

Run the exercise image:

```bash
cargo run --features exercise
```

Convenience wrappers:

```bash
make run
make run-exercise
```

## Reproduce

### From crates.io

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

### From git

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

## Validation

Base checks:

```bash
bash ./test.sh base
```

Exercise checks:

```bash
bash ./test.sh exercise
```

All checks:

```bash
bash ./test.sh all
```

## Notes On User Programs

`build.rs` resolves user programs in this order:

1. `TG_USER_DIR` if explicitly set
2. sibling `../tg-rcore-tutorial-user`
3. `cargo clone tg-rcore-tutorial-user`

That means a standalone clone from crates.io needs `cargo-clone` unless `TG_USER_DIR` is already configured.

## Output Example

Typical boot output includes:

```text
[ INFO] .text ----> 0x80200000..0x80257000
[ INFO] .rodata --> 0x80257000..0x80268000
[ INFO] .data ----> 0x80268000..0x80268420
[ INFO] MMIO range -> 0x10001000..0x10002000
```

Exercise success output should include:

```text
deadlock test mutex 1 OK!
deadlock test semaphore 1 OK!
deadlock test semaphore 2 OK!
```

## Version And Tag

- Crate version: `0.0.0`
- Git tag: `v0.0.0`

Mapping:

```text
v0.0.0 -> cg-tg-rcore-tutorial-ch8 0.0.0
```

## Included Documents

- [`exercise.md`](./exercise.md): exercise requirements
- [`report.md`](./report.md): implementation notes and debug record

These files are intentionally included in the published package so the crate can help both the author and other learners improve OS kernel understanding through direct reading and reproduction.

## Limitations

- this is a teaching kernel, not a production OS
- deadlock detection follows the chapter scope and handles mutexes and semaphores separately
- `cargo run` requires a working local QEMU installation
- the first standalone build may need network access for user-program fetching
