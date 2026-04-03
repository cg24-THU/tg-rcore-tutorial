## 一、实验要求分析

- 本次实验的核心任务是在 `tg-rcore-tutorial-ch8` 中补全死锁检测功能。
- 具体要求是新增并实现 `enable_deadlock_detect` 系统调用，在当前进程内按需开启或关闭死锁检测。
- 开启后，`mutex_lock` 与 `semaphore_down` 在检测到死锁风险时不能继续阻塞，而应直接返回 `-0xDEAD`。
- 同时需要保证本章原有的线程、互斥锁、信号量、条件变量等基础功能不被破坏，继续通过 `base` 与 `exercise` 两套测试。

## 二、代码实现逻辑

- 修改模块：
  - `src/process.rs`
  - `src/main.rs`

- `src/process.rs` 的修改思路：
  - 给 `Process` 新增 `deadlock: DeadlockState` 字段，把死锁检测需要的资源状态集中放在进程层维护。
  - `DeadlockState` 中分别维护两套状态：
    - semaphore 检测状态：`semaphore_total`、`semaphore_alloc`、`semaphore_need`
    - mutex 检测状态：`mutex_owner`、`mutex_wait`
  - 这样做的原因是：本章里同步原语属于进程共享资源，线程只是执行流；死锁检测也应站在“进程内所有线程共享资源”的视角上做。

- `src/main.rs` 的修改思路：
  - 在 `semaphore_create` / `mutex_create` 时注册新的资源类型。
  - 在 `semaphore_down` 路径上：
    - 先读取当前进程的活跃线程集合；
    - 若已经开启死锁检测，则基于 `Available / Allocation / Need` 做一次安全性检查；
    - 若不安全，直接返回 `-0xDEAD`；
    - 若安全，再进入原本的 `down` 逻辑；成功则更新分配矩阵，阻塞则更新需求矩阵。
  - 在 `semaphore_up` 路径上：
    - 除了唤醒线程，还要把“资源从当前线程转交给被唤醒线程”的状态同步到检测矩阵里。
  - 在 `mutex_lock` 路径上：
    - 使用等待图做环检测；
    - 一旦沿“当前想拿的锁 -> 持锁线程 -> 该线程正在等待的锁 -> 下一持锁线程”形成回路，则返回 `-0xDEAD`。
  - 在 `mutex_unlock` 路径上：
    - 和内核已有语义保持一致，若唤醒了等待线程，则锁所有权直接转交给被唤醒线程。
  - 在 `condvar_wait` 路径上：
    - 由于其内部会发生一次“解锁/再尝试加锁”，因此也同步更新 mutex 的持有关系，避免后续死锁检测读到过期状态。
  - 在 `enable_deadlock_detect` 路径上：
    - 只做参数校验和开关切换；
    - 资源分配状态始终维护，这样即使进程运行中途再开启检测，也能基于当前真实资源状态工作。

- 核心代码片段 1：在进程层维护 semaphore 的检测状态

```rust
#[derive(Default)]
pub struct DeadlockState {
    pub enabled: bool,
    semaphore_total: Vec<usize>,
    semaphore_alloc: BTreeMap<ThreadId, Vec<usize>>,
    semaphore_need: BTreeMap<ThreadId, Vec<usize>>,
    mutex_owner: Vec<Option<ThreadId>>,
    mutex_wait: BTreeMap<ThreadId, usize>,
}
```

- 关键说明：
  - `semaphore_total[j]` 表示第 `j` 类资源总量。
  - `semaphore_alloc[i][j]` 表示线程 `i` 当前已持有的该类资源数。
  - `semaphore_need[i][j]` 表示线程 `i` 当前阻塞等待的该类资源数。

- 核心代码片段 2：`semaphore_down` 中的死锁检测入口

```rust
if current_proc.deadlock.enabled
    && current_proc
        .deadlock
        .semaphore_would_deadlock(active_threads.as_slice(), tid, sem_id)
{
    return DEADLOCK_DETECTED;
}
if !sem.down(tid) {
    current_proc.deadlock.semaphore_wait(tid, sem_id);
    -1
} else {
    current_proc.deadlock.semaphore_acquired(tid, sem_id);
    0
}
```

- 关键说明：
  - 只有在开关开启时才拒绝请求。
  - 若当前请求会把系统带入不安全状态，则不让线程真正进入等待队列。
  - 若只是普通阻塞，则仍走原有的线程阻塞调度路径。

- 核心代码片段 3：`mutex_lock` 的等待环检测

```rust
pub fn mutex_would_deadlock(&self, tid: ThreadId, mutex_id: usize) -> bool {
    let Some(mut holder) = self.mutex_owner.get(mutex_id).copied().flatten() else {
        return false;
    };
    loop {
        if holder == tid {
            return true;
        }
        let Some(wait_mutex_id) = self.mutex_wait.get(&holder).copied() else {
            return false;
        };
        let Some(next_holder) = self.mutex_owner.get(wait_mutex_id).copied().flatten() else {
            return false;
        };
        holder = next_holder;
    }
}
```

- 关键说明：
  - mutex 是单实例资源，因此用等待图判环比矩阵算法更直接。
  - 这也满足实验说明里“mutex 和 semaphore 可分别检测，无需混合考虑”的要求。

- 最终验证命令全部在 Docker 容器中执行：

```bash
docker exec -it rcore-container /bin/sh -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch8 && TG_USER_DIR=/tmp/tg-rcore-tutorial/tg-rcore-tutorial-user cargo build --features exercise'
docker exec -it rcore-container /bin/sh -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch8 && TG_USER_DIR=/tmp/tg-rcore-tutorial/tg-rcore-tutorial-user bash ./test.sh exercise'
docker exec -it rcore-container /bin/sh -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch8 && TG_USER_DIR=/tmp/tg-rcore-tutorial/tg-rcore-tutorial-user bash ./test.sh base'
```

## 三、遇到的问题与 Debug 记录（核心重点）

- **Bug 描述**：第一次在容器里编译时直接报错：
  - `/bin/sh: 1: cd: can't cd to /Users/chaoge/workspace/OS/tg-rcore-tutorial/tg-rcore-tutorial-ch8`
- **原因排查**：
  - 宿主机绝对路径没有直接挂到容器内。
  - 容器里真正可用的教程仓库在 `/tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch8`，而不是宿主机路径。
- **解决过程**：
  - 先用 `docker exec -it rcore-container pwd`、`ls /mnt`、`find / -name tg-rcore-tutorial` 定位容器内仓库位置。
  - 确认容器自带完整 `tg-rcore-tutorial` 仓库后，把测试工作目录切到 `/tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch8`。

- **Bug 描述**：第一次在容器仓库执行 `cargo build --features exercise` 时，构建脚本失败：
  - `error: no such command: clone`
  - `failed to clone tg-rcore-tutorial-user@0.4.8 ... ensure cargo-clone is installed or set TG_USER_DIR`
- **原因排查**：
  - `build.rs` 默认希望通过 `cargo clone` 拉取 `tg-rcore-tutorial-user`。
  - 容器里没有安装 `cargo-clone`，但仓库根目录已经自带了 `tg-rcore-tutorial-user`。
- **解决过程**：
  - 不去改 `build.rs`，直接在测试命令里显式注入：
    - `TG_USER_DIR=/tmp/tg-rcore-tutorial/tg-rcore-tutorial-user`
  - 这样构建脚本就会复用现成用户态仓库，问题消失。

- **Bug 描述**：第二次编译进入本 crate 后，报错：
  - `error: unused #[macro_use] import`
  - 位置在 `src/main.rs:61:1`
- **原因排查**：
  - `main.rs` 中 `extern crate alloc;` 前面仍保留了 `#[macro_use]`。
  - 当前代码并没有在该作用域直接依赖 `alloc` 导出的宏，因此在 `deny(warnings)` 下被提升为编译错误。
- **解决过程**：
  - 删除 `#[macro_use] extern crate alloc;` 中的 `#[macro_use]`。
  - 保留 `extern crate alloc;` 本身，重新编译后该错误消失。

- **Bug 描述**：第一次执行整测时脚本没有真正跑起来：
  - `/bin/sh: 1: ./test.sh: Permission denied`
- **原因排查**：
  - 容器仓库里的 `test.sh` 没有可执行位。
- **解决过程**：
  - 不修改脚本权限，直接改用：
    - `bash ./test.sh exercise`
    - `bash ./test.sh base`
  - 这样既满足容器化测试要求，也避免额外改动仓库文件属性。

- **Bug 描述**：实现 semaphore 死锁检测时，存在一个高风险误判点：`ch8_deadlock_sem2` 里多个子线程会先持有资源，再统一阻塞在 barrier semaphore 上；如果只看“当前阻塞的子线程”，很容易把这个场景误判成死锁。
- **原因排查**：
  - `ch8_deadlock_sem2` 中主线程会提前占有 barrier semaphore 的资源，随后再释放给子线程。
  - 如果安全性检查时没有把主线程也纳入 Allocation 矩阵，那么 `Available` 会一直显示为 0，导致所有子线程都看起来“无法推进”。
- **解决过程**：
  - 在 `semaphore_down` 的检测逻辑里，不只看当前阻塞线程，而是通过 `PThreadManager::get_thread(pid)` 获取当前进程全部活跃线程。
  - 这样主线程持有的 barrier 资源也会进入 `Allocation`，安全性分析才能正确判断“主线程可先运行并释放资源”，从而避免误报。

- **Bug 描述**：`condvar_wait` 虽然不是本次练习重点，但它内部会解锁再尝试加锁；如果不更新 mutex 的死锁跟踪状态，后续 mutex 检测就可能读到过期持有关系。
- **原因排查**：
  - 本章 `condvar_wait` 的实现并不是简单睡眠，而是带有一次 mutex 所有权变更。
  - 死锁检测状态若只在 `mutex_lock/unlock` 外层更新，会和真实内核状态脱节。
- **解决过程**：
  - 在 `condvar_wait` 返回 `(flag, waking_tid)` 后：
    - 先记录一次 mutex 释放 / 转交；
    - 再根据 `flag` 判断当前线程是重新持锁还是进入等待；
  - 这样可以保证 mutex 相关检测状态始终与真实锁状态一致。

- **最终结果**：
  - `bash ./test.sh exercise` 通过，checker 显示 `25/25`
  - `bash ./test.sh base` 通过，checker 显示 `22/22`
