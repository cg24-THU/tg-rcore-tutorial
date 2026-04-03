//! # 第八章：并发
//!
//! 本章在第七章"进程间通信与信号"的基础上，引入了 **线程** 和 **同步原语**。
//!
//! ## 核心概念
//!
//! ### 1. 线程（Thread）
//!
//! 将原来的"进程"拆分为两个独立的抽象：
//! - **Process（进程）**：管理共享资源（地址空间、文件描述符表、同步原语列表、信号）
//! - **Thread（线程）**：管理执行状态（上下文、TID）
//!
//! 同一进程的多个线程共享地址空间，但各自有独立的用户栈和执行上下文。
//!
//! ### 2. 同步原语
//!
//! - **Mutex（互斥锁）**：保证临界区互斥访问
//! - **Semaphore（信号量）**：P/V 操作，支持计数型资源管理
//! - **Condvar（条件变量）**：配合互斥锁使用，支持线程等待/唤醒
//!
//! ### 3. 线程阻塞
//!
//! 当线程尝试获取已被占用的锁/信号量时，内核将其标记为**阻塞态**，
//! 从就绪队列中移除。当持有者释放资源时，唤醒等待队列中的线程。
//!
//! ## 与第七章的主要区别
//!
//! | 特性 | 第七章 | 第八章 |
//! |------|--------|--------|
//! | 执行单元 | Process（进程即线程） | Thread（线程），Process 仅管理资源 |
//! | 管理器 | PManager | PThreadManager（进程 + 线程双层管理） |
//! | 同步 | 无 | Mutex / Semaphore / Condvar |
//! | task-manage feature | `proc` | `thread` |
//! | 新增依赖 | — | tg-sync |
//!
//! 教程阅读建议：
//!
//! - 先看 `rust_main`：抓住“线程创建 + 双层管理 + trap 分发”总流程；
//! - 再看主循环中 `SEMAPHORE_DOWN/MUTEX_LOCK/CONDVAR_WAIT` 分支：理解阻塞态切换；
//! - 最后看 `impls`：把线程、信号、同步三类系统调用如何交织串起来。

// 不使用标准库
#![no_std]
// 不使用默认 main 入口
#![no_main]
#![cfg_attr(target_arch = "riscv64", deny(warnings, missing_docs))]
#![cfg_attr(not(target_arch = "riscv64"), allow(dead_code, unused_imports))]

/// 文件系统模块：easy-fs 封装 + 统一 Fd 枚举
mod fs;
/// 进程与线程模块：Process（资源容器）和 Thread（执行单元）
mod process;
/// 处理器模块：PROCESSOR 全局管理器（PThreadManager）
mod processor;
/// VirtIO 块设备驱动
mod virtio_block;
/// VirtIO GPU framebuffer 驱动
mod virtio_gpu;
/// VirtIO 键盘输入驱动
mod virtio_input;

#[macro_use]
extern crate tg_console;

extern crate alloc;

use crate::{
    fs::{read_all, FS},
    impls::{Sv39Manager, SyscallContext},
    process::{Process, Thread},
    processor::{ProcManager, ProcessorInner, ThreadManager},
};
use alloc::alloc::alloc;
use core::{alloc::Layout, cell::UnsafeCell, mem::MaybeUninit};
use impls::Console;
pub use processor::PROCESSOR;
use riscv::register::*;
#[cfg(not(target_arch = "riscv64"))]
use stub::Sv39;
use tg_console::log;
use tg_easy_fs::{FSManager, OpenFlags};
use tg_kernel_context::foreign::MultislotPortal;
#[cfg(target_arch = "riscv64")]
use tg_kernel_vm::page_table::Sv39;
use tg_kernel_vm::{
    page_table::{MmuMeta, VAddr, VmFlags, VmMeta, PPN, VPN},
    AddressSpace,
};
use tg_sbi;
use tg_signal::SignalResult;
use tg_syscall::Caller;
use tg_task_manage::ProcId;
use xmas_elf::ElfFile;

/// 构建 VmFlags
#[cfg(target_arch = "riscv64")]
const fn build_flags(s: &str) -> VmFlags<Sv39> {
    VmFlags::build_from_str(s)
}

/// 解析 VmFlags
#[cfg(target_arch = "riscv64")]
fn parse_flags(s: &str) -> Result<VmFlags<Sv39>, ()> {
    s.parse()
}

#[cfg(not(target_arch = "riscv64"))]
use stub::{build_flags, parse_flags};

// 内核入口，栈 = 32 页 = 128 KiB。
//
// 这里不再调用 tg_linker::boot0! 宏，避免外部已发布版本与 Rust 2024
// 在属性语义上的兼容差异影响本 crate 的发布校验。
#[cfg(target_arch = "riscv64")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
unsafe extern "C" fn _start() -> ! {
    const STACK_SIZE: usize = 32 * 4096;
    #[unsafe(link_section = ".boot.stack")]
    static mut STACK: [u8; STACK_SIZE] = [0u8; STACK_SIZE];

    core::arch::naked_asm!(
        "la sp, {stack} + {stack_size}",
        "j  {main}",
        stack = sym STACK,
        stack_size = const STACK_SIZE,
        main = sym rust_main,
    )
}

/// 物理内存容量 = 48 MiB
const MEMORY: usize = 48 << 20;
/// 异界传送门所在虚页
const PROTAL_TRANSIT: VPN<Sv39> = VPN::MAX;

/// 内核地址空间的全局存储
struct KernelSpace {
    inner: UnsafeCell<MaybeUninit<AddressSpace<Sv39, Sv39Manager>>>,
}

unsafe impl Sync for KernelSpace {}

impl KernelSpace {
    const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    unsafe fn write(&self, space: AddressSpace<Sv39, Sv39Manager>) {
        unsafe { *self.inner.get() = MaybeUninit::new(space) };
    }

    unsafe fn assume_init_ref(&self) -> &AddressSpace<Sv39, Sv39Manager> {
        unsafe { &*(*self.inner.get()).as_ptr() }
    }
}

/// 内核地址空间全局实例
static KERNEL_SPACE: KernelSpace = KernelSpace::new();

/// VirtIO MMIO 设备地址范围
pub const MMIO: &[(usize, usize)] = &[
    (0x1000_1000, 0x00_1000),
    (0x1000_2000, 0x00_1000),
    (0x1000_3000, 0x00_1000),
];

/// 内核主函数
///
/// 与第七章相比：
/// - 新增 `init_thread`（线程系统调用）和 `init_sync_mutex`（同步原语系统调用）
/// - 使用 `PThreadManager`（双层管理器）替代 `PManager`
/// - 初始化时同时创建 Process 和 Thread
/// - 主循环中新增**线程阻塞**处理（SEMAPHORE_DOWN/MUTEX_LOCK/CONDVAR_WAIT）
extern "C" fn rust_main() -> ! {
    let layout = tg_linker::KernelLayout::locate();
    // 步骤 1：BSS 清零
    unsafe { layout.zero_bss() };
    // 步骤 2：控制台和日志
    tg_console::init_console(&Console);
    tg_console::set_log_level(option_env!("LOG"));
    tg_console::test_log();
    // 步骤 3：堆分配器
    tg_kernel_alloc::init(layout.start() as _);
    unsafe {
        tg_kernel_alloc::transfer(core::slice::from_raw_parts_mut(
            layout.end() as _,
            MEMORY - layout.len(),
        ))
    };
    // 步骤 4：异界传送门
    let portal_size = MultislotPortal::calculate_size(1);
    let portal_layout = Layout::from_size_align(portal_size, 1 << Sv39::PAGE_BITS).unwrap();
    let portal_ptr = unsafe { alloc(portal_layout) };
    assert!(portal_layout.size() < 1 << Sv39::PAGE_BITS);
    // 步骤 5：内核地址空间
    kernel_space(layout, MEMORY, portal_ptr as _);
    // 步骤 6：异界传送门初始化
    let portal = unsafe { MultislotPortal::init_transit(PROTAL_TRANSIT.base().val(), 1) };
    // 步骤 7：系统调用初始化
    tg_syscall::init_io(&SyscallContext);
    tg_syscall::init_process(&SyscallContext);
    tg_syscall::init_scheduling(&SyscallContext);
    tg_syscall::init_clock(&SyscallContext);
    tg_syscall::init_memory(&SyscallContext);
    tg_syscall::init_signal(&SyscallContext);
    tg_syscall::init_thread(&SyscallContext);       // 本章新增：线程系统调用
    tg_syscall::init_sync_mutex(&SyscallContext);   // 本章新增：同步原语系统调用
    // 步骤 8：加载 initproc（返回 Process + Thread）
    let initproc = read_all(FS.open("initproc", OpenFlags::RDONLY).unwrap());
    if let Some((process, thread)) = Process::from_elf(ElfFile::new(initproc.as_slice()).unwrap()) {
        // 初始化双层管理器：ProcManager（进程）+ ThreadManager（线程）
        PROCESSOR.get_mut().set_proc_manager(ProcManager::new());
        PROCESSOR.get_mut().set_manager(ThreadManager::new());
        let (pid, tid) = (process.pid, thread.tid);
        PROCESSOR
            .get_mut()
            .add_proc(pid, process, ProcId::from_usize(usize::MAX));
        PROCESSOR.get_mut().add(tid, thread, pid);
    }

    // ─── 主调度循环 ───
    loop {
        let processor: *mut ProcessorInner = PROCESSOR.get_mut() as *mut ProcessorInner;
        if let Some(task) = unsafe { (*processor).find_next() } {
            unsafe { task.context.execute(portal, ()) };

            match scause::read().cause() {
                // ─── 系统调用 ───
                scause::Trap::Exception(scause::Exception::UserEnvCall) => {
                    use tg_syscall::{SyscallId as Id, SyscallResult as Ret};
                    let ctx = &mut task.context.context;
                    ctx.move_next();
                    let id: Id = ctx.a(7).into();
                    let args = [ctx.a(0), ctx.a(1), ctx.a(2), ctx.a(3), ctx.a(4), ctx.a(5)];
                    let syscall_ret = tg_syscall::handle(Caller { entity: 0, flow: 0 }, id, args);

                    // ─── 信号处理 ───
                    let current_proc = unsafe { (*processor).get_current_proc().unwrap() };
                    match current_proc.signal.handle_signals(ctx) {
                        SignalResult::ProcessKilled(exit_code) => unsafe {
                            (*processor).make_current_exited(exit_code as _)
                        },
                        _ => match syscall_ret {
                            Ret::Done(ret) => match id {
                                Id::EXIT => unsafe { (*processor).make_current_exited(ret) },
                                // ─── 本章新增：同步原语阻塞处理 ───
                                // 当 semaphore_down / mutex_lock / condvar_wait 返回 -1 时，
                                // 表示资源不可用，将当前线程标记为阻塞态
                                Id::SEMAPHORE_DOWN | Id::MUTEX_LOCK | Id::CONDVAR_WAIT => {
                                    let ctx = &mut task.context.context;
                                    *ctx.a_mut(0) = ret as _;
                                    if ret == -1 {
                                        // 阻塞：从就绪队列移除，等待资源释放后唤醒
                                        unsafe { (*processor).make_current_blocked() };
                                    } else {
                                        // 成功获取：正常挂起（时间片轮转）
                                        unsafe { (*processor).make_current_suspend() };
                                    }
                                }
                                _ => {
                                    let ctx = &mut task.context.context;
                                    *ctx.a_mut(0) = ret as _;
                                    unsafe { (*processor).make_current_suspend() };
                                }
                            },
                            Ret::Unsupported(_) => {
                                log::info!("id = {id:?}");
                                unsafe { (*processor).make_current_exited(-2) };
                            }
                        },
                    }
                }
                e => {
                    log::error!(
                        "unsupported trap: {e:?}, sepc={:#x}, stval={:#x}",
                        sepc::read(),
                        stval::read()
                    );
                    unsafe { (*processor).make_current_exited(-3) };
                }
            }
        } else {
            println!("no task");
            break;
        }
    }

    tg_sbi::shutdown(false)
}

/// panic 处理
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("{info}");
    tg_sbi::shutdown(true)
}

/// 建立内核地址空间（与前几章相同）
fn kernel_space(layout: tg_linker::KernelLayout, memory: usize, portal: usize) {
    let mut space = AddressSpace::new();
    for region in layout.iter() {
        log::info!("{region}");
        use tg_linker::KernelRegionTitle::*;
        let flags = match region.title {
            Text => "X_RV",
            Rodata => "__RV",
            Data | Boot => "_WRV",
        };
        let s = VAddr::<Sv39>::new(region.range.start);
        let e = VAddr::<Sv39>::new(region.range.end);
        space.map_extern(
            s.floor()..e.ceil(),
            PPN::new(s.floor().val()),
            build_flags(flags),
        )
    }
    let s = VAddr::<Sv39>::new(layout.end());
    let e = VAddr::<Sv39>::new(layout.start() + memory);
    log::info!("(heap) ---> {:#10x}..{:#10x}", s.val(), e.val());
    space.map_extern(
        s.floor()..e.ceil(),
        PPN::new(s.floor().val()),
        build_flags("_WRV"),
    );
    space.map_extern(
        PROTAL_TRANSIT..PROTAL_TRANSIT + 1,
        PPN::new(portal >> Sv39::PAGE_BITS),
        build_flags("__G_XWRV"),
    );
    println!();
    for (base, len) in MMIO {
        let s = VAddr::<Sv39>::new(*base);
        let e = VAddr::<Sv39>::new(*base + *len);
        log::info!("MMIO range -> {:#10x}..{:#10x}", s.val(), e.val());
        space.map_extern(
            s.floor()..e.ceil(),
            PPN::new(s.floor().val()),
            build_flags("_WRV"),
        );
    }
    unsafe { satp::set(satp::Mode::Sv39, 0, space.root_ppn().val()) };
    unsafe { KERNEL_SPACE.write(space) };
}

/// 将异界传送门映射到用户地址空间
fn map_portal(space: &AddressSpace<Sv39, Sv39Manager>) {
    let portal_idx = PROTAL_TRANSIT.index_in(Sv39::MAX_LEVEL);
    space.root()[portal_idx] = unsafe { KERNEL_SPACE.assume_init_ref() }.root()[portal_idx];
}

/// 各种接口库的实现
///
/// 与第七章相比，本章新增了：
/// - `Thread` trait（thread_create/gettid/waittid）
/// - `SyncMutex` trait（mutex/semaphore/condvar 系统调用）
/// - 所有操作通过 `ProcessorInner`（PThreadManager）进行双层管理
mod impls {
    use crate::{
        build_flags,
        fs::{read_all, Fd, FS},
        processor::ProcessorInner,
        virtio_gpu::GPU_DEVICE,
        virtio_input::INPUT_DEVICE,
        Sv39, Thread, PROCESSOR,
    };
    use alloc::sync::Arc;
    use alloc::{alloc::alloc_zeroed, string::String};
    use core::{alloc::Layout, cmp::min, ptr::NonNull};
    use spin::Mutex;
    use tg_console::log;
    use tg_easy_fs::{make_pipe, FSManager, OpenFlags, UserBuffer};
    use tg_kernel_vm::{
        page_table::{MmuMeta, Pte, VAddr, VmFlags, VmMeta, PPN, VPN},
        PageManager,
    };
    use tg_signal::SignalNo;
    use tg_sync::{Condvar, Mutex as MutexTrait, MutexBlocking, Semaphore};
    use tg_syscall::*;
    use tg_task_manage::{ProcId, ThreadId};
    use virtio_drivers::InputEvent;
    use xmas_elf::ElfFile;

    const DEADLOCK_DETECTED: isize = -0xdead;
    const EV_KEY: u16 = 0x01;
    const SEEK_SET: usize = 0;
    const SEEK_CUR: usize = 1;
    const SEEK_END: usize = 2;

    // ─── Sv39 页表管理器 ───

    /// Sv39 页表管理器
    #[repr(transparent)]
    pub struct Sv39Manager(NonNull<Pte<Sv39>>);

    impl Sv39Manager {
        const OWNED: VmFlags<Sv39> = unsafe { VmFlags::from_raw(1 << 8) };
        #[inline]
        fn page_alloc<T>(count: usize) -> *mut T {
            unsafe {
                alloc_zeroed(Layout::from_size_align_unchecked(
                    count << Sv39::PAGE_BITS,
                    1 << Sv39::PAGE_BITS,
                ))
            }
            .cast()
        }
    }

    impl PageManager<Sv39> for Sv39Manager {
        #[inline]
        fn new_root() -> Self { Self(NonNull::new(Self::page_alloc(1)).unwrap()) }
        #[inline]
        fn root_ppn(&self) -> PPN<Sv39> { PPN::new(self.0.as_ptr() as usize >> Sv39::PAGE_BITS) }
        #[inline]
        fn root_ptr(&self) -> NonNull<Pte<Sv39>> { self.0 }
        #[inline]
        fn p_to_v<T>(&self, ppn: PPN<Sv39>) -> NonNull<T> {
            unsafe { NonNull::new_unchecked(VPN::<Sv39>::new(ppn.val()).base().as_mut_ptr()) }
        }
        #[inline]
        fn v_to_p<T>(&self, ptr: NonNull<T>) -> PPN<Sv39> {
            PPN::new(VAddr::<Sv39>::new(ptr.as_ptr() as _).floor().val())
        }
        #[inline]
        fn check_owned(&self, pte: Pte<Sv39>) -> bool { pte.flags().contains(Self::OWNED) }
        #[inline]
        fn allocate(&mut self, len: usize, flags: &mut VmFlags<Sv39>) -> NonNull<u8> {
            *flags |= Self::OWNED;
            NonNull::new(Self::page_alloc(len)).unwrap()
        }
        fn deallocate(&mut self, _pte: Pte<Sv39>, _len: usize) -> usize { todo!() }
        fn drop_root(&mut self) { todo!() }
    }

    // ─── 控制台 ───

    /// 控制台实现
    pub struct Console;
    impl tg_console::Console for Console {
        #[inline]
        fn put_char(&self, c: u8) { tg_sbi::console_putchar(c); }
    }

    // ─── 系统调用实现 ───

    /// 系统调用上下文
    pub struct SyscallContext;
    const READABLE: VmFlags<Sv39> = build_flags("RV");
    const WRITEABLE: VmFlags<Sv39> = build_flags("W_V");

    fn translate_input_key(code: u16) -> Option<u16> {
        Some(match code {
            1 => 27,
            2 => b'1' as u16,
            3 => b'2' as u16,
            4 => b'3' as u16,
            5 => b'4' as u16,
            6 => b'5' as u16,
            7 => b'6' as u16,
            8 => b'7' as u16,
            9 => b'8' as u16,
            10 => b'9' as u16,
            11 => b'0' as u16,
            14 => 0x7f,
            15 => 9,
            16 => b'q' as u16,
            17 => b'w' as u16,
            18 => b'e' as u16,
            19 => b'r' as u16,
            20 => b't' as u16,
            21 => b'y' as u16,
            22 => b'u' as u16,
            23 => b'i' as u16,
            24 => b'o' as u16,
            25 => b'p' as u16,
            28 => 13,
            29 => 0x80 + 0x1d,
            30 => b'a' as u16,
            31 => b's' as u16,
            32 => b'd' as u16,
            33 => b'f' as u16,
            34 => b'g' as u16,
            35 => b'h' as u16,
            36 => b'j' as u16,
            37 => b'k' as u16,
            38 => b'l' as u16,
            42 | 54 => 0x80 + 0x36,
            44 => b'z' as u16,
            45 => b'x' as u16,
            46 => b'c' as u16,
            47 => b'v' as u16,
            48 => b'b' as u16,
            49 => b'n' as u16,
            50 => b'm' as u16,
            56 | 100 => 0x80 + 0x38,
            57 => 0xa2,
            59 => 0x80 + 0x3b,
            60 => 0x80 + 0x3c,
            61 => 0x80 + 0x3d,
            62 => 0x80 + 0x3e,
            63 => 0x80 + 0x3f,
            64 => 0x80 + 0x40,
            65 => 0x80 + 0x41,
            66 => 0x80 + 0x42,
            67 => 0x80 + 0x43,
            68 => 0x80 + 0x44,
            87 => 0x80 + 0x57,
            88 => 0x80 + 0x58,
            103 => 0xad,
            105 => 0xac,
            106 => 0xae,
            108 => 0xaf,
            _ => return None,
        })
    }

    fn copy_user_bytes(current: &crate::process::Process, user_ptr: usize, buffer: &mut [u8]) -> bool {
        let mut copied = 0usize;
        while copied < buffer.len() {
            let addr = user_ptr + copied;
            let chunk = min(
                (1 << Sv39::PAGE_BITS) - VAddr::<Sv39>::new(addr).offset(),
                buffer.len() - copied,
            );
            let Some(src) = current.address_space.translate::<u8>(VAddr::new(addr), READABLE) else {
                return false;
            };
            buffer[copied..copied + chunk]
                .copy_from_slice(unsafe { core::slice::from_raw_parts(src.as_ptr(), chunk) });
            copied += chunk;
        }
        true
    }

    fn user_buffer_segments(
        current: &crate::process::Process,
        user_ptr: usize,
        len: usize,
        flags: VmFlags<Sv39>,
    ) -> Option<alloc::vec::Vec<&'static mut [u8]>> {
        let mut buffers = alloc::vec::Vec::new();
        let mut offset = 0usize;
        while offset < len {
            let addr = user_ptr + offset;
            let chunk = min(
                (1 << Sv39::PAGE_BITS) - VAddr::<Sv39>::new(addr).offset(),
                len - offset,
            );
            let ptr = current
                .address_space
                .translate::<u8>(VAddr::new(addr), flags)?;
            unsafe { buffers.push(core::slice::from_raw_parts_mut(ptr.as_ptr(), chunk)); }
            offset += chunk;
        }
        Some(buffers)
    }

    /// IO 系统调用（与第七章基本相同）
    ///
    /// 注意：本章通过 `get_current_proc()` 获取当前线程所属的进程，
    /// 而非直接 `current()`，因为 fd_table 属于进程而非线程。
    impl IO for SyscallContext {
        fn write(&self, _caller: Caller, fd: usize, buf: usize, count: usize) -> isize {
            let current = PROCESSOR.get_mut().get_current_proc().unwrap();
            if fd == STDOUT || fd == STDDEBUG {
                let mut data = alloc::vec![0u8; count];
                if !copy_user_bytes(current, buf, &mut data) {
                    log::error!("ptr not readable");
                    return -1;
                }
                print!("{}", unsafe { core::str::from_utf8_unchecked(&data) });
                count as _
            } else if fd < current.fd_table.len() && let Some(file) = &current.fd_table[fd] {
                let file = file.lock();
                if file.writable() {
                    if let Some(buffers) = user_buffer_segments(current, buf, count, READABLE) {
                        file.write(UserBuffer::new(buffers)) as _
                    } else {
                        log::error!("ptr not readable");
                        -1
                    }
                } else { log::error!("file not writable"); -1 }
            } else { log::error!("unsupported fd: {fd}"); -1 }
        }

        fn read(&self, _caller: Caller, fd: usize, buf: usize, count: usize) -> isize {
            let current = PROCESSOR.get_mut().get_current_proc().unwrap();
            if fd == STDIN {
                if let Some(buffers) = user_buffer_segments(current, buf, count, WRITEABLE) {
                    for segment in buffers {
                        for byte in segment {
                            *byte = tg_sbi::console_getchar() as u8;
                        }
                    }
                    count as _
                } else {
                    log::error!("ptr not writeable");
                    -1
                }
            } else if fd < current.fd_table.len() && let Some(file) = &current.fd_table[fd] {
                let file = file.lock();
                if file.readable() {
                    if let Some(buffers) = user_buffer_segments(current, buf, count, WRITEABLE) {
                        file.read(UserBuffer::new(buffers)) as _
                    } else {
                        log::error!("ptr not writeable");
                        -1
                    }
                } else { log::error!("file not readable"); -1 }
            } else { log::error!("unsupported fd: {fd}"); -1 }
        }

        fn open(&self, _caller: Caller, path: usize, flags: usize) -> isize {
            let current = PROCESSOR.get_mut().get_current_proc().unwrap();
            if let Some(ptr) = current.address_space.translate(VAddr::new(path), READABLE) {
                let mut string = String::new();
                let mut raw_ptr: *mut u8 = ptr.as_ptr();
                loop {
                    unsafe {
                        let ch = *raw_ptr;
                        if ch == 0 { break; }
                        string.push(ch as char);
                        raw_ptr = (raw_ptr as usize + 1) as *mut u8;
                    }
                }
                if let Some(file_handle) =
                    FS.open(string.as_str(), OpenFlags::from_bits(flags as u32).unwrap())
                {
                    let new_fd = current.fd_table.len();
                    current.fd_table.push(Some(Mutex::new(Fd::File((*file_handle).clone()))));
                    new_fd as isize
                } else { -1 }
            } else { log::error!("ptr not writeable"); -1 }
        }

        #[inline]
        fn close(&self, _caller: Caller, fd: usize) -> isize {
            let current = PROCESSOR.get_mut().get_current_proc().unwrap();
            if fd >= current.fd_table.len() || current.fd_table[fd].is_none() { return -1; }
            current.fd_table[fd].take();
            0
        }

        fn lseek(&self, _caller: Caller, fd: usize, offset: isize, whence: usize) -> isize {
            let current = PROCESSOR.get_mut().get_current_proc().unwrap();
            if fd >= current.fd_table.len() {
                return -1;
            }
            let Some(file) = current.fd_table[fd].as_ref() else {
                return -1;
            };
            let file = file.lock();
            let Fd::File(file) = &*file else {
                return -1;
            };
            let Some(inode) = file.inode.as_ref() else {
                return -1;
            };
            let new_offset = match whence {
                SEEK_SET => offset,
                SEEK_CUR => file.offset.get() as isize + offset,
                SEEK_END => inode.size() as isize + offset,
                _ => return -1,
            };
            if new_offset < 0 {
                return -1;
            }
            file.offset.set(new_offset as usize);
            new_offset
        }

        /// pipe 系统调用
        fn pipe(&self, _caller: Caller, pipe: usize) -> isize {
            let current = PROCESSOR.get_mut().get_current_proc().unwrap();
            let (read_end, write_end) = make_pipe();
            let read_fd = current.fd_table.len();
            let write_fd = read_fd + 1;
            if let Some(mut ptr) = current.address_space
                .translate::<usize>(VAddr::new(pipe), WRITEABLE)
            { unsafe { *ptr.as_mut() = read_fd }; } else { return -1; }
            if let Some(mut ptr) = current.address_space
                .translate::<usize>(VAddr::new(pipe + core::mem::size_of::<usize>()), WRITEABLE)
            { unsafe { *ptr.as_mut() = write_fd }; } else { return -1; }
            current.fd_table.push(Some(Mutex::new(Fd::PipeRead(read_end))));
            current.fd_table.push(Some(Mutex::new(Fd::PipeWrite(write_end))));
            0
        }

        fn fstat(&self, _caller: Caller, fd: usize, st: usize) -> isize {
            let current = PROCESSOR.get_mut().get_current_proc().unwrap();
            if fd >= current.fd_table.len() {
                return -1;
            }
            let Some(file) = current.fd_table[fd].as_ref() else {
                return -1;
            };
            let mut stat = Stat::new();
            match &*file.lock() {
                Fd::File(file) => {
                    let Some(inode) = file.inode.as_ref() else {
                        return -1;
                    };
                    stat.dev = 0;
                    stat.ino = inode.inode_id() as u64;
                    stat.mode = if inode.is_dir() {
                        StatMode::DIR
                    } else {
                        StatMode::FILE
                    };
                    stat.nlink = inode.nlink();
                }
                Fd::PipeRead(_) | Fd::PipeWrite(_) | Fd::Empty { .. } => {
                    stat.mode = StatMode::NULL;
                }
            }
            if let Some(mut ptr) = current.address_space.translate::<Stat>(VAddr::new(st), WRITEABLE) {
                *unsafe { ptr.as_mut() } = stat;
                0
            } else {
                -1
            }
        }

        fn framebuffer_get_info(&self, _caller: Caller, info: usize) -> isize {
            let current = PROCESSOR.get_mut().get_current_proc().unwrap();
            let gpu = GPU_DEVICE.lock();
            let frame_info = FramebufferInfo {
                width: gpu.width as u32,
                height: gpu.height as u32,
                stride: gpu.stride as u32,
                format: FRAMEBUFFER_FORMAT_BGRX8888,
            };
            drop(gpu);
            if let Some(mut ptr) = current
                .address_space
                .translate::<FramebufferInfo>(VAddr::new(info), WRITEABLE)
            {
                *unsafe { ptr.as_mut() } = frame_info;
                0
            } else {
                -1
            }
        }

        fn framebuffer_flush(
            &self,
            _caller: Caller,
            buf: usize,
            len: usize,
            width: usize,
            height: usize,
            stride: usize,
        ) -> isize {
            let current = PROCESSOR.get_mut().get_current_proc().unwrap();
            let Some(required_len) = stride.checked_mul(height) else {
                return -1;
            };
            if width == 0 || height == 0 || stride < width * 4 || len < required_len {
                return -1;
            }

            let mut gpu = GPU_DEVICE.lock();
            let dst_width = min(width, gpu.width);
            let dst_height = min(height, gpu.height);
            let dst_x = (gpu.width.saturating_sub(dst_width)) / 2;
            let dst_y = (gpu.height.saturating_sub(dst_height)) / 2;
            let dst_stride = gpu.stride;
            {
                let framebuffer = gpu.framebuffer();
                for row in 0..dst_height {
                    let dst_offset = (dst_y + row) * dst_stride + dst_x * 4;
                    let dst_row = &mut framebuffer[dst_offset..dst_offset + dst_width * 4];
                    let src_offset = row * stride;
                    if !copy_user_bytes(current, buf + src_offset, dst_row) {
                        return -1;
                    }
                }
            }
            gpu.flush();
            dst_height as isize
        }

        fn input_next_event(&self, _caller: Caller, event: usize) -> isize {
            let current = PROCESSOR.get_mut().get_current_proc().unwrap();
            let mut input = INPUT_DEVICE.lock();
            let Some(InputEvent {
                event_type,
                code,
                value,
            }) = input.pop_pending_event() else {
                return 0;
            };
            if event_type != EV_KEY {
                return 0;
            }
            let Some(key) = translate_input_key(code) else {
                return 0;
            };
            let translated = InputKeyEvent {
                key,
                pressed: if value == 0 { 0 } else { 1 },
            };
            if let Some(mut ptr) = current
                .address_space
                .translate::<InputKeyEvent>(VAddr::new(event), WRITEABLE)
            {
                *unsafe { ptr.as_mut() } = translated;
                1
            } else {
                -1
            }
        }
    }

    /// 进程管理系统调用
    impl Process for SyscallContext {
        #[inline]
        fn exit(&self, _caller: Caller, exit_code: usize) -> isize { exit_code as isize }

        /// fork：创建子进程（返回 Process + Thread）
        fn fork(&self, _caller: Caller) -> isize {
            let processor: *mut ProcessorInner = PROCESSOR.get_mut() as *mut ProcessorInner;
            let current_proc = unsafe { (*processor).get_current_proc().unwrap() };
            let parent_pid = current_proc.pid;
            let (proc, mut thread) = current_proc.fork().unwrap();
            let pid = proc.pid;
            *thread.context.context.a_mut(0) = 0 as _;
            unsafe {
                (*processor).add_proc(pid, proc, parent_pid);
                (*processor).add(thread.tid, thread, pid);
            }
            pid.get_usize() as isize
        }

        /// exec：从文件系统加载新程序
        fn exec(&self, _caller: Caller, path: usize, count: usize) -> isize {
            const READABLE: VmFlags<Sv39> = build_flags("RV");
            let current = PROCESSOR.get_mut().get_current_proc().unwrap();
            current.address_space
                .translate(VAddr::new(path), READABLE)
                .map(|ptr| unsafe {
                    core::str::from_utf8_unchecked(core::slice::from_raw_parts(ptr.as_ptr(), count))
                })
                .and_then(|name| FS.open(name, OpenFlags::RDONLY))
                .map_or_else(
                    || {
                        log::error!("unknown app, select one in the list: ");
                        FS.readdir("").unwrap().into_iter().for_each(|app| println!("{app}"));
                        println!();
                        -1
                    },
                    |fd| { current.exec(ElfFile::new(&read_all(fd)).unwrap()); 0 },
                )
        }

        fn wait(&self, _caller: Caller, pid: isize, exit_code_ptr: usize) -> isize {
            let processor: *mut ProcessorInner = PROCESSOR.get_mut() as *mut ProcessorInner;
            let current = unsafe { (*processor).get_current_proc().unwrap() };
            const WRITABLE: VmFlags<Sv39> = build_flags("W_V");
            if let Some((dead_pid, exit_code)) =
                unsafe { (*processor).wait(ProcId::from_usize(pid as usize)) }
            {
                if let Some(mut ptr) = current.address_space
                    .translate::<i32>(VAddr::new(exit_code_ptr), WRITABLE)
                { unsafe { *ptr.as_mut() = exit_code as i32 }; }
                return dead_pid.get_usize() as isize;
            } else { return -1; }
        }

        fn getpid(&self, _caller: Caller) -> isize {
            PROCESSOR.get_mut().get_current_proc().unwrap().pid.get_usize() as _
        }

        fn sbrk(&self, _caller: Caller, size: i32) -> isize {
            let current = PROCESSOR.get_mut().get_current_proc().unwrap();
            if let Some(old_brk) = current.change_program_brk(size as isize) {
                old_brk as isize
            } else {
                -1
            }
        }
    }

    impl Scheduling for SyscallContext {
        #[inline]
        fn sched_yield(&self, _caller: Caller) -> isize { 0 }
    }

    impl Clock for SyscallContext {
        #[inline]
        fn clock_gettime(&self, _caller: Caller, clock_id: ClockId, tp: usize) -> isize {
            const WRITABLE: VmFlags<Sv39> = build_flags("W_V");
            match clock_id {
                ClockId::CLOCK_MONOTONIC => {
                    if let Some(mut ptr) = PROCESSOR.get_mut().get_current_proc().unwrap()
                        .address_space.translate(VAddr::new(tp), WRITABLE)
                    {
                        let time = riscv::register::time::read() * 10000 / 125;
                        *unsafe { ptr.as_mut() } = TimeSpec {
                            tv_sec: time / 1_000_000_000,
                            tv_nsec: time % 1_000_000_000,
                        };
                        0
                    } else { log::error!("ptr not readable"); -1 }
                }
                _ => -1,
            }
        }
    }

    impl Memory for SyscallContext {
        #[inline]
        fn mmap(
            &self,
            _caller: Caller,
            _addr: usize,
            _length: usize,
            _prot: i32,
            _flags: i32,
            _fd: i32,
            _offset: usize,
        ) -> isize {
            -1
        }

        #[inline]
        fn munmap(&self, _caller: Caller, _addr: usize, _length: usize) -> isize {
            -1
        }
    }

    /// 信号系统调用（与第七章相同）
    impl Signal for SyscallContext {
        fn kill(&self, _caller: Caller, pid: isize, signum: u8) -> isize {
            if let Some(target_task) = PROCESSOR.get_mut()
                .get_proc(ProcId::from_usize(pid as usize))
            {
                if let Ok(signal_no) = SignalNo::try_from(signum) {
                    if signal_no != SignalNo::ERR {
                        target_task.signal.add_signal(signal_no);
                        return 0;
                    }
                }
            }
            -1
        }

        fn sigaction(&self, _caller: Caller, signum: u8, action: usize, old_action: usize) -> isize {
            if signum as usize > tg_signal::MAX_SIG { return -1; }
            let current = PROCESSOR.get_mut().get_current_proc().unwrap();
            if let Ok(signal_no) = SignalNo::try_from(signum) {
                if signal_no == SignalNo::ERR { return -1; }
                if old_action as usize != 0 {
                    if let Some(mut ptr) = current.address_space.translate(VAddr::new(old_action), WRITEABLE) {
                        if let Some(signal_action) = current.signal.get_action_ref(signal_no) {
                            *unsafe { ptr.as_mut() } = signal_action;
                        } else { return -1; }
                    } else { return -1; }
                }
                if action as usize != 0 {
                    if let Some(ptr) = current.address_space.translate(VAddr::new(action), READABLE) {
                        if !current.signal.set_action(signal_no, &unsafe { *ptr.as_ptr() }) { return -1; }
                    } else { return -1; }
                }
                return 0;
            }
            -1
        }

        fn sigprocmask(&self, _caller: Caller, mask: usize) -> isize {
            PROCESSOR.get_mut().get_current_proc().unwrap().signal.update_mask(mask) as isize
        }

        fn sigreturn(&self, _caller: Caller) -> isize {
            let processor: *mut ProcessorInner = PROCESSOR.get_mut() as *mut ProcessorInner;
            let current = unsafe { (*processor).get_current_proc().unwrap() };
            let current_thread = unsafe { (*processor).current().unwrap() };
            if current.signal.sig_return(&mut current_thread.context.context) { 0 } else { -1 }
        }
    }

    /// 线程系统调用（**本章新增**）
    impl tg_syscall::Thread for SyscallContext {
        /// thread_create：在当前进程中创建新线程
        ///
        /// 为新线程分配独立的用户栈（从高地址向下搜索未映射的页面），
        /// 创建新的执行上下文，入口为 entry，参数为 arg。
        fn thread_create(&self, _caller: Caller, entry: usize, arg: usize) -> isize {
            let processor: *mut ProcessorInner = PROCESSOR.get_mut() as *mut ProcessorInner;
            let current_proc = unsafe { (*processor).get_current_proc().unwrap() };
            // 从最高用户栈位置向下搜索空闲的页表区域
            let mut vpn = VPN::<Sv39>::new((1 << 26) - 2);
            let addrspace = &mut current_proc.address_space;
            loop {
                let idx = vpn.index_in(Sv39::MAX_LEVEL);
                if !addrspace.root()[idx].is_valid() { break; }
                vpn = VPN::<Sv39>::new(vpn.val() - 3);
            }
            // 分配 2 页用户栈
            let stack = unsafe {
                alloc_zeroed(Layout::from_size_align_unchecked(
                    2 << Sv39::PAGE_BITS, 1 << Sv39::PAGE_BITS,
                ))
            };
            addrspace.map_extern(vpn..vpn + 2, PPN::new(stack as usize >> Sv39::PAGE_BITS), build_flags("U_WRV"));
            let satp = (8 << 60) | addrspace.root_ppn().val();
            let mut context = tg_kernel_context::LocalContext::user(entry);
            *context.sp_mut() = (vpn + 2).base().val();
            *context.a_mut(0) = arg;
            let thread = Thread::new(satp, context);
            let tid = thread.tid;
            unsafe { (*processor).add(tid, thread, current_proc.pid); }
            tid.get_usize() as _
        }

        /// gettid：获取当前线程 TID
        fn gettid(&self, _caller: Caller) -> isize {
            PROCESSOR.get_mut().current().unwrap().tid.get_usize() as _
        }

        /// waittid：等待指定线程退出
        fn waittid(&self, _caller: Caller, tid: usize) -> isize {
            let processor: *mut ProcessorInner = PROCESSOR.get_mut() as *mut ProcessorInner;
            let current_thread = unsafe { (*processor).current().unwrap() };
            if tid == current_thread.tid.get_usize() { return -1; }
            if let Some(exit_code) = unsafe { (*processor).waittid(ThreadId::from_usize(tid)) } {
                exit_code
            } else { -1 }
        }
    }

    /// 同步原语系统调用（**本章新增**）
    ///
    /// 实现 Mutex、Semaphore、Condvar 的创建和操作。
    /// 这些同步原语存储在 Process 的列表中，由所有线程共享。
    impl SyncMutex for SyscallContext {
        /// 创建信号量（初始计数 = res_count）
        fn semaphore_create(&self, _caller: Caller, res_count: usize) -> isize {
            let current_proc = PROCESSOR.get_mut().get_current_proc().unwrap();
            let id = if let Some(id) = current_proc.semaphore_list.iter().enumerate()
                .find(|(_, item)| item.is_none()).map(|(id, _)| id)
            {
                current_proc.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
                id
            } else {
                current_proc.semaphore_list.push(Some(Arc::new(Semaphore::new(res_count))));
                current_proc.semaphore_list.len() - 1
            };
            current_proc.deadlock.register_semaphore(res_count);
            id as isize
        }

        /// V 操作：释放信号量，唤醒等待线程
        fn semaphore_up(&self, _caller: Caller, sem_id: usize) -> isize {
            let processor: *mut ProcessorInner = PROCESSOR.get_mut() as *mut ProcessorInner;
            let tid = unsafe { (*processor).current().unwrap().tid };
            let pid = unsafe { (*processor).get_current_proc().unwrap().pid };
            let waking_tid = {
                let current_proc = unsafe { (*processor).get_proc(pid).unwrap() };
                if sem_id >= current_proc.semaphore_list.len()
                    || current_proc.semaphore_list[sem_id].is_none()
                {
                    return -1;
                }
                let sem = Arc::clone(current_proc.semaphore_list[sem_id].as_ref().unwrap());
                let waking_tid = sem.up();
                current_proc
                    .deadlock
                    .semaphore_release(tid, sem_id, waking_tid);
                waking_tid
            };
            if let Some(tid) = waking_tid {
                unsafe { (*processor).re_enque(tid); }
            }
            0
        }

        /// P 操作：获取信号量，不可用则阻塞
        fn semaphore_down(&self, _caller: Caller, sem_id: usize) -> isize {
            let processor: *mut ProcessorInner = PROCESSOR.get_mut() as *mut ProcessorInner;
            let tid = unsafe { (*processor).current().unwrap().tid };
            let pid = unsafe { (*processor).get_current_proc().unwrap().pid };
            let active_threads = unsafe { (*processor).get_thread(pid).unwrap().clone() };
            let current_proc = unsafe { (*processor).get_proc(pid).unwrap() };
            if sem_id >= current_proc.semaphore_list.len()
                || current_proc.semaphore_list[sem_id].is_none()
            {
                return -1;
            }
            let sem = Arc::clone(current_proc.semaphore_list[sem_id].as_ref().unwrap());
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
        }

        /// 创建互斥锁（blocking=true 为阻塞锁）
        fn mutex_create(&self, _caller: Caller, blocking: bool) -> isize {
            let new_mutex: Option<Arc<dyn MutexTrait>> = if blocking {
                Some(Arc::new(MutexBlocking::new()))
            } else { None };
            let current_proc = PROCESSOR.get_mut().get_current_proc().unwrap();
            if let Some(id) = current_proc.mutex_list.iter().enumerate()
                .find(|(_, item)| item.is_none()).map(|(id, _)| id)
            {
                current_proc.mutex_list[id] = new_mutex;
                current_proc.deadlock.register_mutex();
                id as isize
            } else {
                current_proc.mutex_list.push(new_mutex);
                current_proc.deadlock.register_mutex();
                current_proc.mutex_list.len() as isize - 1
            }
        }

        /// 解锁，唤醒等待线程
        fn mutex_unlock(&self, _caller: Caller, mutex_id: usize) -> isize {
            let processor: *mut ProcessorInner = PROCESSOR.get_mut() as *mut ProcessorInner;
            let tid = unsafe { (*processor).current().unwrap().tid };
            let pid = unsafe { (*processor).get_current_proc().unwrap().pid };
            let waking_tid = {
                let current_proc = unsafe { (*processor).get_proc(pid).unwrap() };
                if mutex_id >= current_proc.mutex_list.len() || current_proc.mutex_list[mutex_id].is_none() {
                    return -1;
                }
                let mutex = Arc::clone(current_proc.mutex_list[mutex_id].as_ref().unwrap());
                let waking_tid = mutex.unlock();
                current_proc.deadlock.mutex_release(tid, mutex_id, waking_tid);
                waking_tid
            };
            if let Some(tid) = waking_tid {
                unsafe { (*processor).re_enque(tid); }
            }
            0
        }

        /// 加锁，已被占用则阻塞
        fn mutex_lock(&self, _caller: Caller, mutex_id: usize) -> isize {
            let processor: *mut ProcessorInner = PROCESSOR.get_mut() as *mut ProcessorInner;
            let tid = unsafe { (*processor).current().unwrap().tid };
            let pid = unsafe { (*processor).get_current_proc().unwrap().pid };
            let current_proc = unsafe { (*processor).get_proc(pid).unwrap() };
            if mutex_id >= current_proc.mutex_list.len() || current_proc.mutex_list[mutex_id].is_none() {
                return -1;
            }
            let mutex = Arc::clone(current_proc.mutex_list[mutex_id].as_ref().unwrap());
            if current_proc.deadlock.enabled && current_proc.deadlock.mutex_would_deadlock(tid, mutex_id) {
                return DEADLOCK_DETECTED;
            }
            if !mutex.lock(tid) {
                current_proc.deadlock.mutex_wait(tid, mutex_id);
                -1
            } else {
                current_proc.deadlock.mutex_acquired(tid, mutex_id);
                0
            }
        }

        /// 创建条件变量
        fn condvar_create(&self, _caller: Caller, _arg: usize) -> isize {
            let current_proc = PROCESSOR.get_mut().get_current_proc().unwrap();
            let id = if let Some(id) = current_proc.condvar_list.iter().enumerate()
                .find(|(_, item)| item.is_none()).map(|(id, _)| id)
            {
                current_proc.condvar_list[id] = Some(Arc::new(Condvar::new()));
                id
            } else {
                current_proc.condvar_list.push(Some(Arc::new(Condvar::new())));
                current_proc.condvar_list.len() - 1
            };
            id as isize
        }

        /// 唤醒一个等待线程
        fn condvar_signal(&self, _caller: Caller, condvar_id: usize) -> isize {
            let processor: *mut ProcessorInner = PROCESSOR.get_mut() as *mut ProcessorInner;
            let current_proc = unsafe { (*processor).get_current_proc().unwrap() };
            let condvar = Arc::clone(current_proc.condvar_list[condvar_id].as_ref().unwrap());
            if let Some(tid) = condvar.signal() {
                unsafe { (*processor).re_enque(tid); }
            }
            0
        }

        /// 等待条件变量（释放锁 + 阻塞 + 重新获取锁）
        fn condvar_wait(&self, _caller: Caller, condvar_id: usize, mutex_id: usize) -> isize {
            let processor: *mut ProcessorInner = PROCESSOR.get_mut() as *mut ProcessorInner;
            let tid = unsafe { (*processor).current().unwrap().tid };
            let pid = unsafe { (*processor).get_current_proc().unwrap().pid };
            let current_proc = unsafe { (*processor).get_proc(pid).unwrap() };
            let condvar = Arc::clone(current_proc.condvar_list[condvar_id].as_ref().unwrap());
            let mutex = Arc::clone(current_proc.mutex_list[mutex_id].as_ref().unwrap());
            let (flag, waking_tid) = condvar.wait_with_mutex(tid, mutex);
            current_proc
                .deadlock
                .mutex_release(tid, mutex_id, waking_tid);
            if flag {
                current_proc.deadlock.mutex_acquired(tid, mutex_id);
            } else {
                current_proc.deadlock.mutex_wait(tid, mutex_id);
            }
            if let Some(waking_tid) = waking_tid {
                unsafe { (*processor).re_enque(waking_tid); }
            }
            if !flag { -1 } else { 0 }
        }

        /// 开启或关闭死锁检测。
        fn enable_deadlock_detect(&self, _caller: Caller, is_enable: i32) -> isize {
            let current_proc = PROCESSOR.get_mut().get_current_proc().unwrap();
            match is_enable {
                0 => {
                    current_proc.deadlock.enabled = false;
                    0
                }
                1 => {
                    current_proc.deadlock.enabled = true;
                    0
                }
                _ => -1,
            }
        }
    }
}

/// 非 RISC-V64 架构的占位实现
#[cfg(not(target_arch = "riscv64"))]
mod stub {
    use tg_kernel_vm::page_table::{MmuMeta, VmFlags};

    /// Sv39 占位类型
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
    pub struct Sv39;
    impl MmuMeta for Sv39 {
        const P_ADDR_BITS: usize = 56;
        const PAGE_BITS: usize = 12;
        const LEVEL_BITS: &'static [usize] = &[9, 9, 9];
        const PPN_POS: usize = 10;
        #[inline]
        fn is_leaf(value: usize) -> bool { value & 0b1110 != 0 }
    }
    /// 构建 VmFlags 占位
    pub const fn build_flags(_s: &str) -> VmFlags<Sv39> { unsafe { VmFlags::from_raw(0) } }
    /// 解析 VmFlags 占位
    pub fn parse_flags(_s: &str) -> Result<VmFlags<Sv39>, ()> { Ok(unsafe { VmFlags::from_raw(0) }) }

    #[unsafe(no_mangle)]
    pub extern "C" fn main() -> i32 { 0 }
    #[unsafe(no_mangle)]
    pub extern "C" fn __libc_start_main() -> i32 { 0 }
    #[unsafe(no_mangle)]
    pub extern "C" fn rust_eh_personality() {}
}
