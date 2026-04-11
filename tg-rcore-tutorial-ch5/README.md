# cg-tg-rcore-tutorial-ch5

`cg-tg-rcore-tutorial-ch5` 是一个面向学习者整理的 rCore Tutorial 第五章实验 crate。它聚焦 RISC-V 裸机内核中的进程管理主题，保留了实验代码、复现文档、测试脚本与实验报告，目标是让助教、老师和其他学习者能够直接构建、运行、阅读和复现实验结果。

本 crate 对应的核心实验内容包括：

- 进程装载与地址空间初始化
- `fork` / `exec` / `waitpid` / `getpid`
- `spawn` 系统调用
- stride 调度与 `set_priority`
- `sbrk`
- `mmap` / `munmap`

## 学习目标

通过这个 crate，可以系统练习和理解以下操作系统内核知识：

- 进程与任务的区别，以及进程控制块需要维护哪些状态
- 如何从 ELF 创建用户进程，并建立用户栈与地址空间
- `fork` 深拷贝地址空间的语义
- `exec` 如何替换当前进程执行映像
- 父子进程关系、僵尸进程、`waitpid` 资源回收
- 为什么 `spawn` 可以在某些场景下替代 `fork + exec`
- stride 调度如何让 CPU 时间近似按优先级比例分配
- 用户态匿名内存映射与解除映射的基本实现思路

## 功能说明

本 crate 提供了一个可直接 `cargo run` 的 RISC-V 内核实验工程：

- 默认运行 chapter 5 的基础用户程序集
- 开启 `exercise` feature 后运行扩展练习测例
- 构建时会按 `.cargo/config.toml` 中的 `TG_USER_VERSION=0.4.8` 自动获取 `tg-rcore-tutorial-user`，也可以通过 `TG_USER_DIR` 指向本地用户态程序目录
- 附带 `exercise.md` 与 `report.md`，方便阅卷与学习复盘

## 项目结构

```text
cg-tg-rcore-tutorial-ch5/
├── .cargo/config.toml
├── Cargo.toml
├── README.md
├── build.rs
├── exercise.md
├── report.md
├── rust-toolchain.toml
├── test.sh
├── src/
│   ├── main.rs
│   ├── process.rs
│   └── processor.rs
└── optional local cache/
    └── tg-rcore-tutorial-user/   # 通过 TG_USER_DIR 或首次构建自动准备
```

关键源码入口：

- `src/main.rs`：内核入口、trap 返回主循环、syscall trait 实现
- `src/process.rs`：进程结构、ELF 装载、`fork`、`exec`、`sbrk`、`mmap`、`munmap`
- `src/processor.rs`：进程管理器与 stride 调度器

## 环境要求

- Rust stable 1.85 或更高版本
- `riscv64gc-unknown-none-elf` 目标
- `qemu-system-riscv64`
- 基本构建环境：`rust-src`、`llvm-tools-preview`、`rustfmt`
- `cargo-clone`，用于首次构建时自动获取 `tg-rcore-tutorial-user`

推荐初始化命令：

```bash
rustup toolchain install stable
rustup target add riscv64gc-unknown-none-elf
rustup component add rust-src llvm-tools-preview rustfmt
cargo install cargo-clone
```

如果你在 Linux / macOS 上本地运行，还需要安装 QEMU：

```bash
qemu-system-riscv64 --version
```

## 构建与运行

这个 crate 的标准运行方式是 `cargo run`，当前没有提供 `make run`。

### 1. 基础模式

```bash
cargo run
```

### 2. 练习模式

```bash
cargo run --features exercise
```

### 3. 运行测试

```bash
bash ./test.sh base
bash ./test.sh exercise
```

## 复现方式

### 方式一：通过 crates.io 获取

需要先安装 `cargo-clone`，因为 `cargo clone` 和首次构建都依赖它：

```bash
cargo install cargo-clone
cargo clone cg-tg-rcore-tutorial-ch5
cd cg-tg-rcore-tutorial-ch5
cargo run
```

练习模式：

```bash
cargo run --features exercise
```

### 方式二：通过 Git 仓库获取

当前实际仓库是一个 monorepo，因此从 Git 复现时需要进入本 crate 对应的子目录。这是当前仓库的真实布局。

```bash
git clone https://github.com/cg24-THU/tg-rcore-tutorial.git
cd tg-rcore-tutorial/tg-rcore-tutorial-ch5
cargo run
```

练习模式：

```bash
cargo run --features exercise
```

## 版本与 Tag

- Crate version: `0.0.1`
- Git tag: `cg-tg-rcore-tutorial-ch5-v0.0.1`

本 README 所描述的发布版本与 tag `cg-tg-rcore-tutorial-ch5-v0.0.1` 对应。

## 输出示例

基础运行时会看到类似输出：

```text
Hello, world from user mode program!
Test power_3 OK!
Test power_5 OK!
Test power_7 OK!
hello child process!
forktest pass.
```

练习测例通过时会看到类似输出：

```text
Test spawn0 OK!
Test wait OK!
Test waitpid OK!
Test set_priority OK!
ch5 Usertests passed!
```

## 课程文档与学习材料

- `exercise.md`：原始实验要求与任务说明
- `report.md`：本次实验的实现说明与真实 Debug 记录

如果你想从“做完实验”进一步过渡到“讲清楚实验”，建议阅读顺序如下：

1. `exercise.md`
2. `src/process.rs`
3. `src/processor.rs`
4. `src/main.rs`
5. `report.md`

## 注意事项与局限性

- 本 crate 运行目标是 RISC-V 裸机环境，不支持普通宿主架构直接执行最终二进制。
- `cargo run` 依赖 QEMU runner；如果你的环境没有 `qemu-system-riscv64`，运行会失败。
- 发布包不会内嵌 `tg-rcore-tutorial-user` 源码目录；首次 `cargo run` 时会按 `.cargo/config.toml` 中的版本自动拉取该 crate，或者你可以手动设置 `TG_USER_DIR=/path/to/tg-rcore-tutorial-user`。
- 用户程序构建默认不启用 Doom 相关支持；本 crate 的 chapter 5 学习与测试不依赖该路径。
- `cargo test` 对这个 `no_std + riscv64 bare-metal` 二进制 crate 不适用，默认会因为缺少 test harness 而报 `can't find crate for 'test'`；发布验收应以 `cargo build`、`cargo package`、`cargo publish --dry-run` 和实际 `cargo run` / `test.sh` 为准。
- 当前 Git 仓库是多章节 monorepo，因此 Git 方式复现时需要进入 `tg-rcore-tutorial-ch5/` 子目录。

## Repository

- Repository: <https://github.com/cg24-THU/tg-rcore-tutorial>
- Crate subdir: <https://github.com/cg24-THU/tg-rcore-tutorial/tree/test/tg-rcore-tutorial-ch5>
