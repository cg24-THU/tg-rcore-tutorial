# cg-tg-rcore-tutorial-t2l9

`cg-tg-rcore-tutorial-t2l9` is a reproducible Rust learning crate built from the completed `tg-rcore-tutorial` task 1 experiment: the SMP extension for chapter 1 and chapter 2.

The crate keeps the experiment in a publishable shape:

- `cargo clone cg-tg-rcore-tutorial-t2l9 && cd cg-tg-rcore-tutorial-t2l9 && cargo run`
- `git clone https://github.com/cg24-THU/tg-rcore-tutorial.git && cd tg-rcore-tutorial/cg-tg-rcore-tutorial-t2l9 && cargo run`

The default runnable target is the chapter-2 SMP kernel. The package also bundles a companion binary for the chapter-1 SMP demo.

## 1. Project Summary

This crate corresponds to the teaching task usually described as `t2l9`:

- chapter 1: per-hart stacks and serialized console output
- chapter 2: multi-hart startup with primary/secondary separation

The design goal is not “true parallel kernel scheduling”, but a stable **SMP-safe boot path**:

- every hart gets its own boot stack
- only hart 0 performs one-time initialization
- secondary harts announce themselves and then park
- chapter-2 batch execution still proceeds on the primary hart

## 2. Learning Goals

This crate helps learners study:

- why a shared boot stack is fatal on SMP
- how `mhartid` is used to distinguish harts
- why `zero_bss` and one-time initialization must only happen once
- why serialized console output matters in early-kernel debugging
- how to evolve a single-core teaching kernel into a multi-core safe kernel without breaking the chapter logic

## 3. Functional Scope

The package contains two runnable artifacts:

- default binary: chapter-2 SMP kernel
- companion binary: `ch1_smp`

The default binary demonstrates:

- 4-hart startup in QEMU
- primary-only batch execution
- secondary harts parked before entering the chapter-2 execution path

The companion binary demonstrates:

- per-hart boot stacks
- ordered console output with a spinlock

## 4. Project Layout

```text
.
├── .cargo/config.toml     # target + QEMU runner + bundled tg-user config
├── Makefile               # make run / make run-ch1 / build / package helpers
├── build.rs               # chapter-2 app image generation
├── report.md              # original task report for ch1/ch2 SMP extension
├── docs/
│   ├── ch1-companion.md   # chapter-1 companion explanation
│   └── publish-notes.md   # publish/reproduce notes
├── src/
│   ├── main.rs            # chapter-2 SMP kernel
│   └── bin/ch1_smp.rs     # chapter-1 SMP companion binary
└── tg-user/               # bundled user-space apps used by build.rs
```

## 5. Environment Requirements

- Rust toolchain: stable
- target: `riscv64gc-unknown-none-elf`
- `rust-objcopy`
- `qemu-system-riscv64`

Recommended setup:

```bash
rustup target add riscv64gc-unknown-none-elf
cargo install cargo-binutils
rustup component add llvm-tools-preview
```

## 6. Build And Run

### 6.1 Default run

```bash
cargo run
```

This runs the chapter-2 SMP kernel with `-smp 4`.

Expected observation:

```text
hart 1 is online but parked before batch execution
hart 2 is online but parked before batch execution
hart 3 is online but parked before batch execution
[ INFO] all secondary harts are parked; boot hart continues with apps
```

### 6.2 Companion chapter-1 demo

```bash
cargo run --bin ch1_smp
```

Expected observation:

```text
Hello from hart 0
Hello from hart 1
Hello from hart 2
Hello from hart 3
```

The order is not fixed, but each line should remain intact.

### 6.3 Make shortcuts

```bash
make run
make run-ch1
make build
make package
```

## 7. Reproduce

### 7.1 Via crates.io

```bash
cargo clone cg-tg-rcore-tutorial-t2l9
cd cg-tg-rcore-tutorial-t2l9
cargo run
```

Or:

```bash
cargo clone cg-tg-rcore-tutorial-t2l9
cd cg-tg-rcore-tutorial-t2l9
make run
```

### 7.2 Via git

```bash
git clone https://github.com/cg24-THU/tg-rcore-tutorial.git
cd tg-rcore-tutorial/cg-tg-rcore-tutorial-t2l9
cargo run
```

Or:

```bash
git clone https://github.com/cg24-THU/tg-rcore-tutorial.git
cd tg-rcore-tutorial/cg-tg-rcore-tutorial-t2l9
make run
```

## 8. Version And Tag

- crate version: `0.0.0`
- task-specific tag used for this release: `cg-tg-rcore-tutorial-t2l9-v0.0.0`

Note:
The repository already contains an older top-level tag named `v0.0.0`. Because one git repository cannot use the same tag name for two different release states, this crate uses a task-specific tag while keeping the crate version at `0.0.0`.

## 9. Documentation And Learning Materials

- [report.md](report.md): original ch1/ch2 SMP task report
- [docs/ch1-companion.md](docs/ch1-companion.md): chapter-1 companion notes
- [docs/publish-notes.md](docs/publish-notes.md): reproducibility and publish notes

## 10. Limitations

- This crate is SMP-safe, but it is not a true multi-core scheduler.
- Secondary harts are intentionally parked.
- `cargo test` is not used as the release gate because this is a `no_std` bare-metal binary crate without a standard test harness.
