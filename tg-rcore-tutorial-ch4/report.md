# 一、 实验要求分析

本次实验的目标主要有三项：

- 在引入 Sv39 地址空间后，重写 `trace` 系统调用，使其能够在用户虚拟地址上正确完成读、写和 syscall 次数统计。
- 实现匿名页映射 `mmap/munmap`，支持按页对齐的用户态内存申请与解除映射，并正确处理非法参数与冲突区间。
- 保证现有的 `sbrk`、时钟、调度等路径在接入虚拟内存后仍能与练习测例兼容。

本次最终完成的是 `tg-rcore-tutorial-ch4` 目录下的内核实现，实际修改集中在：

- `src/main.rs`
- `src/process.rs`

虽然题面提示“可能需要修改 `tg-rcore-tutorial-kernel-vm`”，但本次最终没有改动该 crate，而是通过调整 `Process::new` 的装载逻辑，在 `ch4` 自身完成了兼容修复。

# 二、 代码实现逻辑

## 1. `src/main.rs`

### （1）补全 `trace`

`trace` 最终实现了 3 类请求：

- `trace_request = 0`：读取用户地址上的一个字节
- `trace_request = 1`：向用户地址写入一个字节
- `trace_request = 2`：查询当前进程某个 syscall 的调用次数

实现关键点：

- 使用 `address_space.translate()` 做虚拟地址翻译，不再直接把用户指针当内核地址使用。
- 读取时要求页表项至少具有 `U + R + V`。
- 写入时要求页表项至少具有 `U + W + V`。
- syscall 次数统计在 `schedule()` 分发系统调用前记录，因此 `trace(2, SYS_TRACE, 0)` 会把“本次 trace 调用本身”计入结果，这与题目测例一致。

核心代码片段：

```rust
impl Trace for SyscallContext {
    fn trace(&self, caller: Caller, trace_request: usize, id: usize, data: usize) -> isize {
        const READABLE: VmFlags<Sv39> = build_flags("U__RV");
        const WRITABLE: VmFlags<Sv39> = build_flags("U_W_V");
        let process = unsafe { PROCESSES.get_mut() }.get_mut(caller.entity).unwrap();
        match trace_request {
            0 => process
                .address_space
                .translate::<u8>(VAddr::new(id), READABLE)
                .map_or(-1, |ptr| unsafe { *ptr.as_ptr() as isize }),
            1 => process
                .address_space
                .translate::<u8>(VAddr::new(id), WRITABLE)
                .map_or(-1, |mut ptr| {
                    unsafe { *ptr.as_mut() = data as u8 };
                    0
                }),
            2 => process.syscall_times(id) as isize,
            _ => -1,
        }
    }
}
```

### （2）实现 `mmap/munmap`

`mmap` 和 `munmap` 的实现都放在 `impl Memory for SyscallContext` 中。

`mmap` 的处理流程：

- 检查 `addr` 是否页对齐
- 检查 `prot` 是否只使用低 3 位，且不能为 0
- 检查 `addr + len` 是否溢出
- 将区间换算为页级 `[floor(addr), ceil(addr+len))`
- 遍历当前地址空间 `areas`，拒绝与已有映射重叠的区间
- 通过 `address_space.map()` 建立匿名页映射

`munmap` 的处理流程：

- 检查 `addr` 是否页对齐
- 检查 `addr + len` 是否溢出
- 将区间换算为页级范围
- 逐页确认该范围完全被当前地址空间覆盖
- 调用 `address_space.unmap()` 解除映射

核心代码片段：

```rust
fn mmap(&self, caller: Caller, addr: usize, len: usize, prot: i32, _flags: i32, _fd: i32, _offset: usize) -> isize {
    const PAGE_SIZE: usize = 1 << Sv39::PAGE_BITS;
    if addr & (PAGE_SIZE - 1) != 0 || prot < 0 {
        return -1;
    }
    let Some(flags) = user_map_flags(prot as usize) else {
        return -1;
    };
    if len == 0 {
        return 0;
    }
    let Some(end) = addr.checked_add(len) else {
        return -1;
    };
    let range = VAddr::<Sv39>::new(addr).floor()..VAddr::<Sv39>::new(end).ceil();
    let process = unsafe { PROCESSES.get_mut() }.get_mut(caller.entity).unwrap();
    if process.address_space.areas.iter().any(|area| ranges_overlap(area, &range)) {
        return -1;
    }
    process.address_space.map(range, &[], 0, flags);
    0
}
```

### （3）补上 syscall 计数链路

为了让 `trace(..., request = 2, ...)` 工作，需要在系统调用分发入口记录每次调用。

最终做法是在 `schedule()` 的 `UserEnvCall` 分支中：

- 先读取 `a7` 中的 syscall id
- 在当前进程上记录一次调用
- 再把 syscall 分发给 `tg_syscall::handle`

核心代码片段：

```rust
let process = unsafe { PROCESSES.get_mut() }.get_mut(0).unwrap();
let (raw_id, args) = {
    let ctx = &process.context.context;
    (ctx.a(7), [ctx.a(0), ctx.a(1), ctx.a(2), ctx.a(3), ctx.a(4), ctx.a(5)])
};
process.record_syscall(raw_id);
let id: Id = raw_id.into();
```

## 2. `src/process.rs`

### （1）增加每进程 syscall 次数统计

为了避免把一个较大的固定数组直接内嵌进 `Process`，最终采用了堆上的 `Vec<usize>`：

```rust
const MAX_SYSCALL_NUM: usize = 512;

pub struct Process {
    pub context: ForeignContext,
    pub address_space: AddressSpace<Sv39, Sv39Manager>,
    pub heap_bottom: usize,
    pub program_brk: usize,
    syscall_times: Vec<usize>,
}
```

并补充了：

- `record_syscall()`
- `syscall_times()`

### （2）修复 ELF 装载时的重复映射问题

调试时发现某些用户 ELF 的多个 `LOAD` 段会共享同一个 4 KiB 页面。如果仍按“每个段直接 `address_space.map(range, data, ...)`”的方式装载，就会在第二次映射同一页时触发断言。

因此最终把装载逻辑改为：

1. 遍历所有 `LOAD` 段，先按页收集每个 VPN 需要的联合权限
2. 逐页建立映射
3. 再按段把文件数据拷贝回已映射页面

这样即使段之间共享页面，也只会映射一次。

核心代码片段：

```rust
let mut page_flags = BTreeMap::<VPN<Sv39>, VmFlags<Sv39>>::new();
let mut load_segments = Vec::new();

for program in elf.program_iter() {
    // ...
    let flags = parse_flags(...).unwrap();
    let vpn_range = VAddr::new(off_mem).floor()..VAddr::new(end_mem).ceil();
    let mut vpn = vpn_range.start;
    while vpn < vpn_range.end {
        page_flags
            .entry(vpn)
            .and_modify(|page_flag| *page_flag |= flags)
            .or_insert(flags);
        vpn = vpn + 1;
    }
    load_segments.push((off_mem, &elf.input[off_file..][..len_file]));
}

for (vpn, flags) in page_flags {
    address_space.map(vpn..vpn + 1, &[], 0, flags);
}
```

# 三、 遇到的问题与 Debug 记录（核心重点）

## 1. `build.rs` 在容器内直接失败

- **Bug 描述**：
  容器内执行 `cargo run --features exercise` 时，`build.rs` 直接 panic：

  `failed to clone tg-rcore-tutorial-user@0.4.8 ... ensure cargo-clone is installed or set TG_USER_DIR`

- **原因排查**：
  该仓库的构建脚本默认会调用 `cargo clone` 获取 `tg-rcore-tutorial-user`。但当前容器里没有安装 `cargo-clone`，所以在真正进入内核编译前就失败了。

- **解决过程**：
  我没有去改业务代码，也没有在容器里额外安装工具，而是直接使用本地已经存在的用户仓库：

  `TG_USER_DIR=../tg-rcore-tutorial-user cargo run --features exercise`

  后续所有容器内的编译和测试命令都统一带上这个环境变量，避免重复踩坑。

## 2. `ch3_trace` 断言失败，`count_syscall()` 返回错误

- **Bug 描述**：
  第一次跑通用户测例后，`ch3_trace` 失败，串口输出中出现：

  `Panicked at src/bin/ch3_trace.rs:29, assertion failed: 3 <= count_syscall(SYS_CLOCK_GETTIME)`

- **原因排查**：
  我最初只按题面直觉实现了 `trace_request = 0/1` 的读写逻辑，但忽略了旧实验中 `trace_request = 2` 还承担“查询 syscall 次数”的职责。同时，当前代码里也没有现成的 per-process syscall 计数存储。

- **解决过程**：
  我补了两条链路：

  1. 在 `Process` 中增加 syscall 计数存储
  2. 在 `schedule()` 的系统调用分发入口先记录一次，再进入 `tg_syscall::handle`

  最后在 `trace()` 中补充 `request = 2` 分支，返回当前进程对应 syscall id 的累计次数。

## 3. 修改后 QEMU 启动卡死，只打印到 `detect app[0]`

- **Bug 描述**：
  加入 syscall 计数后，QEMU 不再正常启动，只能看到：

  - 内核段映射日志
  - `detect app[0]: ...`

  之后没有 panic，也没有继续打印 `process entry = ...`，表现为静默卡死。

- **原因排查**：
  这一版里我把 syscall 次数统计做成了 `Process` 结构体中的固定数组 `[usize; 512]`。这会显著放大 `Process` 对象大小。在当前裸机启动阶段，内核仍在较小的 boot stack 上初始化应用，较大的按值返回对象很容易造成隐式栈压力。虽然没有直接 panic，但症状符合“早期栈破坏导致静默异常”。

  这是基于现象和修复结果做出的推断：把固定数组改掉之后，系统立刻恢复正常装载。

- **解决过程**：
  我将 `Process` 中的 syscall 计数从固定数组改成了堆上的 `Vec<usize>`：

  - 原方案：`[usize; 512]`
  - 最终方案：`Vec<usize>`

  这样 `Process` 本体保持轻量，启动阶段不再因为大对象按值传递导致异常。

## 4. 第 3 个应用装载时触发页表重复映射断言

- **Bug 描述**：
  修复卡死后，系统继续启动，但在加载第 3 个应用时 panic：

  `panicked at .../tg-rcore-tutorial-kernel-vm/src/space/mapper.rs:37:9: assertion failed: !pte.is_valid()`

- **原因排查**：
  我进一步检查了用户程序 `ch3_trace` 的 ELF program headers，发现多个 `LOAD` 段共享同一个 4 KiB 页面：

  - `.text` 结束页与 `.rodata` 起始页共享
  - `.rodata` 结束页与 `.data` 起始页共享

  原先的装载逻辑是“每个段直接调用一次 `address_space.map(range, ...)`”，这会导致共享页被第二次映射，从而在 `Mapper::arrive()` 的 `assert!(!pte.is_valid())` 处直接 panic。

- **解决过程**：
  我把 `Process::new` 的 ELF 装载方式改成了“按页建模”：

  1. 遍历所有 `LOAD` 段，先把每个 VPN 的权限做并集
  2. 逐页执行一次映射
  3. 再按段内容回填数据

  这样每个页只会被建立一次页表映射，即使多个段共享同一页，也不会重复写 PTE。

## 5. `./test.sh exercise` 在容器内提示 `Permission denied`

- **Bug 描述**：
  代码通过后，执行完整测试命令时报错：

  `/bin/sh: 1: ./test.sh: Permission denied`

- **原因排查**：
  这是宿主机代码通过 `tar | docker exec` 同步到容器后的文件属性问题，脚本的执行位没有保留下来。它不属于内核实现问题。

- **解决过程**：
  我没有去改测试脚本内容，只是把执行方式改成：

  `bash ./test.sh exercise`

  最终完整练习测试通过。

---

最终在容器内通过的验收命令为：

```bash
docker exec -it rcore-container /bin/sh -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch4 && TG_USER_DIR=../tg-rcore-tutorial-user bash ./test.sh exercise'
```

最终结果：

- `ch4 exercise` 测试通过
- `16/16` 检查项全部通过
