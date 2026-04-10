# Moving Tangram Design Notes

`cg-tg-rcore-tutorial-t3l2` is a Chapter 2 teaching crate derived from the
batch-processing kernel in the rCore tutorial. The experiment is intentionally
small and fixed-purpose so that the OS concepts stay visible.

## What this crate teaches

1. Batch loading of multiple user programs into one kernel image.
2. Trap entry/return between U-mode and S-mode.
3. System call dispatch through the `ecall` convention.
4. Why `fence.i` is required when one physical load address is reused for
   different applications.
5. How a kernel-owned framebuffer can preserve state across process exits.

## Why the crate vendors user-program sources

The original workspace implementation depended on a sibling
`tg-rcore-tutorial-user` crate with locally extended syscall bindings. A crate
published to crates.io cannot rely on unpublished local path changes, so this
package ships a minimal `user-src/` source bundle and lets `build.rs` generate
a temporary helper crate inside `OUT_DIR`.

That helper crate contains exactly seven user programs:

- `ch2_draw_piece_0`
- `ch2_draw_piece_1`
- `ch2_draw_piece_2`
- `ch2_draw_piece_3`
- `ch2_draw_piece_4`
- `ch2_draw_piece_5`
- `ch2_draw_piece_6`

Each program does one thing:

1. issue `ecall` with syscall id `1043` and its piece id in `a0`
2. receive a return code in `a0`
3. call `exit`

This keeps the user side readable while preserving the chapter's focus on trap
handling instead of graphics library design.

## Why the kernel handles `draw_piece` directly

The upstream `tg-rcore-tutorial-syscall` crate published on crates.io does not
define the custom `draw_piece` API used by this experiment. To keep this crate
publishable without introducing a second unpublished dependency, the kernel
checks for syscall id `1043` before delegating other requests to the upstream
syscall dispatcher.

This preserves compatibility with the published base crates while keeping the
teaching point explicit: a system call is ultimately just a numeric ABI between
user and kernel.
