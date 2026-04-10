# Chapter 1 Companion

This package uses chapter 2 as the default runnable target, but it also bundles the chapter-1 SMP demo as `cargo run --bin ch1_smp`.

The chapter-1 companion demo focuses on two early-SMP ideas:

- each hart must use a different boot stack
- console output should be serialized as a whole string, not just per byte

That demo is intentionally minimal so learners can isolate the bring-up mechanics before studying chapter 2 batch execution.
