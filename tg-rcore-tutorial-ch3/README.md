# cg-tg-rcore-tutorial-ch3

`cg-tg-rcore-tutorial-ch3` 是一个面向学习者整理的 rCore Tutorial 第三章实验 crate。它保留了 Chapter 3 的核心实验代码、练习说明、调试报告和自动测试脚本，目标是让助教、老师和其他学习者可以直接构建、运行、阅读并复现实验结果。

本 crate 聚焦的主题是“多道程序与分时多任务”：

- 任务控制块（TCB）
- 用户态到内核态的 Trap 返回链路
- `yield` 驱动的协作式调度
- 时钟中断驱动的抢占式调度
- `clock_gettime` 与 `trace` 系统调用

## 学习目标

通过这个 crate，可以系统理解并练习以下操作系统内核知识：

- 多道程序系统与串行批处理系统的差异
- 任务控制块如何封装“上下文 + 栈 + 生命周期状态”
- 为什么时钟中断能够实现抢占式调度
- `yield` 与时间片轮转各自对应怎样的调度语义
- Trap 进入内核后，寄存器状态如何保存、修改并返回用户态
- 在没有地址空间隔离的早期内核中，系统调用如何直接访问用户地址
- 如何为一个教学型 OS 实验补齐文档、报告与可复现发布流程

## 功能说明

本 crate 提供了一个可直接 `cargo run` 的 RISC-V 裸机内核实验工程：

- 默认运行 Chapter 3 基础用户程序集合
- 启用 `exercise` feature 后运行扩展练习用例
- 启用 `coop` feature 后切换到协作式调度模式，便于对比与默认抢占式模式的差异
- 构建时会根据 `.cargo/config.toml` 自动获取 `tg-rcore-tutorial-user`
- 附带 `exercise.md` 与 `report.md`，方便阅卷、复盘和二次学习

## 项目结构

```text
cg-tg-rcore-tutorial-ch3/
├── .cargo/config.toml
├── Cargo.toml
├── LICENSE
├── Makefile
├── README.md
├── build.rs
├── exercise.md
├── report.md
├── rust-toolchain.toml
├── test.sh
└── src/
    ├── main.rs   # 内核入口、主调度循环、syscall trait 实现
    └── task.rs   # TCB、调度事件、syscall 统计
```

关键阅读入口：

- `src/task.rs`：任务控制块、`handle_syscall()`、`trace` 计数关联
- `src/main.rs`：Trap 处理、轮转调度、Clock / Scheduling / Trace syscall 实现
- `exercise.md`：原始实验要求
- `report.md`：真实 Debug 过程与最终实现说明

## 环境要求

- Rust stable 1.85 或更高版本
- `riscv64gc-unknown-none-elf` 目标
- `qemu-system-riscv64`
- `cargo-clone`
- `cargo-binutils` 与 `llvm-tools-preview`

推荐初始化命令：

```bash
rustup toolchain install stable
rustup target add riscv64gc-unknown-none-elf
rustup component add llvm-tools-preview rustfmt
cargo install cargo-clone cargo-binutils
```

确认 QEMU 可用：

```bash
qemu-system-riscv64 --version
```

## 构建与运行

这个 crate 的标准运行方式是 `cargo run`。同时也提供了等价的 `make run` 便捷入口。

### 1. 基础模式

```bash
cargo run
```

或：

```bash
make run
```

### 2. 协作式调度模式

```bash
cargo run --features coop
```

或：

```bash
make run-coop
```

### 3. 练习模式

```bash
cargo run --features exercise
```

或：

```bash
make run-exercise
```

### 4. 自动测试

```bash
bash ./test.sh base
bash ./test.sh exercise
```

或：

```bash
make test-base
make test-exercise
```

## 复现方式

### 方式一：通过 crates.io 获取

`cargo clone` 来自 `cargo-clone`，若本机未安装，请先执行 `cargo install cargo-clone`。

```bash
cargo clone cg-tg-rcore-tutorial-ch3
cd cg-tg-rcore-tutorial-ch3
cargo run
```

练习模式：

```bash
cargo run --features exercise
```

### 方式二：通过 Git 仓库获取

当前实际仓库是 monorepo，因此 Git 复现时需要进入本 crate 对应的子目录。这是当前仓库的真实布局。

```bash
git clone https://github.com/cg24-THU/tg-rcore-tutorial.git
cd tg-rcore-tutorial/tg-rcore-tutorial-ch3
cargo run
```

也可以使用：

```bash
make run
```

## 版本与 Tag

- Crate version: `0.0.1`
- Repository: `https://github.com/cg24-THU/tg-rcore-tutorial`
- Crate subdir: `tg-rcore-tutorial-ch3`
- Git tag for this crate: `cg-tg-rcore-tutorial-ch3-v0.0.1`

本 README 描述的发布内容与 `cg-tg-rcore-tutorial-ch3-v0.0.1` 对应。

## 输出示例

基础模式运行时会出现类似输出：

```text
Hello, world from user mode program!
Test power OK!
Test power_3 OK!
Test power_5 OK!
Test power_7 OK!
Test write A OK!
Test write B OK!
Test write C OK!
Test sleep OK!
```

练习模式通过时会出现类似输出：

```text
get_time OK! 10
current time_msec = 13
Test sleep1 passed!
string from task trace test
Test trace OK!
```

这些现象分别对应：

- 多个用户程序在抢占式调度下交错执行
- `clock_gettime` 返回随时间单调增加
- `trace` 能够读写当前任务用户内存，并查询 syscall 计数

## 课程文档与学习材料

- `exercise.md`：原始实验要求与接口定义
- `report.md`：实现思路、关键代码片段与真实 Debug 记录

建议阅读顺序：

1. `exercise.md`
2. `src/task.rs`
3. `src/main.rs`
4. `report.md`

## 注意事项与局限性

- 本 crate 是 `no_std + bare-metal + riscv64` 的教学内核工程，不支持在普通宿主架构上直接执行最终内核二进制。
- `cargo run` 依赖 `.cargo/config.toml` 中的 QEMU runner；若本机没有 `qemu-system-riscv64`，运行会失败。
- 发布包不会直接携带 `tg-rcore-tutorial-user` 源码目录；首次构建时会按 `.cargo/config.toml` 中的 `TG_USER_VERSION` 自动获取，或者使用 `TG_USER_DIR=/path/to/tg-rcore-tutorial-user` 指向本地副本。
- 由于这是 bare-metal 二进制 crate，`cargo test` 在默认配置下并不提供常规宿主机 test harness；发布验收主要依赖 `cargo build`、`cargo package`、`cargo publish --dry-run`、`cargo run` 和 `test.sh`。
- 当前 Git 仓库是多章节 monorepo，因此通过 Git 方式复现时需要进入 `tg-rcore-tutorial-ch3/` 子目录。

## Repository

- Repository: <https://github.com/cg24-THU/tg-rcore-tutorial>
- Crate subdir: <https://github.com/cg24-THU/tg-rcore-tutorial/tree/test/tg-rcore-tutorial-ch3>
