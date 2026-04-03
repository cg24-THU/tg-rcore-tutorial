# ch6~ch8 多核扩展实验报告

本报告覆盖以下三个目录：

- `tg-rcore-tutorial-ch6-smp`
- `tg-rcore-tutorial-ch7-smp`
- `tg-rcore-tutorial-ch8-smp`

对应的设计目标不是“让 ch6~ch8 立即变成真正的并行内核调度器”，而是先把这三个章节改造成 **SMP-safe bring-up**：

- 4 个 hart 都能安全启动；
- 每个 hart 都有独立启动栈；
- 只有主核执行一次性内核初始化与后续应用程序/线程调度；
- 其余从核在完成启动确认后停驻（`wfi`），不再破坏单核章节原有的内核假设。

这样做的意义是把“单核实验代码直接开 4 核会挂”演进为“多核环境下也能稳定运行单核语义的后续章节”，为后面真正的 per-hart 调度器改造打基础。

## 一、实验目标与架构设计

### 1. 总体目标

`ch6~ch8` 的内核代码仍然保留了明显的单核设计痕迹：

- 全局只有一个 `PROCESSOR`；
- 文件系统、块设备初始化只有一份全局状态；
- trap 返回路径默认“当前只有一个内核执行流”；
- `zero_bss`、堆初始化、页表切换等启动动作都默认只能做一次。

因此，直接给原始 `ch6/ch7/ch8` 加上 `-smp 4`，并不能自然得到“4 核并行执行内核”的结果。实际现象是：

- 机器几乎没有任何串口输出；
- QEMU 一直卡住；
- 最终只能被 `timeout` 强行杀掉。

这说明多核启动最先要解决的不是调度，而是 **启动路径本身的并发安全**。

### 2. 参考 ArceOS 的设计思路

本次实现参考了 ArceOS 常见的 SMP bring-up 思路：

- 通过 `mhartid` 区分当前 hart；
- 入口阶段为每个 hart 分配独立栈；
- 用“主核一次性初始化 + 从核等待初始化完成”的模式避免重复初始化；
- 从核不直接参与复杂的内核子系统，而是在一个明确的同步点之后停驻或进入 idle/wfi。

对应到本实验中的具体策略是：

1. 汇编入口 `_start` 不再给所有 hart 使用同一块栈，而是根据 hart id 计算各自的栈顶。
2. 主核负责：
   - `zero_bss`
   - 控制台初始化
   - 堆初始化
   - 页表初始化
   - 文件系统/块设备初始化
   - 加载初始应用并进入原章节的调度循环
3. 从核负责：
   - 等待主核发布 “runtime ready”
   - 打印 `hart X is online but parked ...`
   - 计数后进入 `wfi`

### 3. 为什么这里只做“主从核分离”，不做“多核并行调度”

这是教学上必须说清楚的一点。

`ch6~ch8` 当前做到的是：

- **支持多核启动**
- **支持多核观测**
- **支持多核环境下继续稳定执行原单核章节功能**

但没有做到：

- 每个 hart 各自维护 `current task/thread`
- 每个 hart 各自处理中断/时钟
- 多个 hart 同时安全进入 `PROCESSOR` / `ThreadManager`
- 多个 hart 同时安全进入文件系统和同步原语内部状态机

换句话说，这一版的目标是 **SMP-safe boot**，不是 **true SMP scheduler**。

如果在教学中直接把“支持 4 核启动”说成“支持 4 核并行内核执行”，学生后面会很容易在 `PROCESSOR`、trap、阻塞队列、文件系统锁这些地方踩更大的坑。

## 二、核心代码实现与观测指标

### 1. 多核启动：汇编层的每核独立栈

`ch6-smp/ch7-smp/ch8-smp` 的 `_start` 都做了相同的改造：

1. 使用 `a0` 传入的 hart id；
2. 把 hart id 保存到 `tp`；
3. 定义 `STACKS: [u8; STACK_SIZE * MAX_HARTS]`；
4. 根据 `hart_id * STACK_SIZE` 计算当前 hart 的栈顶；
5. 超过 `MAX_HARTS` 的 hart 直接进入 `wfi`，避免越界踩栈。

核心效果是：

- hart0/hart1/hart2/hart3 不再共享一个 `sp`；
- 早期 Rust 栈帧、函数调用、panic 信息不会互相踩踏；
- 为后续的主从核分流提供了最基本的执行安全。

### 2. 同步机制：主核一次性初始化 + 从核停驻

Rust 层引入了三类关键状态：

- `PRIMARY_STATE: AtomicUsize`
  - 主核完成控制台与日志初始化后，把状态从 `PRIMARY_WAITING` 改为 `PRIMARY_READY`
- `SECONDARY_PARKED: AtomicUsize`
  - 每个从核打印完停驻提示后自增
- `CONSOLE_LOCK: SpinLock<()>`
  - 串口输出按整串加锁，避免多核打印字符交错

对应执行路径如下：

#### 主核路径

- `hart_id == 0` 进入 `primary_rust_main()`
- 做完 `zero_bss`、控制台和日志初始化
- 发布 `PRIMARY_READY`
- 等待 `SECONDARY_PARKED == HART_COUNT - 1`
- 输出：

```text
[ INFO] all secondary harts are parked; boot hart continues with ...
```

- 然后继续执行原章节的：
  - 堆初始化
  - 页表初始化
  - 文件系统/块设备初始化
  - 用户程序/线程调度

#### 从核路径

- `hart_id != 0` 进入 `secondary_rust_main()`
- 在 `PRIMARY_READY` 发布之前一直忙等
- 主核 ready 后输出：

```text
hart 1 is online but parked before ...
hart 2 is online but parked before ...
hart 3 is online but parked before ...
```

- 自增 `SECONDARY_PARKED`
- 永久执行 `wfi`

### 3. 控制台同步：避免串口字符交错

如果直接让多个 hart 同时调用 `tg_sbi::console_putchar()`，输出会以“字节”为单位交织。

因此本次改造中：

- 重写了 `Console::put_char`
- 新增了 `Console::put_str`
- panic 输出不再走普通 `println!`，而是改为 `RawConsole + CONSOLE_LOCK`

这样做之后：

- 从核的启动提示是整行输出；
- checker 能稳定匹配章节测试中的关键模式；
- 课堂上也可以保留“去掉锁就乱序”的反例作为观测案例。

### 4. 三个章节的具体落点

#### ch6-smp

主核在文件系统初始化前等待从核停驻，日志为：

```text
hart X is online but parked before filesystem scheduling
[ INFO] all secondary harts are parked; boot hart continues with filesystem init
```

#### ch7-smp

主核在 pipe/signal 相关运行前等待从核停驻，日志为：

```text
hart X is online but parked before pipe and signal scheduling
[ INFO] all secondary harts are parked; boot hart continues with pipe and signal scheduling
```

#### ch8-smp

主核在 thread/sync 运行前等待从核停驻，日志为：

```text
hart X is online but parked before thread scheduling
[ INFO] all secondary harts are parked; boot hart continues with thread scheduling
```

### 5. 多核执行特点与优势测试

下面给出本次实验实际采用的观测指标和 Docker 命令。

#### 测试环境

- 宿主机目录：`/Users/chaoge/workspace/OS/tg-rcore-tutorial`
- 容器：`rcore-container`
- 容器工作目录：`/tmp/tg-rcore-tutorial`
- QEMU 参数核心点：`-smp 4`

#### 观测指标 A：原始单核版本直接开 4 核时会卡死

这是最重要的负例。

`ch6/ch7/ch8` 原始版本在 4 核下的观测命令分别为：

```bash
docker exec -it rcore-container bash -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch6 && timeout 8s qemu-system-riscv64 -machine virt -nographic -bios none -smp 4 -drive file=target/riscv64gc-unknown-none-elf/debug/fs.img,if=none,format=raw,id=x0 -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 -kernel target/riscv64gc-unknown-none-elf/debug/tg-rcore-tutorial-ch6'
```

```bash
docker exec -it rcore-container bash -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch7 && timeout 8s qemu-system-riscv64 -machine virt -nographic -bios none -smp 4 -drive file=target/riscv64gc-unknown-none-elf/debug/fs.img,if=none,format=raw,id=x0 -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 -kernel target/riscv64gc-unknown-none-elf/debug/tg-rcore-tutorial-ch7'
```

```bash
docker exec -it rcore-container bash -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch8 && timeout 8s qemu-system-riscv64 -machine virt -nographic -bios none -smp 4 -drive file=target/riscv64gc-unknown-none-elf/debug/fs.img,if=none,format=raw,id=x0 -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 -kernel target/riscv64gc-unknown-none-elf/debug/tg-rcore-tutorial-ch8'
```

实际现象都是：

```text
qemu-system-riscv64: terminating on signal 15 from pid ... (timeout)
```

也就是 8 秒内几乎没有有效串口输出，系统直接卡死。

#### 观测指标 B：SMP 版本能稳定打印“从核上线并停驻”

以 `ch7-smp` 为例：

```bash
docker exec -it rcore-container bash -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch7-smp && timeout 12s env TG_USER_DIR=/tmp/tg-rcore-tutorial/tg-rcore-tutorial-user cargo run'
```

典型输出：

```text
hart 2 is online but parked before pipe and signal scheduling
hart 3 is online but parked before pipe and signal scheduling
hart 1 is online but parked before pipe and signal scheduling
[ INFO] all secondary harts are parked; boot hart continues with pipe and signal scheduling
```

`ch6-smp` 和 `ch8-smp` 也能得到同样的结构化输出，只是提示语不同。

这个指标说明：

- 4 个 hart 都已经被成功拉起；
- 但只有主核继续运行章节主体逻辑；
- 从核没有破坏原本单核章节代码的内核假设。

#### 观测指标 C：原章节功能测试仍然通过

这是“继续支持多核能力”的验收关键。

##### ch6-smp

```bash
docker exec -it rcore-container bash -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch6-smp && export TG_USER_DIR=/tmp/tg-rcore-tutorial/tg-rcore-tutorial-user && bash test.sh base'
```

```bash
docker exec -it rcore-container bash -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch6-smp && export TG_USER_DIR=/tmp/tg-rcore-tutorial/tg-rcore-tutorial-user && bash test.sh exercise'
```

实际结果：

- `ch6 base` 通过：`15/15`
- `ch6 exercise` 通过：`33/33`

##### ch7-smp

```bash
docker exec -it rcore-container bash -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch7-smp && export TG_USER_DIR=/tmp/tg-rcore-tutorial/tg-rcore-tutorial-user && bash test.sh base'
```

实际结果：

- `ch7 base` 通过：`18/18`

##### ch8-smp

```bash
docker exec -it rcore-container bash -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch8-smp && export TG_USER_DIR=/tmp/tg-rcore-tutorial/tg-rcore-tutorial-user && bash test.sh base'
```

```bash
docker exec -it rcore-container bash -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch8-smp && export TG_USER_DIR=/tmp/tg-rcore-tutorial/tg-rcore-tutorial-user && bash test.sh exercise'
```

实际结果：

- `ch8 base` 通过：`22/22`
- `ch8 exercise` 通过：`25/25`

### 6. 宿主机侧的一键脚本

为了方便学生复现实验，本次还额外提供了三个宿主机脚本：

- `tg-rcore-tutorial-ch6-smp/docker-test.sh`
- `tg-rcore-tutorial-ch7-smp/docker-test.sh`
- `tg-rcore-tutorial-ch8-smp/docker-test.sh`

它们的职责是：

- 先把宿主机上修改后的目录同步到容器；
- 再在容器里执行 `cargo run` 或 `bash test.sh ...`；
- 保证“代码改在宿主机、编译运行在容器”的约束不被破坏。

## 三、易错点剖析与 Debug 记录（教学重点）

下面记录这次多核改造中真实遇到的，或在这条实现路径上必然会遇到的经典问题。

### 1. Bug：原始 `ch6/ch7/ch8` 直接开 `-smp 4` 后完全卡死

#### Bug 描述 / 非预期现象

运行原始章节时，QEMU 在 8 秒内几乎没有任何有效输出，最后只能看到：

```text
qemu-system-riscv64: terminating on signal 15 from pid ... (timeout)
```

#### 底层原因剖析

这不是单点 bug，而是一串并发初始化问题叠加后的结果：

- 所有 hart 共用同一个启动栈，栈帧互相踩踏；
- 多个 hart 同时执行 `zero_bss()`，会把别的 hart 正在使用的全局状态再次清零；
- 多个 hart 同时初始化堆、页表、块设备、文件系统，等价于对全局状态做无序并发写；
- 多个 hart 同时进入 `PROCESSOR`/调度路径，但原章节并没有 per-hart 的 `current task` 设计。

这些问题同时出现时，表现往往不是“可读的 panic”，而是“静默卡死”。

#### 修复与调试策略

修复分两步做：

1. 先在 `_start` 里让每个 hart 使用自己的栈。
2. 再把 Rust 入口切成主核和从核两条路径，只让主核做一次性初始化。

调试建议：

- 遇到“多核一开机就死”的情况，先别急着查调度器；
- 第一优先级永远是：栈是否独立、BSS 是否只清一次、全局初始化是否只做一次。

### 2. Bug：多个 hart 共用启动栈，日志和返回地址互相覆盖

#### Bug 描述 / 非预期现象

在最早期引导阶段，可能会出现：

- 完全没有日志；
- 日志打印到一半突然停住；
- 偶发跳转到错误地址；
- 相同行为每次复现都不稳定。

#### 底层原因剖析

如果所有 hart 都把 `sp` 设置到同一块 `STACK` 顶部，那么：

- hart0 的函数调用会压栈；
- hart1/hart2/hart3 也会在同一片区域压栈；
- 保存寄存器、返回地址、局部变量全部互相覆盖。

这种问题经常表现为“随机挂”，因为破坏的是最底层的调用现场。

#### 修复与调试策略

修复方式是改成：

- `STACKS: [u8; STACK_SIZE * MAX_HARTS]`
- `sp = &STACKS + hart_id * STACK_SIZE + STACK_SIZE`

同时加上 hart 数量边界检查，避免超过 `MAX_HARTS` 后继续越界。

教学建议：

- 学生第一次做多核 bring-up 时，最容易忽略“汇编入口也需要 per-hart 资源隔离”；
- 这类 bug 不能靠高层锁解决，因为锁本身也要先依赖一个没坏掉的栈。

### 3. Bug：多个 hart 同时 `zero_bss()`，把同步状态重新清零

#### Bug 描述 / 非预期现象

常见现象包括：

- 某些轮次能打印 1~2 行日志后又突然卡住；
- `SECONDARY_PARKED` 计数永远到不了期望值；
- 自旋锁像“失效了一样”。

#### 底层原因剖析

`zero_bss()` 的语义是把 BSS 段全部清零。

而很多同步变量恰好就放在 BSS 中，例如：

- `SECONDARY_PARKED = 0`
- `CONSOLE_LOCK.locked = false`

如果多个 hart 都执行 `zero_bss()`，就可能出现：

- 主核刚把计数加 1；
- 另一个 hart 又把整个 BSS 段刷回 0；
- 于是等待条件永远无法满足。

#### 修复与调试策略

最终修复是：

- 只有主核执行 `zero_bss()`
- 从核必须先等主核完成早期初始化，再进入自己的逻辑

本次实现里还把 `PRIMARY_STATE` 设计成非零初值常量，避免它落进 BSS 被“默认零值语义”掩盖问题。

教学建议：

- 让学生先画出“哪些全局变量在 BSS，哪些在 data”，会比只盯着 Rust 语法更容易理解这一类并发 bug。

### 4. Bug：从核过早打印，导致控制台乱码或日志丢失

#### Bug 描述 / 非预期现象

可能出现：

- 从核打印的提示语半行丢失；
- 多个 hart 的字符串交织在一起；
- checker 无法匹配关键输出。

#### 底层原因剖析

SBI 控制台输出本质上是“逐字节写串口”。

如果多个 hart 同时打印：

- `println!` 不是原子的；
- `console_putchar()` 也不是原子的；
- 一个 hart 刚输出到 `hart 1 is on...`
- 另一个 hart 就可能把字符插进去。

另外，如果从核在 `tg_console::init_console()` 之前就抢先输出，也会出现日志丢失或顺序混乱。

#### 修复与调试策略

修复分两层：

1. 主核完成控制台和日志初始化后，才把 `PRIMARY_STATE` 置为 ready。
2. 所有内核串口输出都用 `CONSOLE_LOCK` 保护。

其中 `Console::put_str()` 整串加锁尤其关键。只锁 `put_char()` 还不够，因为那只能保证“单字节原子”，不能保证“整行原子”。

### 5. Bug：让从核也进入 `PROCESSOR`/文件系统，会出现更隐蔽的并发损坏

#### Bug 描述 / 非预期现象

如果不把从核停驻，而是让它们继续执行原始章节主体逻辑，后面通常会遇到：

- 当前任务指针混乱；
- 阻塞队列/就绪队列状态不一致；
- 文件系统行为随机失败；
- 某些用户程序测试偶发失败；
- 更糟时，系统无日志卡死。

#### 底层原因剖析

`ch6~ch8` 的关键内核数据结构仍然默认单执行流：

- 一个全局 `PROCESSOR`
- 一个全局调度器/线程管理器
- 一套没有 per-hart 分离的 trap 返回路径
- 文件系统和同步原语内部结构也没有围绕“多个内核 hart 同时进入”设计

所以“从核继续跑”并不等于“多核更快”，而是“把单核不变量全部打碎”。

#### 修复与调试策略

当前实验中的正确修复不是“在所有地方随手补锁”，而是先明确边界：

- 这一阶段只做 **SMP-safe boot**
- 从核停在 `wfi`
- 保证原章节功能不回归

真正的下一步应该是：

- 引入 per-hart `Processor`
- 引入 per-hart trap/中断上下文
- 重新设计 ready/block 队列与同步原语内部锁

教学建议：

- 这类问题最适合在课堂上强调“并发设计要先定义边界，再定义锁”；
- 不先划清“哪些代码允许多核同时进入”，只会把 bug 从启动阶段推迟到运行阶段。

### 6. Bug：`HART_COUNT` 与 QEMU `-smp` 不一致，导致等待条件永远不满足

#### Bug 描述 / 非预期现象

如果代码里写的是：

- `HART_COUNT = 4`

但运行时实际用了：

- `-smp 2`

那么主核会一直等待 `SECONDARY_PARKED == 3`，从而死等。

#### 底层原因剖析

主核等待条件是显式写死的：

```text
SECONDARY_PARKED == HART_COUNT - 1
```

只要运行参数和编译期常量不一致，就会导致主核永远等不到。

#### 修复与调试策略

本实验里采用的策略是：

- 明确把 runner 固定为 `-smp 4`
- 代码中的 `HART_COUNT` 也固定为 4

如果后续要做得更通用，可以从 SBI 或设备树读取 CPU 数量，但这已经超出当前教学实验的目标。

## 结论

本次 `ch6~ch8` 多核扩展实验的核心收获有三点：

1. 单核章节代码直接开 4 核，最先出问题的是启动路径，而不是调度算法。
2. 让每个 hart 都“能启动”之后，下一步不是盲目追求并行，而是先用主从核分离稳定住原有单核语义。
3. 真正的 SMP 内核演进应该分层推进：
   - 第一步：per-hart 栈和启动同步
   - 第二步：主从核分离，保证单核章节在多核环境下可运行
   - 第三步：重构 per-hart 调度器、trap、锁与资源管理，才谈得上真正的并行内核执行

如果学生能真正理解这三步，就不容易把“能开多核”误判成“已经支持多核并发内核”。
