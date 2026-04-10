# cg-tg-rcore-tutorial-ch6

`cg-tg-rcore-tutorial-ch6` 是基于 rCore Tutorial 第六章整理出的一个可复现、可学习、可发布的实验 crate。它聚焦于“文件系统进入内核”的关键路径：内核通过 VirtIO 块设备访问磁盘镜像，借助 easy-fs 管理文件，并通过文件描述符表向用户态暴露 `open`、`read`、`write`、`close`、`linkat`、`unlinkat`、`fstat` 等系统调用。

本 crate 保留了实验代码、练习说明、调试报告和自动化测试脚本，既能用于课程验收，也适合作为学习操作系统内核中文件系统与 syscall 设计的样例。

## 1. 项目简介

- 对应实验：rCore Tutorial 第六章，主题是文件系统、easy-fs、VirtIO 块设备和文件相关系统调用。
- 当前实现：支持从 `fs.img` 加载用户程序，支持文件描述符表，完成了 `linkat`、`unlinkat`、`fstat` 练习，并保持 `spawn`、`mmap`、`munmap` 等相关测试可通过。
- 发布目的：让助教、老师和学习者可以直接从 crates.io 或 git 仓库复现本实验。

## 2. 学习目标

通过阅读和运行本 crate，可以重点学习：

- 内核如何通过 MMIO 驱动 VirtIO 块设备。
- easy-fs 的磁盘布局与 inode / dirent 管理方式。
- 文件描述符表如何把标准输入输出和普通文件统一抽象。
- `open/read/write/close` 与 `linkat/unlinkat/fstat` 的内核实现路径。
- 为什么硬链接实现必须同时修改目录项和 inode 的链接计数。
- 为什么“用户程序从内嵌镜像迁移到磁盘镜像”会改变构建与启动方式。

## 3. 功能说明

本 crate 当前包含：

- 一个可运行的 RISC-V 内核二进制。
- `build.rs` 自动构建用户程序并打包 `fs.img`。
- easy-fs 文件系统接入与 VirtIO block 驱动支持。
- 文件系统相关 syscall：
  - `open`
  - `close`
  - `read`
  - `write`
  - `linkat`
  - `unlinkat`
  - `fstat`
- 课程练习文档：[exercise.md](./exercise.md)
- 本次实现报告：[report.md](./report.md)
- 自动化测试脚本：[test.sh](./test.sh)

## 4. 项目结构

```text
tg-rcore-tutorial-ch6/
├── .cargo/config.toml   # 交叉编译目标、QEMU runner、tg-user 配置
├── build.rs             # 构建用户程序并打包 easy-fs 镜像
├── Cargo.toml           # crate 元数据与发布配置
├── LICENSE
├── Makefile             # make run / make test 等快捷入口
├── README.md
├── exercise.md          # 课程练习要求
├── report.md            # 实际实现与 Debug 记录
├── rust-toolchain.toml
├── test.sh              # 官方测试脚本
└── src
    ├── lib.rs           # crate 级学习文档与发布元信息
    ├── main.rs          # 内核入口、Trap 返回处理、syscall 实现
    ├── fs.rs            # easy-fs 封装与 FSManager 实现
    ├── process.rs       # 进程结构、地址空间、fd_table
    ├── processor.rs     # 调度与进程管理器
    └── virtio_block.rs  # VirtIO block 驱动
```

## 5. 环境要求

- Rust 1.94 或更新版本
- `riscv64gc-unknown-none-elf` 目标
- QEMU `qemu-system-riscv64`
- 建议安装 `cargo-clone`

安装示例：

```bash
rustup target add riscv64gc-unknown-none-elf
cargo install cargo-clone
```

`cargo-clone` 的作用有两处：

- 你可以直接用 `cargo clone cg-tg-rcore-tutorial-ch6` 拉取本 crate。
- 当本 crate 在本地缺少 `tg-rcore-tutorial-user` 时，`build.rs` 会自动调用 `cargo clone` 拉取用户态程序包。

## 6. 构建与运行方法

### 6.1 使用 `cargo run`

基础模式：

```bash
cargo run
```

练习模式：

```bash
cargo run --features exercise
```

### 6.2 使用 `make run`

基础模式：

```bash
make run
```

练习模式：

```bash
make run-exercise
```

### 6.3 运行测试

基础测试：

```bash
make test-base
```

练习测试：

```bash
make test-exercise
```

也可以直接运行：

```bash
bash ./test.sh exercise
```

## 7. 复现方法

### 方式一：从 crates.io 获取

```bash
cargo clone cg-tg-rcore-tutorial-ch6
cd cg-tg-rcore-tutorial-ch6
cargo run
```

或：

```bash
make run
```

### 方式二：从 git 仓库获取

```bash
git clone https://github.com/cg24-THU/tg-rcore-tutorial.git
cd tg-rcore-tutorial/tg-rcore-tutorial-ch6
cargo run
```

或：

```bash
make run
```

如果你只想运行练习版本：

```bash
cargo run --features exercise
```

或：

```bash
make run-exercise
```

## 8. 版本与 tag

- Crate name: `cg-tg-rcore-tutorial-ch6`
- Version: `0.0.0`
- Git tag: `v0.0.0`

本 README 与 `report.md` 都以 `v0.0.0` 为课程提交和 crate 发布的对应版本。

## 9. 实验现象与输出示例

在 `cargo run --features exercise` 成功运行后，你会看到类似输出：

```text
[ INFO] MMIO range -> 0x10001000..0x10002000
Usertests: Running ch6_file1
Test fstat OK!
Usertests: Running ch6_file2
Test link OK!
Usertests: Running ch6_file3
Test mass open/unlink OK!
```

这些现象对应了本章最核心的三条学习主线：

- `MMIO range` 说明 VirtIO block 所需地址已映射。
- `Test fstat OK!` 说明 inode 元数据能够回填到 `Stat`。
- `Test link OK!` 与 `Test mass open/unlink OK!` 说明硬链接和目录项回收路径工作正常。

## 10. 局限性与注意事项

- 这是面向教学的内核实验 crate，不是通用 OS 发行版。
- 运行依赖 QEMU 与 RISC-V 交叉编译环境，不能把它当成宿主机上的普通 CLI 程序。
- `build.rs` 会构建用户程序并生成 `fs.img`，首次构建会比普通 crate 更慢。
- 如果环境里没有 `cargo-clone`，则需要先安装该工具，或者手动设置 `TG_USER_DIR` 指向本地 `tg-rcore-tutorial-user`。

## 11. 对学习者的建议

建议按下面顺序阅读：

1. [`src/main.rs`](./src/main.rs)：先看内核初始化、主调度循环和 syscall 分发。
2. [`src/virtio_block.rs`](./src/virtio_block.rs)：理解块设备是如何接入内核地址空间的。
3. [`src/fs.rs`](./src/fs.rs)：看文件系统封装层怎样把 easy-fs 暴露给 syscall。
4. [`src/process.rs`](./src/process.rs)：看 `fd_table` 如何进入进程结构并在 `fork` 后继承。
5. [`report.md`](./report.md)：对照真实 Debug 记录理解为什么最终实现要修改 easy-fs inode 元数据。

如果你的目标是“通过实验理解 OS kernel 文件系统”，这个 crate 的价值不只是能跑通，而是能帮助你把“磁盘块、inode、目录项、fd、syscall、QEMU 启动链路”串成一条完整路径。
