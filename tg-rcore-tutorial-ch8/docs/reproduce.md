# Reproducibility Notes

This crate is published as a learning snapshot of the `tg-rcore-tutorial` chapter 8 experiment.

## What Is Bundled

- the kernel crate itself
- chapter exercise notes and report
- a bundled `tg-user` snapshot used by `build.rs`
- DoomGeneric-related user-space support code

## What Is Not Bundled

- QEMU
- the RISC-V cross C toolchain
- a Doom WAD file

## Reviewer Checklist

1. Install Rust stable and `riscv64gc-unknown-none-elf`.
2. Install `qemu-system-riscv64`.
3. Install `riscv64-unknown-elf-gcc` and picolibc if you want the bundled Doom user app to compile.
4. Run `cargo run` or `make run`.
5. Use `bash ./test.sh base` and `bash ./test.sh exercise` for chapter checks.
6. If you want to try Doom, provide `TG_DOOM_WAD=/path/to/doom1.wad` before building.
