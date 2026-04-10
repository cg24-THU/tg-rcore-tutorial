# cg-tg-rcore-tutorial-t3l1

`cg-tg-rcore-tutorial-t3l1` 是一个面向学习者的操作系统内核实验 crate。它基于 `tg-rcore-tutorial` 的 chapter 1 裸机启动代码，在最小执行环境上扩展了 VirtIO-GPU framebuffer 输出能力，并在 QEMU 图形窗口中静态渲染由七巧板色块组成的 `OS` 图案。

这个 crate 的目标不是做完整内核，而是把一个清晰、可复现、可阅读的内核实验成果整理成可以直接 `cargo run` 的学习样例，帮助助教、老师和同学快速复现实验效果，并理解从裸机启动到 framebuffer 渲染的最小闭环。

![Tangram OS](docs/ch1-tangram.png)

## 学习目标

这个实验对应的核心学习点包括：

- 理解 `#![no_std]`、`#![no_main]` 下的最小内核执行环境
- 理解 RISC-V S 态程序的裸机入口、手动设栈与 panic 收口
- 理解 QEMU `virt` 平台上的 VirtIO-MMIO 设备发现方式
- 理解 VirtIO-GPU framebuffer 初始化、像素写入与 flush 的基本流程
- 理解如何用 Rust 常量几何数据描述静态图案，并在内核中直接进行光栅化

## 功能说明

当前 crate 已实现：

- 最小 RISC-V S 态裸机启动流程
- 使用 `tg-rcore-tutorial-sbi` 提供的 M 态启动与 SBI 能力
- 扫描 QEMU `virt` 平台上的 VirtIO-MMIO 槽位并识别 VirtIO-GPU
- 初始化 framebuffer，并在其中绘制静态七巧板风格 `OS` 图案
- 调用 `flush` 将 guest framebuffer 内容提交到 host 图形输出
- 提供 headless 验证脚本，便于在没有图形界面的环境中检查渲染路径是否成功执行

## 项目结构

```text
cg-tg-rcore-tutorial-t3l1/
├── .cargo/config.toml      # 默认 target 与 QEMU runner
├── build.rs                # 生成链接脚本，安排 M/S-mode 内存布局
├── Cargo.toml              # crate 元数据与依赖
├── Makefile                # 构建、运行、headless 验证快捷入口
├── README.md               # 面向学习者的实验说明
├── docs/
│   ├── ch1-tangram.png     # 当前实验渲染截图
│   └── reproducibility.md  # 补充复现与验证说明
├── src/main.rs             # 裸机入口、VirtIO-GPU 初始化、framebuffer 绘制
└── test.sh                 # headless 自动化验证脚本
```

## 环境要求

- Rust toolchain，且支持 `riscv64gc-unknown-none-elf`
- `qemu-system-riscv64`，建议版本 `>= 7.0`
- 如果直接运行 `cargo run`，需要可用的图形环境
  - Linux: X11/Wayland
  - macOS: 本地 GUI 环境
  - 容器/CI/SSH 环境若无图形能力，请使用 `make run-headless` 或 `bash test.sh`

## 构建与运行

### 1. 安装目标平台

```bash
rustup target add riscv64gc-unknown-none-elf
```

### 2. 构建

```bash
cargo build
```

### 3. 运行图形版

```bash
cargo run
```

`cargo run` 会调用 `.cargo/config.toml` 中配置的 QEMU runner，启动带 `virtio-gpu-device` 的 `qemu-system-riscv64`。若图形环境可用，QEMU 窗口中会显示七巧板风格 `OS` 静态图案。

### 4. 使用 Makefile

```bash
make build
make run
make run-headless
make test
```

其中：

- `make run` 等价于 `cargo run`
- `make run-headless` 在无图形输出环境中运行 QEMU，并输出串口日志
- `make test` 调用 `test.sh`，验证 framebuffer 渲染路径是否执行成功

## 复现方法

### 方式一：从 crates.io 获取

如未安装 `cargo-clone`，先执行：

```bash
cargo install cargo-clone
```

然后执行：

```bash
cargo clone cg-tg-rcore-tutorial-t3l1
cd cg-tg-rcore-tutorial-t3l1
cargo run
```

若当前环境没有图形界面：

```bash
make run-headless
```

### 方式二：从 Git 仓库获取

```bash
git clone https://github.com/cg24-THU/tg-rcore-tutorial.git
cd tg-rcore-tutorial/tg-rcore-tutorial-ch1
cargo run
```

若当前环境没有图形界面：

```bash
make run-headless
```

## 输出示例

在 headless 验证模式下，串口应看到类似输出：

```text
Booting ch1-tangram...
VirtIO-GPU framebuffer ready: 1280x800
Tangram OS rendered.
```

如果使用图形版运行，QEMU 窗口会显示一幅深色背景上的彩色七巧板 `OS` 静态图案。

## 版本与 tag

- crate version: `0.0.0`
- git tag: `v0.0.0`

本 README 对应的发布版本和仓库 tag 应保持一致，便于助教和老师通过仓库 tag 或 crates.io 包版本进行交叉验证。

## 注意事项与局限

- 这是一个 chapter 1 实验成果，不包含完整的进程、内存管理或文件系统功能
- 绘制内容是静态图案，不包含交互、动画或游戏逻辑
- `cargo run` 依赖图形环境；在容器、CI 或远程 SSH 中更推荐 `make run-headless` / `bash test.sh`
- 该 crate 重点在于帮助理解“最小内核执行环境 + VirtIO-GPU framebuffer 输出”的组合路径

## 对学习者的价值

如果你刚开始接触 OS kernel 实验，这个 crate 适合作为一个可读性较高、反馈直观的切入点。它把“从裸机启动到图形输出”的链路压缩到较少代码中，既便于自己学习，也便于带着同学或助教一起复现、讲解和检查。
