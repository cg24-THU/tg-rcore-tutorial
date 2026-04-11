# cg-tg-rcore-tutorial-t2l3

`cg-tg-rcore-tutorial-t2l3` 是一个面向学习者的操作系统内核实验 crate。它基于 `tg-rcore-tutorial` 的 Chapter 1 最小裸机内核，扩展了 VirtIO-GPU framebuffer 输出能力，并在 QEMU 图形窗口中渲染一个七巧板风格的 `OS` 图案。

这个 crate 的目标是把“从 RISC-V 裸机入口到图形输出”的最小闭环整理成一个可以独立构建、独立运行、独立打包发布的学习样例，便于助教、老师和同学快速复现实验、检查结果，并理解操作系统内核实验中最基础的启动与设备访问路径。

![Tangram OS](docs/ch1-tangram.png)

## 项目简介

这个实验聚焦于以下主线：

- 在 `#![no_std]`、`#![no_main]` 条件下建立最小内核执行环境
- 使用 `tg-rcore-tutorial-sbi` 提供的 M 态入口和 SBI 能力
- 在 QEMU `virt` 平台上扫描 VirtIO-MMIO 设备
- 初始化 VirtIO-GPU，并申请 framebuffer
- 在 framebuffer 中直接绘制七巧板风格的 `OS` 图案
- 将图像刷新到 QEMU 图形输出

需要特别说明的是：当前实现是**静态七巧板图案渲染实验**，不是交互式动画系统。文档会严格按照实际代码描述，而不会把静态渲染写成并不存在的动画功能。

## 学习目标

通过这个 crate，可以重点学习以下 OS kernel 知识：

- RISC-V S 态裸机程序如何从 `_start` 进入 Rust 主逻辑
- 内核实验中为什么需要自定义链接脚本和启动栈
- `panic_handler`、`no_std`、`no_main` 在最小内核里的作用
- QEMU `virt` 平台上的 VirtIO-MMIO 设备是如何被发现和初始化的
- framebuffer 图形输出的最小工作路径
- 如何用 Rust 常量数据与简单光栅化逻辑表达图形实验

## 功能说明

当前 crate 已实现：

- 最小 RISC-V S 态裸机启动流程
- VirtIO-GPU 设备扫描与识别
- framebuffer 建立与像素写入
- 七巧板风格 `OS` 图案绘制与 flush
- 无图形环境下的 headless 串口验证
- 便于教学和发布的 README、复现实验文档、Makefile 与测试脚本

## 项目结构

```text
cg-tg-rcore-tutorial-t2l3/
├── .cargo/config.toml      # 默认 target 与 QEMU runner
├── build.rs                # 生成链接脚本，安排 M/S-mode 内存布局
├── Cargo.toml              # crate 元数据与依赖
├── Makefile                # build/check/run/test/package 等快捷入口
├── README.md               # 面向学习者的总说明
├── docs/
│   ├── ch1-tangram.png     # 七巧板图案示意截图
│   └── reproducibility.md  # 补充复现与验证说明
├── src/
│   └── main.rs             # 裸机入口、VirtIO-GPU 初始化、framebuffer 绘制
└── test.sh                 # headless 自动化验证脚本
```

## 环境要求

- Rust toolchain，支持 `riscv64gc-unknown-none-elf`
- `qemu-system-riscv64`
- 如果执行 `cargo run`，需要可用图形环境
- 如果处于容器、CI、SSH 或无图形桌面环境，推荐使用 `make run` 或 `bash test.sh`

安装目标平台：

```bash
rustup target add riscv64gc-unknown-none-elf
```

## 构建与运行

### 1. cargo 方式

图形环境可用时：

```bash
cargo build
cargo run
```

其中 `cargo run` 会启动配置了 `virtio-gpu-device` 的 QEMU，并显示图形窗口。

### 2. Makefile 方式

```bash
make build
make check
make run
make run-gui
make test
```

命令说明：

- `make run`：默认走 headless 复现实验路径，更适合教学机、容器和无图形环境
- `make run-gui`：显式启动图形版 QEMU
- `make test`：运行自动化验证脚本，检查串口日志里是否出现渲染成功标志

## 复现方法

### 方式一：从 crates.io 获取

如未安装 `cargo-clone`，先执行：

```bash
cargo install cargo-clone
```

图形环境可用时：

```bash
cargo clone cg-tg-rcore-tutorial-t2l3
cd cg-tg-rcore-tutorial-t2l3
cargo run
```

无图形环境时：

```bash
cargo clone cg-tg-rcore-tutorial-t2l3
cd cg-tg-rcore-tutorial-t2l3
make run
```

### 方式二：从 Git 仓库获取

```bash
git clone https://github.com/cg24-THU/tg-rcore-tutorial.git
cd tg-rcore-tutorial/cg-tg-rcore-tutorial-t2l3
make run
```

如果有图形环境，也可以执行：

```bash
cargo run
```

## 输出示例

headless 运行时，串口输出应包含：

```text
Booting ch1-tangram...
VirtIO-GPU framebuffer ready: 1280x800
Tangram OS rendered.
```

图形运行时，QEMU 窗口中会看到深色背景和彩色七巧板风格的 `OS` 图案。

## 版本与 tag

- crate name: `cg-tg-rcore-tutorial-t2l3`
- crate version: `0.0.0`
- git tag: `cg-tg-rcore-tutorial-t2l3-v0.0.0`

README、crate 版本与仓库 tag 应保持一致，便于助教和老师通过版本号与源码状态交叉验证。

## 注意事项与局限

- 这是一个学习型的最小内核图形实验，不包含完整进程管理、文件系统或设备中断系统
- 当前实现是静态图案渲染，不包含动画或交互逻辑
- `cargo run` 依赖图形桌面环境；在教学服务器、Docker、CI 中更推荐 `make run`
- 该 crate 的重点是帮助学习“裸机启动 + VirtIO-GPU + framebuffer 绘制”的基础知识

## 对学习者的价值

如果你正在学习操作系统内核实验，这个 crate 适合作为一个较短路径、较强可视反馈的切入点。它把“启动、设备发现、显存写入、图像显示”压缩到一个容易阅读的实验规模中，既适合自学，也适合带着同学、助教或老师一起复现和讲解。
