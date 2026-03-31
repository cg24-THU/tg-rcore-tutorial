# 一、实验要求分析

本次实验基于 Chapter 3 的多道程序与分时多任务内核，补全一个新的系统调用 `sys_trace`（syscall ID = 410），要求支持以下三类行为：

- `trace_request = 0`：把 `id` 视为 `*const u8`，读取当前任务用户内存中的 1 字节数据并返回。
- `trace_request = 1`：把 `id` 视为 `*mut u8`，将 `data` 的低 8 位写入该地址，返回 `0`。
- `trace_request = 2`：查询当前任务调用编号为 `id` 的系统调用次数，且本次 `trace` 调用本身也必须被计入统计。

结合 Chapter 3 的运行模型，这个实验的核心不只是补一个 syscall 分支，还包括：

- 在任务控制块中维护“按任务隔离”的 syscall 统计信息。
- 在 Trap 进入内核后，从当前任务上下文中提取 syscall 编号和参数。
- 在系统调用返回后，把结果写回用户态寄存器，并继续沿用 Chapter 3 的轮转调度逻辑。

另外，本地仓库和当前工具链环境比教程原始环境更新，实际完成过程中还需要解决构建脚本、Rust 2024 `unsafe` 语义变化，以及启动栈空间不足导致的运行时异常。

# 二、代码实现逻辑

## 1. `src/task.rs`：在任务控制块中维护 syscall 计数

本次实验的关键改动放在 `TaskControlBlock::handle_syscall()`。原因很直接：这里天然能拿到“当前任务”和“本次 syscall 的编号”，最适合做按任务粒度的统计。

最终实现思路：

- 给 `TaskControlBlock` 增加一个稀疏计数表 `syscall_counters`。
- 在 `handle_syscall()` 一进入时先执行 `record_syscall(id)`。
- 用一个 `CURRENT_TASK` 原始指针暂存“当前正在内核中处理 syscall 的任务”，让 `trace()` 能回查到自己所属的 TCB。
- syscall 返回后再清空 `CURRENT_TASK`，避免悬垂使用。

核心代码如下：

```rust
const SYSCALL_RECORD_CAPACITY: usize = 16;

#[derive(Clone, Copy)]
struct SyscallCounterSlot {
    id: SyscallId,
    count: usize,
}

pub struct TaskControlBlock {
    ctx: LocalContext,
    pub finish: bool,
    stack: [usize; 1024],
    syscall_counters: [SyscallCounterSlot; SYSCALL_RECORD_CAPACITY],
}

pub fn handle_syscall(&mut self) -> SchedulingEvent {
    let id = self.ctx.a(7).into();
    let args = [
        self.ctx.a(0),
        self.ctx.a(1),
        self.ctx.a(2),
        self.ctx.a(3),
        self.ctx.a(4),
        self.ctx.a(5),
    ];

    self.record_syscall(id);
    set_current_task(self as *mut _);
    let result = tg_syscall::handle(Caller { entity: 0, flow: 0 }, id, args);
    set_current_task(ptr::null_mut());
    // 后续再按返回结果映射为 None / Yield / Exit / UnsupportedSyscall
}
```

这里我没有直接给每个任务分配一个“超大 syscall 号数组”，而是用了固定容量的稀疏表。原因有两个：

- Chapter 3 的测试只涉及很少的 syscall 编号，没必要做稠密表。
- `rust_main()` 里会一次性在栈上创建整个 `TaskControlBlock` 数组，TCB 再膨胀会直接恶化启动栈压力。

查询逻辑则通过 `syscall_count()` 在线性扫描计数表完成：

```rust
pub(super) fn syscall_count(&self, id: usize) -> usize {
    self.syscall_counters
        .iter()
        .find(|slot| slot.count != 0 && slot.id.0 == id)
        .map_or(0, |slot| slot.count)
}
```

## 2. `src/main.rs`：实现 `Trace for SyscallContext`

`sys_trace` 的最终实现放在 `impl Trace for SyscallContext` 中。这个接口已经由框架预留好，只需要把占位逻辑补全即可。

核心代码如下：

```rust
impl Trace for SyscallContext {
    fn trace(
        &self,
        _caller: Caller,
        trace_request: usize,
        id: usize,
        data: usize,
    ) -> isize {
        match trace_request {
            0 => unsafe { core::ptr::read(id as *const u8) as isize },
            1 => {
                unsafe { core::ptr::write(id as *mut u8, data as u8) };
                0
            }
            2 => crate::task::current_task()
                .map(|task| task.syscall_count(id) as isize)
                .unwrap_or(-1),
            _ => -1,
        }
    }
}
```

几个关键点：

- Chapter 3 还没有地址空间隔离，所以这里直接对用户地址做裸读写即可。
- `trace_request = 2` 依赖 `current_task()` 反查到当前 TCB。
- “本次调用也计入统计”的要求，是通过先在 `handle_syscall()` 中 `record_syscall(id)`，再进入 `trace()` 查询来满足的。

## 3. `src/main.rs`：修复启动栈过小问题

在当前仓库和工具链下，单纯实现 `sys_trace` 后，内核一启动就会在很早期 panic。根因不是 `trace` 本身，而是 `_start` 给 `rust_main` 准备的启动栈太小。

最终修复：

```rust
unsafe extern "C" fn _start() -> ! {
    const STACK_SIZE: usize = (APP_CAPACITY + 4) * 8192;
    #[unsafe(link_section = ".boot.stack")]
    static mut STACK: [u8; STACK_SIZE] = [0u8; STACK_SIZE];
    // ...
}
```

相较原始版本，把栈从 `(APP_CAPACITY + 2) * 8192` 扩大到了 `(APP_CAPACITY + 4) * 8192`，多预留 16 KiB 头部空间，避免 `rust_main` 的大栈帧把 `.bss` 段静态对象踩坏。

## 4. 环境兼容性修复

虽然实验主体只要求修改 `src/`，但为了让当前仓库在 Docker 中可复现地构建和测试，我还补了两处兼容性修复：

- `build.rs`
  - 优先复用同级目录下已有的 `tg-rcore-tutorial-user`，避免构建时强依赖 `cargo clone`。
- `tg-rcore-tutorial-syscall/src/user.rs`
  - 针对 Rust 2024 下的 `unsafe_op_in_unsafe_fn`，给 RISC-V `asm!("ecall", ...)` 再包一层显式 `unsafe` 块，避免依赖 crate 在当前工具链下直接编译失败。
- `tg-rcore-tutorial-kernel-context/src/lib.rs`
  - 同样补齐 `LocalContext::execute()` 中内联汇编和裸指针回写的显式 `unsafe` 块，消除测试阶段的 Rust 2024 兼容性警告。

这几处不是 Chapter 3 机制本身的一部分，但它们是这次“在本地真实环境里把实验跑通”所必需的工程修复。

# 三、遇到的问题与 Debug 记录（核心重点）

## 1. `./test.sh exercise` 初次无法在容器内正常构建

- **Bug 描述**：
  - 一开始直接在容器内执行测试时，容器里并没有宿主机上的实验目录。
  - 把 `ch3` 单独拷进去之后，构建又报 path dependency 缺失，找不到 `tg-rcore-tutorial-signal-defs` 等同仓库依赖。
  - 后续即使依赖路径补齐，`build.rs` 仍会在构建阶段尝试拉取 `tg-rcore-tutorial-user`。

- **原因排查**：
  - 这个仓库不是独立单 crate，而是一个带多个本地 path dependency 的教程工作区。
  - 只同步 `tg-rcore-tutorial-ch3` 会破坏依赖图。
  - 同时当前 `build.rs` 默认假定没有本地 `tg-user` 源码时可通过外部方式自动拉取，但容器环境下不应把成功与否建立在这个步骤上。

- **解决过程**：
  1. 不再只复制 `ch3` 子目录，而是把整个 `tg-rcore-tutorial` 仓库同步到容器的 `/tmp/tg-rcore-tutorial`。
  2. 修改 `build.rs`，先检查同级目录是否已有 `tg-rcore-tutorial-user/Cargo.toml`，若存在则直接复用。
  3. 这样 `./test.sh exercise` 可以在容器中直接构建，不再依赖额外拉取步骤。

## 2. 依赖 crate 在 Rust 2024 下因 `unsafe_op_in_unsafe_fn` 报错

- **Bug 描述**：
  - 构建 `tg-rcore-tutorial-syscall` 时，`src/user.rs` 中多个 `asm!("ecall", ...)` 触发 `unsafe_op_in_unsafe_fn`。
  - 在当前工具链配置下，这会导致依赖 crate 编译失败。

- **原因排查**：
  - Rust 2024 中，`unsafe fn` 的函数体默认不再自动视为“内部所有 unsafe 操作都合法”。
  - 也就是说，即使函数签名本身是 `unsafe fn`，`asm!` 这种操作仍然需要放在显式 `unsafe { ... }` 块里。

- **解决过程**：
  1. 先确认出错位置集中在 `syscall0` 到 `syscall6` 这几个封装函数。
  2. 没有采用临时 `RUSTFLAGS` 压制警告，而是直接修改源码，在每个 `asm!` 外层补上显式 `unsafe`。
  3. 修改后依赖 crate 在容器里可稳定通过编译，后续测试也不再受此问题阻塞。

## 3. `cargo run --features exercise` 能启动 QEMU，但测试没有正常输出

- **Bug 描述**：
  - 修完依赖问题后，`cargo run --features exercise` 已经能编译并启动 QEMU。
  - 但测试脚本迟迟拿不到预期串口输出，看起来像是“内核没有跑起来”。
  - 用 QEMU trace 深挖后发现，内核实际已经进入 `rust_main`，但很快走到了 `panic -> shutdown(true)`。

- **原因排查**：
  - 我先用 `qemu-system-riscv64 -d in_asm,guest_errors -D /tmp/ch3_qemu.log` 抓取访存和指令轨迹。
  - 从 trace 可以看到：
    - 内核执行到了 `tg_rcore_tutorial_syscall::kernel::init_io`。
    - panic 点落在 `spin::Once::try_call_once_slow`。
  - `spin::Once` 本身在这里不该出问题，因此更合理的怀疑是：`Once` 所在 `.bss` 的状态位被提前破坏了。
  - 继续看反汇编后发现，`rust_main` 的函数序言会一次性给局部变量分配约 `0x446e0` 字节栈空间，原因是它在栈上直接创建了 `let mut tcbs = [TaskControlBlock::ZERO; APP_CAPACITY];`。
  - 原 `_start` 的启动栈大小只有 `(APP_CAPACITY + 2) * 8192 = 278528` 字节，实际已经不够，导致启动早期发生栈下溢，把相邻 `.bss` 静态数据踩坏了。

- **解决过程**：
  1. 先怀疑是 `trace` 裸指针访问导致异常，但在更早的 `init_io` 阶段就已经 panic，说明方向不对。
  2. 通过 QEMU 指令日志和反汇编，定位到真正问题是启动栈不够大。
  3. 将 `_start` 中的 `STACK_SIZE` 从 `(APP_CAPACITY + 2) * 8192` 调整为 `(APP_CAPACITY + 4) * 8192`。
  4. 重新在容器内执行 `./test.sh exercise`，串口输出恢复正常，`Test trace OK!` 成功出现。

## 4. 如何保证 `trace_request = 2` 会把“本次 trace 调用”也算进去

- **Bug 描述**：
  - 这是实验要求中最容易漏掉的一个点：如果只在 `trace()` 里统计，或者先查询再统计，得到的次数会比期望少 1。

- **原因排查**：
  - `trace_request = 2` 查询的是 syscall 次数，而当前这次 `trace` 调用本身也是一次 syscall。
  - 所以统计时序必须是“先记账，再分发，再查询”。

- **解决过程**：
  1. 把 `record_syscall(id)` 固定放在 `TaskControlBlock::handle_syscall()` 的最前面。
  2. 之后再进入 `tg_syscall::handle(...)`，最终调用到 `trace()`。
  3. 这样 `trace_request = 2` 查询到的就是已经包含本次 `trace` 的结果，和测例要求一致。

# 四、实验总结

Chapter 3 的核心不是“把多个程序一起装进内存”这么简单，而是把 Trap、调度和任务上下文保存恢复真正串起来：

- 协作式调度依赖任务主动执行 `yield`，适合教学上先理解“为什么会切换”。
- 抢占式调度依赖时钟中断，任务即使不主动让出 CPU，也会在时间片耗尽后被切走。
- Trap 上下文切换的关键在于：用户态寄存器在进入内核时被保存到 `LocalContext`，系统调用处理完后再把返回值写回 `a0`，并通过 `sepc += 4` 跳过 `ecall`。
- `TaskControlBlock` 是 Chapter 3 最重要的数据结构，它把“上下文、用户栈、完成状态、统计信息”都绑定到具体任务上，内核主循环只需要围绕 TCB 做轮转即可。

这次实验还额外暴露了一个很典型的底层系统问题：很多“看起来像逻辑错误”的早期 panic，本质上可能是更底层的栈布局或内存破坏。对这类问题，单看 Rust 报错往往不够，必须结合 QEMU 执行轨迹、反汇编和内存布局一起判断。
