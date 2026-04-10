# ch1~ch2 多核扩展实验报告

本次改造没有直接污染原始章节目录，而是把实现放在以下特殊目录中：

- `tg-rcore-tutorial-sbi-smp`
- `tg-rcore-tutorial-ch1-smp`
- `tg-rcore-tutorial-ch2-smp`

测试环境统一使用 QEMU virt + `-bios none` + `-smp 4`，并且所有编译/运行命令都通过 Docker 容器执行。

## 一、 实验目标与架构设计

- `ch1-smp` 的目标是把原始“单核 Hello world”扩展为“4 个 hart 都能启动、都有独立栈、都能完整输出一行字符串”。
- `ch2-smp` 的目标是在“4 个 hart 都能启动”的基础上，再做主从核分离：主核负责清空 `.bss`、初始化控制台/系统调用、批处理执行用户程序；从核只证明自己成功上线，然后进入 `wfi` 待机。
- 参考 ArceOS 的思路，启动阶段显式把“当前 CPU/Hart ID”作为入口参数传给高层入口，并将主核入口与从核入口逻辑拆开。ArceOS 文档中的 `rust_main(cpu_id, arg)` / `rust_main_secondary` 正是这种设计风格。
- 底层启动分成两级：
- 第一级是 `tg-rcore-tutorial-sbi-smp/src/m_entry.asm` 的 M 态入口。这里用 `mhartid` 给每个 hart 分配独立 M 态栈，并把 hart id 通过 `a0` 传给 S 态 `_start`。
- 第二级是 `tg-rcore-tutorial-ch1-smp/src/main.rs` 和 `tg-rcore-tutorial-ch2-smp/src/main.rs` 的 S 态入口。这里再次按 hart id 计算独立 S 态启动栈，彻底避免“所有核压在同一条栈上”。
- `ch2-smp` 里最关键的同步点不是“谁先打印”，而是“谁有资格先碰全局变量”。主核必须先完成 `zero_bss()`，从核在此之前不能访问位于 `.bss` 的锁、计数器和控制台状态。
- 为了规避这个问题，`ch2-smp` 放了一个带非零初值的 `PRIMARY_STATE`，让它落在 `.data`，从核只轮询这个状态位；等主核完成 `.bss` 清零和全局初始化之后，再允许从核继续执行。

## 二、 核心代码实现与观测指标

- 多核启动的核心实现位于 `tg-rcore-tutorial-sbi-smp/src/m_entry.asm`：
- 读取 `mhartid`。
- 以 `stack_base + hart_id * STACK_SIZE` 的方式给每个 hart 选独立 M 态栈。
- 将当前 hart 的 M 态栈顶写入各自的 `mscratch`。
- 通过 `mret` 跳到 S 态 `_start`，并把 hart id 保存在 `a0/tp`。
- `ch1-smp` 的 S 态入口再次按 hart id 计算独立启动栈。默认路径使用一个简单自旋锁把整行 `Hello from hart X` 包起来，从而保证“行内不交错”；`no-lock-demo` feature 会故意关闭锁，并在逐字符输出之间插入短延迟，稳定制造乱码。
- `ch2-smp` 的主从核分离逻辑如下：
- 主核：`zero_bss()` -> `init_console()` -> `init_io()/init_process()` -> 放行从核 -> 等待从核全部进入待机 -> 批处理执行用户程序。
- 从核：等待 `PRIMARY_STATE == PRIMARY_READY` -> 打印“已上线但进入待机” -> `wfi`。
- `ch2-smp` 的控制台实现覆盖了 `put_str()`，并使用自旋锁保护 SBI 串口输出。虽然最终只有主核长期打印，但这样可以保证从核上线提示与主核日志不会交叉。

多核执行特点与优势测试如下。

### 1. 原始 ch1 在 4 核下的负例

命令：

```bash
docker exec -it rcore-container bash -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch1 && cargo build && qemu-system-riscv64 -machine virt -nographic -bios none -smp 4 -kernel target/riscv64gc-unknown-none-elf/debug/tg-rcore-tutorial-ch1'
```

实测输出：

```text
HHHllHlo, world!
```

观测结论：

- 4 个 hart 同时输出，但没有任何同步，字符级交错立刻出现。
- 这个负例很适合作为“为什么多核下不能只看功能正确、不看输出时序”的第一组演示。

### 2. `ch1-smp` 的无锁演示

命令：

```bash
docker exec -it rcore-container bash -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch1-smp && cargo run --features no-lock-demo'
```

实测输出：

```text
HHHHeeleelloll lolfo ro frmoml fh ahrraotorm ht  fa rr23

to 1m
 hart 0
```

观测结论：

- 每个 hart 都已经拥有独立栈，所以系统没有直接死机。
- 但因为没有锁，逐字符输出还是会被并行 hart 打散。
- 这说明“独立栈”解决的是数据踩踏，“自旋锁”解决的是共享设备访问串行化，两者缺一不可。

### 3. `ch1-smp` 的加锁正确实现

命令：

```bash
docker exec -it rcore-container bash -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch1-smp && cargo run'
```

实测输出：

```text
Hello from hart 3
Hello from hart 0
Hello from hart 1
Hello from hart 2
```

观测结论：

- 行顺序不固定，这是正常的并发行为。
- 但每一行都完整无损，这说明“每 hart 独立栈 + 整行输出自旋锁”已经达到实验目标。

### 4. 原始 ch2 在 4 核下的负例

命令：

```bash
docker exec -it rcore-container bash -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch2 && TG_USER_DIR=/tmp/tg-rcore-tutorial/tg-rcore-tutorial-user cargo build && qemu-system-riscv64 -machine virt -nographic -bios none -smp 4 -kernel target/riscv64gc-unknown-none-elf/debug/tg-rcore-tutorial-ch2'
```

实测现象：

- 终端长时间无输出，系统卡住，需要人工中断。

观测结论：

- 这比 ch1 的“输出乱码”更严重，说明问题已经不是简单的串口竞争，而是启动栈、`.bss` 清零和全局初始化流程在多核下整体失配。

### 5. `ch2-smp` 的主从核分离结果

命令：

```bash
docker exec -it rcore-container bash -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch2-smp && TG_USER_DIR=/tmp/tg-rcore-tutorial/tg-rcore-tutorial-user cargo run'
```

实测关键输出：

```text
hart 3 is online but parked before batch execution
hart 1 is online but parked before batch execution
hart 2 is online but parked before batch execution
[ INFO] all secondary harts are parked; boot hart continues with apps
[ INFO] load app0 to 0x80400000
Hello, world from user mode program!
...
Test power_7 OK!
```

观测结论：

- 多核启动已经生效，因为 3 个从核都能独立上线并打印消息。
- 应用程序执行阶段只有主核继续推进，符合“主核执行批处理、从核休眠等待”的设计目标。
- 这组输出是教学上最好的“从 SMP 启动过渡到单核调度内核”的桥梁案例：硬件是多核的，但内核执行策略可以主动约束为“单执行核”。

## 三、 易错点剖析与 Debug 记录（教学重点）

### Bug 1：多个 hart 共用同一条启动栈

- **Bug 描述 / 非预期现象**：原始 `ch1` 在 `-smp 4` 下输出 `HHHllHlo, world!`；原始 `ch2` 在 `-smp 4` 下几乎无输出直接卡死。
- **底层原因剖析**：原始代码默认只有一个 hart 启动。M 态入口的 `sp` 固定指向同一个 `m_stack_top`，S 态 `_start` 也把所有 hart 的 `sp` 指向同一个 `STACK`。这样多个 hart 会同时覆盖同一片栈内存，轻则局部变量和返回地址互相踩踏，重则在初始化阶段直接跳飞。
- **修复与调试策略**：在 `tg-rcore-tutorial-sbi-smp/src/m_entry.asm` 和 `tg-rcore-tutorial-ch{1,2}-smp/src/main.rs` 中都改成“按 hart id 选独立栈”。调试这类问题时，最有效的方法不是先怀疑 SBI 或 Trap，而是先检查最早几条启动汇编里 `sp` 到底是不是 per-hart 的。

### Bug 2：只有独立栈，没有串口锁，输出仍然会乱码

- **Bug 描述 / 非预期现象**：`ch1-smp` 在 `no-lock-demo` 下虽然不再死机，但输出被打散成 `HHHHeeleell...` 这种字符碎片。
- **底层原因剖析**：独立栈只能保证 hart 之间的调用现场互不覆盖，但 UART 仍然是共享设备。每个 hart 都在逐字符调用 `console_putchar()`，字符级别完全可能交错。
- **修复与调试策略**：在 `ch1-smp` 里引入一个最小自旋锁，并把整行 `writeln!()` 放进临界区，让“共享串口访问”按整行串行化。教学时建议保留 `no-lock-demo`，因为它能非常直观地告诉学生“并发安全不是只有内存安全，还有 I/O 时序安全”。

### Bug 3：主核清 `.bss` 时，从核过早访问全局变量

- **Bug 描述 / 非预期现象**：这是 `ch2` 多核化里必然会遇到的经典坑。若从核太早碰锁、计数器或控制台单例，主核随后执行 `zero_bss()` 就会把这些状态重新清零，表现为随机卡死、计数回退、锁状态异常甚至重复初始化。
- **底层原因剖析**：`.bss` 的语义是“启动时由软件主动清零”。单核时代这一步天然安全；多核时代如果从核已经在读写 `.bss`，主核的清零动作就等价于并发破坏共享状态。
- **修复与调试策略**：`ch2-smp` 中把 `PRIMARY_STATE` 设计成带非零初值的原子变量，使其落在 `.data` 而不是 `.bss`。从核在主核放行之前只轮询这一个状态位，不触碰其他共享对象。凡是多核 bare-metal 启动，只要代码里出现 `zero_bss()`，就应该立刻追问：“其他核在这之前访问了哪些静态变量？”

### Bug 4：所有 hart 都进入批处理主循环，用户程序会被重复执行

- **Bug 描述 / 非预期现象**：这是 `ch2` 多核扩展里另一个必然会遇到的问题。若不做主从核分离，多个 hart 会同时跑 `for app in AppMeta::locate().iter()`，最终出现同一个用户程序被重复加载、重复打印，甚至在共享内核状态上形成竞态。
- **底层原因剖析**：`ch2` 的批处理框架本质上是单执行流内核，没有准备好让多个 hart 共享调度状态、应用装载区和 Trap 处理路径。硬件多核启动不等于软件已经具备多核调度能力。
- **修复与调试策略**：在 `rust_main(hart_id)` 入口处立即分流：主核进入 `primary_rust_main()`，从核进入 `secondary_rust_main()`。这也是 ArceOS 风格的主从核入口拆分方式。教学时要强调一个概念：SMP 启动成功，只说明“多个 CPU 活了”；并不代表“多个 CPU 都应该执行同一套内核主循环”。

### Bug 5：`deny(warnings)` 与 feature 分支叠加后，演示代码无法编译

- **Bug 描述 / 非预期现象**：第一次编译 `docker exec -it rcore-container bash -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch1-smp && cargo run --features no-lock-demo'` 时，编译器报错：

```text
error: struct `SpinLock` is never constructed
error: associated items `new` and `lock` are never used
error: unused imports: `AtomicBool`, `DerefMut`, `Deref`, and `cell::UnsafeCell`
```

- **底层原因剖析**：`no-lock-demo` 故意不走加锁路径，但 crate 顶部开启了 `deny(warnings)`。这样一来，锁实现和相关 import 在该 feature 下全部变成死代码，编译器会直接拒绝构建。
- **修复与调试策略**：把 `SpinLock`、`SpinLockGuard` 以及对应 import 都加上 `#[cfg(not(feature = "no-lock-demo"))]`。这条记录很适合提醒学生：教学演示代码里的 feature 开关也要维护好，否则实验现象还没展示出来，工程层面就先失败了。

### Bug 6：后续章节若在中断上下文复用普通自旋锁，可能自锁死

- **Bug 描述 / 非预期现象**：在 ch1/ch2 当前实现里没有打开时钟中断，所以普通自旋锁可以工作。但如果把同样的锁直接搬到 ch3+ 的时钟中断、Trap 或可抢占路径中，可能出现“持锁时被中断打断，中断里再次尝试拿同一把锁”的自锁死。
- **底层原因剖析**：这是多核 + 中断环境下最常见的锁语义升级问题。单纯的原子交换只能保证核间互斥，不能保证“当前 hart 不会被自己的中断重入”。
- **修复与调试策略**：在 ch1/ch2 报告里提前提醒学生：后续若要把这套锁扩展到中断驱动场景，需要引入“关本地中断的锁”或者更细粒度的中断上下文设计，而不是机械复用当前实现。
