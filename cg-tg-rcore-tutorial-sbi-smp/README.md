# cg-tg-rcore-tutorial-sbi-smp

`cg-tg-rcore-tutorial-sbi-smp` is the shared SBI support crate used by the publishable learning crates:

- `cg-tg-rcore-tutorial-t2l9`
- `cg-tg-rcore-tutorial-t2l11`

It keeps the `nobios` M-mode path needed by the SMP-safe chapter experiments:

- per-hart M-mode entry
- `mhartid`-aware bring-up
- console / timer / shutdown SBI wrappers

The crate is published separately so the main learning crates can be reproduced with a plain:

```bash
cargo clone <main-crate>
cd <main-crate>
cargo run
```

without depending on unpublished local path crates.
