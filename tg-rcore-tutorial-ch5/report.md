# report

## 一、 实验要求分析

- 本次实验需要在 `tg-rcore-tutorial-ch5/src/` 下完成进程与调度相关扩展。
- 具体任务包括：
  - 在新的进程框架下迁移上一章的 `mmap` / `munmap`。
  - 实现 `spawn` 系统调用，使父进程可以按程序名直接创建并启动子进程。
  - 实现 `set_priority` 系统调用。
  - 将原来的 FIFO / RR 调度改为 stride 调度，使 CPU 时间与优先级近似成正比。
  - 保持前向兼容，确保 ch5 `base` 与 `exercise` 测例都能通过。

## 二、 代码实现逻辑

- 本次主要修改了以下三个模块：
  - `src/process.rs`
  - `src/processor.rs`
  - `src/main.rs`

### 1. `process.rs`：补齐进程调度属性与内存映射接口

- 在 `Process` 中新增了两个字段：
  - `priority: usize`：当前进程优先级，默认值为 `16`
  - `stride: u128`：stride 调度中的累计步长，初始值为 `0`
- 为了避免 `set_priority(isize::MAX)` 时出现 `pass = 0` 的问题，使用了 `u128` 类型的 `BIG_STRIDE`，并通过：

```rust
pub fn pass(&self) -> u128 {
    const BIG_STRIDE: u128 = 1u128 << 63;
    BIG_STRIDE / self.priority as u128
}
```

- 每次调度器选中某个进程时，调用：

```rust
pub fn advance_stride(&mut self) {
    self.stride = self.stride.saturating_add(self.pass());
}
```

- 同时在 `Process` 内封装了 `mmap` / `munmap`：
  - `mmap` 负责参数校验、范围冲突检测、按 `prot` 生成页表权限，并调用 `AddressSpace::map` 建立匿名映射。
  - `munmap` 先检查待解除范围是否被当前地址空间完整覆盖，再调用 `AddressSpace::unmap` 删除映射。

核心片段如下：

```rust
pub fn mmap(&mut self, start: usize, len: usize, prot: usize) -> bool {
    if start & (PAGE_SIZE - 1) != 0 {
        return false;
    }
    if prot == 0 || prot & !0x7 != 0 {
        return false;
    }
    let end = if let Some(end) = start.checked_add(len) {
        end
    } else {
        return false;
    };
    let start_vpn = VAddr::<Sv39>::new(start).floor();
    let end_vpn = VAddr::<Sv39>::new(end).ceil();
    if self
        .address_space
        .areas
        .iter()
        .any(|area| area.start < end_vpn && start_vpn < area.end)
    {
        return false;
    }
    self.address_space.map(start_vpn..end_vpn, &[], 0, flags);
    true
}
```

### 2. `processor.rs`：将调度器改为 stride 调度

- 原实现使用 `VecDeque` 做 FIFO 调度，只会“谁先进入就绪队列谁先运行”。
- 修改后保留简单的线性扫描实现，用 `Vec<ProcId>` 存放就绪队列，每次 `fetch()` 时遍历所有 ready 进程，选出 `stride` 最小者。
- 如果多个进程 `stride` 相同，则再按 `ProcId` 做稳定比较。
- 选中之后立即调用 `task.advance_stride()`，保证下一轮调度时其累计步长正确增长。

核心逻辑：

```rust
fn fetch(&mut self) -> Option<ProcId> {
    let mut best: Option<(usize, u128, ProcId)> = None;
    for (idx, &id) in self.ready_queue.iter().enumerate() {
        let Some(task) = self.tasks.get(&id) else {
            continue;
        };
        match best {
            Some((_, best_stride, best_id))
                if task.stride > best_stride
                    || (task.stride == best_stride && id > best_id) => {}
            _ => best = Some((idx, task.stride, id)),
        }
    }
    let (idx, _, id) = best?;
    self.ready_queue.remove(idx);
    self.tasks.get_mut(&id).unwrap().advance_stride();
    Some(id)
}
```

### 3. `main.rs`：接上 syscall 层

- 在 `impl Process for SyscallContext` 中补全了 `spawn`：
  - 从当前进程地址空间中读取用户传入的程序名。
  - 在 `APPS` 中查找目标 ELF。
  - 直接调用 `Process::from_elf` 创建新进程。
  - 通过 `PROCESSOR.add(pid, child_proc, parent_pid)` 建立父子关系并加入调度队列。

```rust
fn spawn(&self, _caller: Caller, path: usize, count: usize) -> isize {
    let (parent_pid, elf) = {
        let current = unsafe { (*processor).current().unwrap() };
        let parent_pid = current.pid;
        let elf = current
            .address_space
            .translate::<u8>(VAddr::new(path), READABLE)
            .map(|ptr| unsafe {
                core::str::from_utf8_unchecked(core::slice::from_raw_parts(
                    ptr.as_ptr(),
                    count,
                ))
            })
            .and_then(|name| APPS.get(name))
            .and_then(|input| ElfFile::new(input).ok());
        (parent_pid, elf)
    };
    elf.and_then(ProcStruct::from_elf).map_or(-1, |child_proc| {
        let pid = child_proc.pid;
        unsafe { (*processor).add(pid, child_proc, parent_pid) };
        pid.get_usize() as isize
    })
}
```

- 在 `impl Scheduling for SyscallContext` 中补全了 `set_priority`：

```rust
fn set_priority(&self, _caller: Caller, prio: isize) -> isize {
    if prio < 2 {
        return -1;
    }
    let current = PROCESSOR.get_mut().current().unwrap();
    current.set_priority(prio as usize);
    prio
}
```

- 在 `impl Memory for SyscallContext` 中补全了 `mmap` / `munmap`，统一转发到 `Process` 对象的方法中。

## 三、 遇到的问题与 Debug 记录（核心重点）

### 1. 容器内测试目录与宿主机源码目录不一致

- **Bug 描述**：最初按宿主机路径在容器中执行 `cd /mnt/tg-rcore-tutorial/tg-rcore-tutorial-ch5`，直接报错：`can't cd to ...`。
- **原因排查**：容器里并没有把当前宿主机仓库直接挂载到这个路径，而是存在一份独立测试副本：`/tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch5`。
- **解决过程**：
  - 先在容器内用 `find` 定位真实测试目录。
  - 确认容器实际运行的是 `/tmp/...` 下的代码。
  - 后续先修改宿主机源码，再把修改后的 `src/` 同步到容器副本中，再运行编译和测试。

### 2. 容器副本没有自动同步宿主机改动

- **Bug 描述**：宿主机代码已经改完，但容器内测试跑的仍然是旧代码。
- **原因排查**：这份 ch5 仓库在容器中不是实时挂载，而是已有一份独立副本，因此单纯修改宿主机文件不会自动影响容器内测试结果。
- **解决过程**：
  - 使用 `docker cp` 将宿主机 `src/` 目录同步到容器中的 `/tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch5/src/`。
  - 同步完成后重新执行编译与测试，确认容器开始使用最新版本源码。

### 3. `cargo build` 失败：`build.rs` 调用了不存在的 `cargo clone`

- **Bug 描述**：第一次容器编译时，构建脚本直接报错：

```text
error: no such command: `clone`
failed to clone tg-rcore-tutorial-user@0.4.8 ...
```

- **原因排查**：`build.rs` 默认尝试通过 `cargo clone` 拉取 `tg-rcore-tutorial-user`，但容器里没有安装这个 cargo 子命令。
- **解决过程**：
  - 检查容器目录，发现用户程序仓库已经存在于 `/tmp/tg-rcore-tutorial/tg-rcore-tutorial-user`。
  - 之后统一在容器编译和测试命令前增加：

```bash
TG_USER_DIR=/tmp/tg-rcore-tutorial/tg-rcore-tutorial-user
```

  - 让 `build.rs` 直接使用现成用户仓库，跳过 `cargo clone` 路径。

### 4. `test.sh` 无法直接运行

- **Bug 描述**：第一次运行测试脚本时先后遇到两个问题：
  - `./test.sh: Permission denied`
  - `set: Illegal option -o pipefail`
- **原因排查**：
  - 容器内 `test.sh` 没有执行位。
  - 直接用 `sh` 执行时，脚本里的 `set -o pipefail` 不被 `sh` 支持，必须使用 `bash`。
- **解决过程**：
  - 将测试命令改为：

```bash
docker exec -it rcore-container sh -lc \
  'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch5 && \
   TG_USER_DIR=/tmp/tg-rcore-tutorial/tg-rcore-tutorial-user \
   bash ./test.sh exercise'
```

  - `base` 测试也使用同样方式执行。

### 5. stride 步长如果用普通 `usize` / 较小常数，`set_priority(isize::MAX)` 会退化

- **Bug 描述**：在实现 `set_priority` 时，测例要求：

```rust
assert_eq!(set_priority(isize::MAX), isize::MAX);
```

如果 `BigStride` 取值过小，则：

```text
pass = BigStride / priority = 0
```

进程 stride 将不再增长，调度结果会失真。
- **原因排查**：stride 算法依赖 `pass > 0`。而优先级上界在测例里被直接拉到了 `isize::MAX`，因此不能继续使用小整数 `BigStride`。
- **解决过程**：
  - 使用 `u128` 保存 `stride`。
  - 将 `BIG_STRIDE` 设为 `1u128 << 63`，保证对本测例中的最大优先级仍有 `pass >= 1`。
  - 使用 `saturating_add` 推进 stride，避免后续长时间运行时潜在溢出反转。

### 6. `mmap/munmap` 迁移到新 AddressSpace API 时，没有现成的“查询映射区间”接口

- **Bug 描述**：这一版框架的 `AddressSpace` 只提供了 `map`、`unmap` 和 `areas` 记录，不像之前章节那样直接暴露完整的 VMA 管理接口。
- **原因排查**：如果不先做范围检查，`mmap` 可能覆盖已有区域，`munmap` 也可能错误地解除包含未映射页的范围，导致行为和测例不一致。
- **解决过程**：
  - `mmap` 中显式扫描 `address_space.areas`，做区间重叠检测。
  - `munmap` 中按 VPN 逐页检查待解除范围是否被现有 `areas` 完整覆盖。
  - 只有校验通过后才调用 `AddressSpace::map/unmap`，从而满足：
    - `ch4_mmap3`
    - `ch4_unmap`
    - `ch4_unmap2`
    的语义要求。

## 四、 最终测试结果

- 练习测例通过：

```bash
docker exec -it rcore-container sh -lc \
  'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch5 && \
   TG_USER_DIR=/tmp/tg-rcore-tutorial/tg-rcore-tutorial-user \
   bash ./test.sh exercise'
```

- 基础测例通过：

```bash
docker exec -it rcore-container sh -lc \
  'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch5 && \
   TG_USER_DIR=/tmp/tg-rcore-tutorial/tg-rcore-tutorial-user \
   bash ./test.sh base'
```

- 最终状态：
  - `ch5 exercise` 通过
  - `ch5 base` 通过
  - `spawn`、`set_priority`、stride 调度、`mmap`、`munmap` 均已正常工作
