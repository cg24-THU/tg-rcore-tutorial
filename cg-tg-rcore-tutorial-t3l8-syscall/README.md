# cg-tg-rcore-tutorial-t3l8-syscall

This crate contains the syscall definitions and user/kernel interfaces required by the published `cg-tg-rcore-tutorial-t3l8` chapter-8 kernel snapshot.

Compared with the upstream tutorial syscall crate, this release keeps the extra interfaces needed by the framebuffer, keyboard input, and `lseek` path used by the user-mode Doom experiment.

It is published as a support crate for the chapter-8 release and is not intended as an independent learning deliverable.
