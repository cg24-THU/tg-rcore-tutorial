# cg-tg-rcore-tutorial-t2l11

`cg-tg-rcore-tutorial-t2l11` is a reproducible Rust learning crate built from the completed `tg-rcore-tutorial` task 2 experiment: the SMP extension for chapter 6 and chapter 8.

The default runnable target is the chapter-8 SMP kernel. The package also bundles a companion chapter-6 SMP binary and the task report used during the development loop.

## 1. Project Summary

This crate packages the `t2l11` learning result:

- chapter 6 SMP-safe boot for the filesystem chapter
- chapter 8 SMP-safe boot for the threading/synchronization chapter

The key design principle is:

- all harts boot correctly
- each hart gets its own boot stack
- only the primary hart performs one-time initialization and enters the chapter logic
- secondary harts announce themselves and park in `wfi`

That keeps the original single-core teaching assumptions intact while still making the kernel safe to boot on a 4-hart QEMU machine.

## 2. Learning Goals

This crate helps learners study:

- why a global `PROCESSOR` is not automatically SMP-safe
- why filesystems, schedulers, and synchronization state should not be entered by multiple harts prematurely
- how to separate “multi-core boot works” from “the kernel is truly parallel”
- how to preserve chapter-level teaching behavior while extending the boot path for SMP

## 3. Functional Scope

The package contains:

- default binary: chapter-8 SMP kernel
- companion binary: chapter-6 SMP kernel under `src/bin/ch6_smp`
- bundled `tg-user/` snapshot for reproducible builds
- task report and extra notes

### Default run

```bash
cargo run
```

Expected observation:

```text
hart 1 is online but parked before thread scheduling
hart 2 is online but parked before thread scheduling
hart 3 is online but parked before thread scheduling
[ INFO] all secondary harts are parked; boot hart continues with thread scheduling
```

### Companion chapter-6 run

```bash
make run-ch6
```

Expected observation:

```text
hart 1 is online but parked before filesystem scheduling
hart 2 is online but parked before filesystem scheduling
hart 3 is online but parked before filesystem scheduling
[ INFO] all secondary harts are parked; boot hart continues with filesystem init
```

## 4. Project Layout

```text
.
├── .cargo/config.toml
├── Makefile
├── build.rs
├── report_ch6_ch8_smp.md
├── docs/
│   ├── ch6-companion.md
│   └── publish-notes.md
├── src/
│   ├── main.rs                 # chapter-8 SMP kernel
│   ├── fs.rs
│   ├── process.rs
│   ├── processor.rs
│   ├── virtio_block.rs
│   └── bin/ch6_smp/**         # chapter-6 companion binary
└── tg-user/
```

## 5. Environment Requirements

- Rust toolchain: stable
- target: `riscv64gc-unknown-none-elf`
- `qemu-system-riscv64`
- `rust-objcopy`

Recommended setup:

```bash
rustup target add riscv64gc-unknown-none-elf
cargo install cargo-binutils
rustup component add llvm-tools-preview
```

## 6. Build And Run

### 6.1 Default chapter-8 path

```bash
cargo run
```

Or:

```bash
make run
```

### 6.2 Chapter-8 exercise path

```bash
cargo run --features exercise
```

Or:

```bash
make run-exercise
```

### 6.3 Companion chapter-6 path

```bash
make run-ch6
```

### 6.4 Automated checks

```bash
bash test.sh base
bash test.sh exercise
```

## 7. Reproduce

### 7.1 Via crates.io

```bash
cargo clone cg-tg-rcore-tutorial-t2l11
cd cg-tg-rcore-tutorial-t2l11
cargo run
```

Or:

```bash
cargo clone cg-tg-rcore-tutorial-t2l11
cd cg-tg-rcore-tutorial-t2l11
make run
```

### 7.2 Via git

```bash
git clone https://github.com/cg24-THU/tg-rcore-tutorial.git
cd tg-rcore-tutorial/cg-tg-rcore-tutorial-t2l11
cargo run
```

Or:

```bash
git clone https://github.com/cg24-THU/tg-rcore-tutorial.git
cd tg-rcore-tutorial/cg-tg-rcore-tutorial-t2l11
make run
```

## 8. Version And Tag

- crate version: `0.0.0`
- task-specific tag used for this release: `cg-tg-rcore-tutorial-t2l11-v0.0.0`

Note:
The repository already contains an older top-level tag named `v0.0.0`. Because one git repository cannot bind the same tag name to two different release states, this crate uses a task-specific tag while keeping the crate version at `0.0.0`.

## 9. Documentation And Learning Materials

- [report_ch6_ch8_smp.md](report_ch6_ch8_smp.md): full task report
- [docs/ch6-companion.md](docs/ch6-companion.md): chapter-6 companion notes
- [docs/publish-notes.md](docs/publish-notes.md): publish/reproduce notes

## 10. Limitations

- This crate is SMP-safe boot, not a full multi-hart scheduler.
- Secondary harts are parked on purpose.
- `cargo test` is not used as the release gate because this is a `no_std` bare-metal binary crate without a standard test harness.
