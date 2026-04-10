# Reproducibility Notes

This document supplements the crate README for teaching and grading.

## What To Expect

The kernel boots in RISC-V S-mode, scans the VirtIO-MMIO slots on QEMU `virt`, finds a VirtIO-GPU device, allocates a framebuffer, rasterizes a tangram-style `OS` image, and flushes the framebuffer to the host display.

## Graphical Reproduction

```bash
cargo run
```

This requires a usable GUI environment for QEMU.

## Headless Reproduction

```bash
make run-headless
```

Expected serial log:

```text
Booting ch1-tangram...
VirtIO-GPU framebuffer ready: 1280x800
Tangram OS rendered.
```

## Verification Script

```bash
bash test.sh
```

The script builds the crate, runs QEMU headlessly, captures the serial log, and checks that the rendering path completed.
