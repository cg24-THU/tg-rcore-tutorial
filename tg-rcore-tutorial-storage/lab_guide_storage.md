# 1. Overview & Objectives

`tg-rcore-tutorial-storage` 是一个从 Chapter 1 最小内核出发扩展得到的新实验内核。它保留了 Ch1 的裸机入口、最小堆分配器和 VirtIO MMIO 访问方式，但将设备从 GPU 切换为 VirtIO Block，并补上了外部中断处理链路，使磁盘读写不再依赖同步轮询接口。

本实验的目标是完成以下闭环：

- 在 QEMU `virt` 平台挂接 VirtIO 块设备
- 在 S 态内核中初始化 PLIC，并打开 Supervisor External Interrupt
- 使用 `virtio-drivers` 的非阻塞块设备接口提交读写请求
- 在外部中断处理函数中确认中断、回收已完成请求、唤醒等待逻辑
- 完成一次写块与读块校验，证明“磁盘请求提交”和“请求完成通知”是通过中断机制连接起来的

# 2. Architecture & Design

本实验没有直接移植 Chapter 6 的完整文件系统与进程框架，而是只抽取了最小可运行的存储链路。

整体设计如下：

```text
_start
  -> rust_main
    -> init_trap
    -> plic::init
    -> init_block_device
    -> write_block_nb
    -> wfi 等待中断
    -> SupervisorExternal trap
    -> PLIC claim
    -> VirtIO ack_interrupt + pop_used
    -> 请求完成
    -> read_block_nb
    -> wfi 等待中断
    -> 再次处理中断并校验数据
```

与 Ch1 的关系：

- 保留了 Ch1 的 `_start -> rust_main` 裸机启动方式
- 复用了 Ch1 的最小 bump allocator 和 DMA HAL 设计
- 同样直接使用固定 MMIO 基地址 `0x1000_1000`

与 Ch6 的关系：

- 复用了 Ch6 的块设备初始化入口，即 `MmioTransport + VirtIOBlk`
- 参考了 Ch6 的 VirtIO HAL 思路，但不引入文件系统、页表和进程调度
- 将 Ch6 的同步 `read_block/write_block` 改为 `read_block_nb/write_block_nb`
- 新增 Ch6 本地并不存在的 PLIC 初始化与 Supervisor 外部中断处理

适配后的最小模块划分如下：

- `build.rs`：生成链接脚本，并自动准备 `disk.img`
- `.cargo/config.toml`：为 QEMU 配置 VirtIO 块设备启动参数
- `src/main.rs`
  - 启动入口 `_start`
  - S 态 trap 向量与 Rust trap 处理函数
  - PLIC 初始化与 claim/complete
  - VirtIO 块设备初始化
  - 非阻塞读写提交与完成等待
  - 最小控制台输出与最小内存分配器

# 3. Step-by-Step Implementation

## 3.1 新建最小 crate

目录结构：

```text
tg-rcore-tutorial-storage/
├── .cargo/config.toml
├── Cargo.toml
├── build.rs
├── lab_guide_storage.md
├── rust-toolchain.toml
└── src/
    └── main.rs
```

关键依赖只有三类：

- `tg-sbi`：提供 `-bios none` 场景下的最小 M 态支持
- `riscv`：访问 S 态 CSR，如 `stvec/sstatus/sie/scause`
- `virtio-drivers`：提供 VirtIO MMIO 与块设备驱动接口

## 3.2 配置 QEMU 块设备

在 `.cargo/config.toml` 中，将运行器改成带块设备的 QEMU：

```toml
[target.riscv64gc-unknown-none-elf]
runner = [
    "qemu-system-riscv64",
    "-machine", "virt",
    "-nographic",
    "-bios", "none",
    "-monitor", "none",
    "-drive", "file=target/riscv64gc-unknown-none-elf/debug/disk.img,if=none,format=raw,id=x0",
    "-device", "virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0",
    "-kernel",
]
```

这一步的意义是把一个原始磁盘镜像挂到 `virtio-mmio-bus.0`，其 MMIO 基地址就是 QEMU `virt` 平台的 `0x1000_1000`。

## 3.3 生成链接脚本并准备磁盘镜像

`build.rs` 做了两件事：

- 为当前 crate 生成链接脚本，保持与 Ch1 一致的 M/S 态布局
- 自动创建 `target/riscv64gc-unknown-none-elf/debug/disk.img`

核心逻辑如下：

```rust
let disk_dir = manifest_dir.join("target").join("riscv64gc-unknown-none-elf").join("debug");
fs::create_dir_all(&disk_dir).unwrap();
let disk_path = disk_dir.join("disk.img");
if !disk_path.exists() {
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&disk_path)
        .unwrap();
    file.seek(SeekFrom::Start((128 * 512 - 1) as u64)).unwrap();
    std::io::Write::write_all(&mut file, &[0]).unwrap();
}
```

这样 `cargo run` 时不需要额外手工制作磁盘镜像。

## 3.4 初始化 trap 和 Supervisor 外部中断

为了接收磁盘完成中断，需要做三件事：

- 设置 `stvec`
- 初始化 `sscratch` 指向专用 trap 栈
- 打开 `sie.sext` 和 `sstatus.sie`

对应核心代码如下：

```rust
unsafe fn init_trap() {
    unsafe extern "C" {
        fn __trap_vector();
        static __trap_stack_top: u8;
    }

    unsafe {
        sscratch::write((&raw const __trap_stack_top) as usize);
        stvec::write(__trap_vector as *const () as usize, TrapMode::Direct);
        sie::set_sext();
        sstatus::set_sie();
    }
}
```

这里没有沿用 Chapter 6 的进程 trap 框架，而是自己写了一个只服务于“内核态设备中断”的最小 trap 向量。这样可以保持 Chapter 1 风格的极简结构，同时仍然满足磁盘中断驱动要求。

## 3.5 初始化 PLIC

QEMU `virt` 平台中，挂在 `virtio-mmio-bus.0` 的第一个 VirtIO 设备通常使用 IRQ 1。为了让该中断送达 S 态，需要：

- 为 IRQ 1 设置非零优先级
- 在 Supervisor context 中打开该 IRQ
- 将阈值设为 0

核心实现如下：

```rust
pub fn init() {
    unsafe {
        write_volatile((PLIC_PRIORITY + VIRTIO0_IRQ as usize * 4) as *mut u32, 1);
        let enable_ptr =
            (PLIC_ENABLE + SUPERVISOR_CONTEXT * 0x80 + ((VIRTIO0_IRQ as usize / 32) * 4))
                as *mut u32;
        let value = read_volatile(enable_ptr);
        write_volatile(enable_ptr, value | (1 << (VIRTIO0_IRQ % 32)));
        write_volatile((PLIC_CONTEXT + SUPERVISOR_CONTEXT * 0x1000) as *mut u32, 0);
    }
}
```

这部分代码是本实验相对 Ch1 和 Ch6 的关键新增点。

## 3.6 初始化 VirtIO 块设备

驱动初始化延续了 Chapter 6 的主思路，只是删掉了文件系统包装：

```rust
fn init_block_device() {
    let header = NonNull::new(VIRTIO0 as *mut VirtIOHeader).unwrap();
    let transport = unsafe { MmioTransport::new(header) }.expect("failed to create transport");
    let device = VirtIOBlk::<memory::VirtioHal, MmioTransport>::new(transport)
        .expect("failed to create block device");
    unsafe {
        *BLOCK_DEVICE.get() = Some(device);
    }
}
```

对应的 HAL 仍然保持“物理地址与虚拟地址等同”的简化模型：

```rust
impl Hal for VirtioHal {
    fn dma_alloc(pages: usize) -> PhysAddr { ... }
    fn dma_dealloc(_paddr: PhysAddr, _pages: usize) -> i32 { 0 }
    fn phys_to_virt(paddr: PhysAddr) -> VirtAddr { paddr }
    fn virt_to_phys(vaddr: VirtAddr) -> PhysAddr { vaddr }
}
```

这种实现之所以可行，是因为当前最小内核没有开启复杂分页映射，实验里 DMA 区和普通内核内存都直接使用恒等地址。

## 3.7 改用非阻塞块请求

若直接调用 `read_block` 或 `write_block`，`virtio-drivers` 会在内部轮询 used ring，这就不再是“中断驱动”。

因此本实验使用如下接口：

```rust
device.write_block_nb(block_id, &*WRITE_BUF.get(), (*RESPONSE.get()).assume_init_mut())
device.read_block_nb(block_id, &mut *READ_BUF.get(), (*RESPONSE.get()).assume_init_mut())
```

请求提交通知后，主流程不轮询设备队列，而是直接执行：

```rust
while !REQUEST_DONE.load(Ordering::SeqCst) {
    unsafe { asm!("wfi"); }
}
```

这一步非常关键：CPU 进入等待中断状态，真正的完成确认由 trap 路径负责。

## 3.8 在中断中确认请求完成

外部中断到来后，处理顺序为：

- `PLIC claim`
- `device.ack_interrupt()`
- `device.pop_used()`
- 对比 token，标记请求完成
- `PLIC complete`

核心代码如下：

```rust
fn handle_external_interrupt() {
    let irq = plic::claim();
    if irq == VIRTIO0_IRQ {
        with_block_device(|device| {
            if device.ack_interrupt() {
                while let Ok(token) = device.pop_used() {
                    if token == REQUEST_TOKEN.load(Ordering::SeqCst) {
                        REQUEST_DONE.store(true, Ordering::SeqCst);
                    }
                }
            }
        });
    }
    if irq != 0 {
        plic::complete(irq);
    }
}
```

这就是“磁盘操作依赖中断完成”的核心证据：主流程不主动去读 used ring，而是等待中断处理函数回收完成项。

## 3.9 做一次写块 + 读块校验

实验主线如下：

```rust
fill_write_buffer();
submit_write(TEST_BLOCK);
wait_for_completion();
ensure_success("write");

clear_read_buffer();
submit_read(TEST_BLOCK);
wait_for_completion();
ensure_success("read");

verify_buffers();
```

验证方式非常直接：

- 先把固定模式数据写到块 1
- 再从块 1 读回
- 最后比较写缓冲区和读缓冲区

只要两者一致，就说明：

- 块设备初始化成功
- PLIC 配置正确
- Supervisor 外部中断已送达
- 非阻塞请求能在 IRQ 中完成回收
- 读写数据链路完整可用

# 4. Debugging Log & Pitfalls

以下问题均来自本次实际实现与验证过程。

## 4.1 容器中看不到目标目录

现象：

- 直接在容器中访问 `/Users/chaoge/workspace/OS/tg-rcore-tutorial-restored` 失败

原因：

- 容器只挂载了主机目录 `/Users/chaoge/workspace/OS/tg-rcore-tutorial`
- 实际开发目录在 `tg-rcore-tutorial-restored`

解决：

- 先将 `tg-rcore-tutorial-restored` 同步到挂载目录
- 再在容器内使用 `/tmp/tg-rcore-tutorial/...` 构建运行

实际使用的同步命令：

```bash
rsync -a --delete /Users/chaoge/workspace/OS/tg-rcore-tutorial-restored/ /Users/chaoge/workspace/OS/tg-rcore-tutorial/
```

## 4.2 `BlkResp::default()` 不能用于静态初始化

现象：

- 编译时报错：`cannot call non-const associated function in statics`

原因：

- `BlkResp::default()` 不是 `const fn`
- 但静态变量初始化要求编译期常量

解决：

- 将响应对象改成 `MaybeUninit<BlkResp>`
- 每次提交请求前再调用 `write(BlkResp::default())`

## 4.3 trap 汇编无法链接到 Rust 处理函数

现象：

- 链接时报错：`undefined symbol: rust_trap_handler`

原因：

- 汇编里 `call rust_trap_handler`
- Rust 侧函数名在链接阶段被重整

解决：

- 为函数增加 `#[unsafe(no_mangle)]`

## 4.4 QEMU 报 SDL 初始化失败

现象：

- `Could not initialize SDL(x11 not available)`

原因：

- 最初 runner 没有显式关闭图形输出
- 容器环境没有可用的图形界面

解决：

- 在 runner 中加入 `-nographic`
- 不再使用图形相关配置

## 4.5 为什么不能用同步 `read_block/write_block`

现象：

- 同步接口在 `virtio-drivers` 内部会轮询 `queue.can_pop()`

原因：

- 这会把“完成等待”留在驱动内部自旋完成，外部中断即使存在，也不是实验的核心完成路径

解决：

- 改用 `read_block_nb/write_block_nb`
- 在主流程中 `wfi`
- 在 IRQ 中 `ack_interrupt + pop_used`

这一步决定了实验是否真正满足“必须利用中断机制完成磁盘操作”的要求。

# 验证命令

本实验在 Docker 容器内通过以下命令完成最终验证：

```bash
docker exec -it rcore-container sh -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-storage && cargo run'
```

最终输出如下：

```text
booting tg-rcore-tutorial-storage
submit write request
submit read request
interrupt-driven storage check passed
```

该输出表明中断驱动的 VirtIO 块设备读写链路已经跑通。
