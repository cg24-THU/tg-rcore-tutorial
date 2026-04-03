//! 进程与线程管理模块
//!
//! ## 与第七章的区别
//!
//! 第七章中 `Process` 既是资源容器又是执行单元。
//! 第八章将两者分离：
//! - **Process**：资源容器，管理地址空间、文件描述符、**同步原语列表**、信号
//! - **Thread**：执行单元，管理 TID 和上下文
//!
//! 同一进程的所有线程共享 `Process` 中的资源。
//!
//! ## 新增字段
//!
//! | 字段 | 说明 |
//! |------|------|
//! | `semaphore_list` | 信号量列表（进程内所有线程共享） |
//! | `mutex_list` | 互斥锁列表 |
//! | `condvar_list` | 条件变量列表 |
//!
//! 教程阅读建议：
//!
//! - 先看 `Process` 与 `Thread` 的字段分工：明确“资源归进程、执行归线程”；
//! - 再看 `fork/exec/from_elf`：理解跨线程模型后，进程复制与替换语义如何变化；
//! - 最后结合 `processor.rs` 看线程生命周期与进程资源回收的关系。

use crate::{
    build_flags, fs::Fd, map_portal, parse_flags, processor::ProcessorInner, Sv39, Sv39Manager,
    PROCESSOR,
};
use alloc::{
    alloc::alloc_zeroed,
    boxed::Box,
    collections::BTreeMap,
    sync::Arc,
    vec,
    vec::Vec,
};
use core::alloc::Layout;
use spin::Mutex;
use tg_kernel_context::{foreign::ForeignContext, LocalContext};
use tg_kernel_vm::{
    page_table::{MmuMeta, VAddr, PPN, VPN},
    AddressSpace,
};
use tg_signal::Signal;
use tg_signal_impl::SignalImpl;
use tg_sync::{Condvar, Mutex as MutexTrait, Semaphore};
use tg_task_manage::{ProcId, ThreadId};
use xmas_elf::{
    header::{self, HeaderPt2, Machine},
    program, ElfFile,
};

/// 线程（执行单元）
///
/// 每个线程有独立的 TID 和上下文（寄存器状态、satp）。
/// 同一进程的多个线程共享地址空间。
pub struct Thread {
    /// 线程 ID（不可变）
    pub tid: ThreadId,
    /// 执行上下文（包含 LocalContext + satp）
    pub context: ForeignContext,
}

impl Thread {
    /// 创建新线程
    pub fn new(satp: usize, context: LocalContext) -> Self {
        Self {
            tid: ThreadId::new(),
            context: ForeignContext { context, satp },
        }
    }
}

/// 进程（资源容器）
///
/// 管理地址空间、文件描述符、同步原语、信号等共享资源。
/// 一个进程可以包含多个线程。
pub struct Process {
    /// 进程 ID
    pub pid: ProcId,
    /// 地址空间（所有线程共享）
    pub address_space: AddressSpace<Sv39, Sv39Manager>,
    /// 文件描述符表（所有线程共享）
    pub fd_table: Vec<Option<Mutex<Fd>>>,
    /// 信号处理器
    pub signal: Box<dyn Signal>,
    /// 信号量列表（**本章新增**，所有线程共享）
    pub semaphore_list: Vec<Option<Arc<Semaphore>>>,
    /// 互斥锁列表（**本章新增**，所有线程共享）
    pub mutex_list: Vec<Option<Arc<dyn MutexTrait>>>,
    /// 条件变量列表（**本章新增**，所有线程共享）
    pub condvar_list: Vec<Option<Arc<Condvar>>>,
    /// 死锁检测所需的资源分配状态
    pub deadlock: DeadlockState,
}

/// 每个进程独立维护的死锁检测状态。
#[derive(Default)]
pub struct DeadlockState {
    /// 是否启用死锁检测
    pub enabled: bool,
    /// 每类 semaphore 的资源总量
    semaphore_total: Vec<usize>,
    /// semaphore 分配矩阵：线程 -> 各类资源持有数
    semaphore_alloc: BTreeMap<ThreadId, Vec<usize>>,
    /// semaphore 需求矩阵：线程 -> 当前阻塞等待的资源数
    semaphore_need: BTreeMap<ThreadId, Vec<usize>>,
    /// mutex 当前持有者
    mutex_owner: Vec<Option<ThreadId>>,
    /// mutex 等待关系：线程 -> 正在等待的 mutex id
    mutex_wait: BTreeMap<ThreadId, usize>,
}

impl DeadlockState {
    fn ensure_sem_row(
        rows: &mut BTreeMap<ThreadId, Vec<usize>>,
        tid: ThreadId,
        width: usize,
    ) -> &mut Vec<usize> {
        let row = rows.entry(tid).or_insert_with(|| vec![0; width]);
        if row.len() < width {
            row.resize(width, 0);
        }
        row
    }

    fn trim_sem_row(rows: &mut BTreeMap<ThreadId, Vec<usize>>, tid: ThreadId) {
        if rows.get(&tid).is_some_and(|row| row.iter().all(|&v| v == 0)) {
            rows.remove(&tid);
        }
    }

    /// 注册新的 semaphore 资源类型，返回其资源 id。
    pub fn register_semaphore(&mut self, total: usize) -> usize {
        self.semaphore_total.push(total);
        let new_width = self.semaphore_total.len();
        self.semaphore_alloc
            .values_mut()
            .for_each(|row| row.resize(new_width, 0));
        self.semaphore_need
            .values_mut()
            .for_each(|row| row.resize(new_width, 0));
        new_width - 1
    }

    /// 注册新的 mutex 资源类型，返回其资源 id。
    pub fn register_mutex(&mut self) -> usize {
        self.mutex_owner.push(None);
        self.mutex_owner.len() - 1
    }

    /// 记录一次 semaphore 立即成功的获取。
    pub fn semaphore_acquired(&mut self, tid: ThreadId, sem_id: usize) {
        let width = self.semaphore_total.len();
        let row = Self::ensure_sem_row(&mut self.semaphore_alloc, tid, width);
        row[sem_id] += 1;
    }

    /// 记录一次 semaphore 阻塞等待。
    pub fn semaphore_wait(&mut self, tid: ThreadId, sem_id: usize) {
        let width = self.semaphore_total.len();
        let row = Self::ensure_sem_row(&mut self.semaphore_need, tid, width);
        row[sem_id] += 1;
    }

    /// 清除一次 semaphore 阻塞等待。
    pub fn semaphore_cancel_wait(&mut self, tid: ThreadId, sem_id: usize) {
        if let Some(row) = self.semaphore_need.get_mut(&tid) {
            if sem_id < row.len() && row[sem_id] > 0 {
                row[sem_id] -= 1;
            }
        }
        Self::trim_sem_row(&mut self.semaphore_need, tid);
    }

    /// 记录一次 semaphore 释放；若唤醒了线程，则资源立即转交给对方。
    pub fn semaphore_release(
        &mut self,
        tid: ThreadId,
        sem_id: usize,
        waking_tid: Option<ThreadId>,
    ) {
        if let Some(row) = self.semaphore_alloc.get_mut(&tid) {
            if sem_id < row.len() && row[sem_id] > 0 {
                row[sem_id] -= 1;
            }
        }
        Self::trim_sem_row(&mut self.semaphore_alloc, tid);
        if let Some(waking_tid) = waking_tid {
            self.semaphore_cancel_wait(waking_tid, sem_id);
            self.semaphore_acquired(waking_tid, sem_id);
        }
    }

    /// 计算当前可用资源向量。
    fn semaphore_available(&self) -> Vec<usize> {
        let mut available = self.semaphore_total.clone();
        for row in self.semaphore_alloc.values() {
            for (idx, count) in row.iter().enumerate() {
                available[idx] = available[idx].saturating_sub(*count);
            }
        }
        available
    }

    /// 检查把当前 semaphore 请求加入等待矩阵后是否会进入不安全状态。
    pub fn semaphore_would_deadlock(
        &self,
        active_threads: &[ThreadId],
        tid: ThreadId,
        sem_id: usize,
    ) -> bool {
        let width = self.semaphore_total.len();
        if sem_id >= width {
            return false;
        }
        let mut work = self.semaphore_available();
        let mut finish = vec![false; active_threads.len()];
        loop {
            let mut progressed = false;
            for (idx, thread_id) in active_threads.iter().copied().enumerate() {
                if finish[idx] {
                    continue;
                }
                let mut need = self
                    .semaphore_need
                    .get(&thread_id)
                    .cloned()
                    .unwrap_or_else(|| vec![0; width]);
                if thread_id == tid {
                    need[sem_id] += 1;
                }
                if need.iter().zip(work.iter()).all(|(need, work)| need <= work) {
                    let alloc = self
                        .semaphore_alloc
                        .get(&thread_id)
                        .cloned()
                        .unwrap_or_else(|| vec![0; width]);
                    for (work_item, alloc_item) in work.iter_mut().zip(alloc.iter()) {
                        *work_item += *alloc_item;
                    }
                    finish[idx] = true;
                    progressed = true;
                }
            }
            if !progressed {
                break;
            }
        }
        finish.iter().any(|finished| !finished)
    }

    /// 记录一次 mutex 成功获取。
    pub fn mutex_acquired(&mut self, tid: ThreadId, mutex_id: usize) {
        self.mutex_wait.remove(&tid);
        self.mutex_owner[mutex_id] = Some(tid);
    }

    /// 记录一次 mutex 阻塞等待。
    pub fn mutex_wait(&mut self, tid: ThreadId, mutex_id: usize) {
        self.mutex_wait.insert(tid, mutex_id);
    }

    /// 记录一次 mutex 释放；若唤醒了线程，则所有权立即转交给对方。
    pub fn mutex_release(
        &mut self,
        tid: ThreadId,
        mutex_id: usize,
        waking_tid: Option<ThreadId>,
    ) {
        if self.mutex_owner.get(mutex_id).copied().flatten() == Some(tid) {
            if let Some(waking_tid) = waking_tid {
                self.mutex_wait.remove(&waking_tid);
                self.mutex_owner[mutex_id] = Some(waking_tid);
            } else {
                self.mutex_owner[mutex_id] = None;
            }
        }
    }

    /// 检查当前线程等待 mutex 时是否形成等待环。
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

    /// 清理线程的等待状态，避免残留的需求边影响后续检测。
    pub fn on_thread_exit(&mut self, tid: ThreadId) {
        self.semaphore_need.remove(&tid);
        self.mutex_wait.remove(&tid);
    }
}

impl Process {
    /// exec：替换当前进程的地址空间和主线程上下文
    ///
    /// 注意：只支持单线程进程执行 exec
    pub fn exec(&mut self, elf: ElfFile) {
        let (proc, thread) = Process::from_elf(elf).unwrap();
        self.address_space = proc.address_space;
        let processor: *mut ProcessorInner = PROCESSOR.get_mut() as *mut ProcessorInner;
        unsafe {
            let pthreads = (*processor).get_thread(self.pid).unwrap();
            (*processor).get_task(pthreads[0]).unwrap().context = thread.context;
        }
    }

    /// fork：创建子进程（复制地址空间和主线程上下文）
    ///
    /// 子进程继承父进程的地址空间（深拷贝）、文件描述符和信号配置。
    /// 同步原语列表不继承（子进程创建空的列表）。
    pub fn fork(&mut self) -> Option<(Self, Thread)> {
        let pid = ProcId::new();
        // 深拷贝地址空间
        let parent_addr_space = &self.address_space;
        let mut address_space: AddressSpace<Sv39, Sv39Manager> = AddressSpace::new();
        parent_addr_space.cloneself(&mut address_space);
        map_portal(&address_space);
        // 复制主线程上下文
        let processor: *mut ProcessorInner = PROCESSOR.get_mut() as *mut ProcessorInner;
        let pthreads = unsafe { (*processor).get_thread(self.pid).unwrap() };
        let context = unsafe {
            (*processor).get_task(pthreads[0]).unwrap().context.context.clone()
        };
        let satp = (8 << 60) | address_space.root_ppn().val();
        let thread = Thread::new(satp, context);
        // 复制文件描述符表
        let new_fd_table: Vec<Option<Mutex<Fd>>> = self.fd_table
            .iter()
            .map(|fd| fd.as_ref().map(|f| Mutex::new(f.lock().clone())))
            .collect();
        Some((
            Self {
                pid,
                address_space,
                fd_table: new_fd_table,
                signal: self.signal.from_fork(),
                // 子进程的同步原语列表初始为空
                semaphore_list: Vec::new(),
                mutex_list: Vec::new(),
                condvar_list: Vec::new(),
                deadlock: DeadlockState::default(),
            },
            thread,
        ))
    }

    /// 从 ELF 文件创建进程和主线程
    ///
    /// 解析 ELF 段，建立地址空间，分配用户栈，创建初始上下文。
    pub fn from_elf(elf: ElfFile) -> Option<(Self, Thread)> {
        let entry = match elf.header.pt2 {
            HeaderPt2::Header64(pt2)
                if pt2.type_.as_type() == header::Type::Executable
                    && pt2.machine.as_machine() == Machine::RISC_V =>
            { pt2.entry_point as usize }
            _ => None?,
        };

        const PAGE_SIZE: usize = 1 << Sv39::PAGE_BITS;
        const PAGE_MASK: usize = PAGE_SIZE - 1;

        let mut address_space = AddressSpace::new();
        for program in elf.program_iter() {
            if !matches!(program.get_type(), Ok(program::Type::Load)) { continue; }
            let off_file = program.offset() as usize;
            let len_file = program.file_size() as usize;
            let off_mem = program.virtual_addr() as usize;
            let end_mem = off_mem + program.mem_size() as usize;
            assert_eq!(off_file & PAGE_MASK, off_mem & PAGE_MASK);
            let mut flags: [u8; 5] = *b"U___V";
            if program.flags().is_execute() { flags[1] = b'X'; }
            if program.flags().is_write() { flags[2] = b'W'; }
            if program.flags().is_read() { flags[3] = b'R'; }
            address_space.map(
                VAddr::new(off_mem).floor()..VAddr::new(end_mem).ceil(),
                &elf.input[off_file..][..len_file],
                off_mem & PAGE_MASK,
                parse_flags(unsafe { core::str::from_utf8_unchecked(&flags) }).unwrap(),
            );
        }
        // 分配 2 页用户栈
        let stack = unsafe {
            alloc_zeroed(Layout::from_size_align_unchecked(
                2 << Sv39::PAGE_BITS, 1 << Sv39::PAGE_BITS,
            ))
        };
        address_space.map_extern(
            VPN::new((1 << 26) - 2)..VPN::new(1 << 26),
            PPN::new(stack as usize >> Sv39::PAGE_BITS),
            build_flags("U_WRV"),
        );
        map_portal(&address_space);
        let satp = (8 << 60) | address_space.root_ppn().val();
        let mut context = LocalContext::user(entry);
        *context.sp_mut() = 1 << 38;
        let thread = Thread::new(satp, context);

        Some((
            Self {
                pid: ProcId::new(),
                address_space,
                fd_table: vec![
                    // stdin
                    Some(Mutex::new(Fd::Empty { read: true, write: false })),
                    // stdout
                    Some(Mutex::new(Fd::Empty { read: false, write: true })),
                    // stderr
                    Some(Mutex::new(Fd::Empty { read: false, write: true })),
                ],
                signal: Box::new(SignalImpl::new()),
                semaphore_list: Vec::new(),
                mutex_list: Vec::new(),
                condvar_list: Vec::new(),
                deadlock: DeadlockState::default(),
            },
            thread,
        ))
    }
}
