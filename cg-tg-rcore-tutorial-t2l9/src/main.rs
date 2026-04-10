//! # 第二章 SMP 扩展：主从核分离的批处理系统
//!
//! 设计目标：
//! - 多个 hart 可以同时从 `_start` 进入 S 态；
//! - 只有主核负责清空 `.bss`、初始化全局状态并执行用户程序；
//! - 从核在主核完成初始化后输出一条待机信息并进入 `wfi`。

#![no_std]
#![no_main]
#![cfg_attr(target_arch = "riscv64", deny(warnings, missing_docs))]
#![cfg_attr(not(target_arch = "riscv64"), allow(dead_code))]

#[macro_use]
extern crate tg_console;

use core::{
    cell::UnsafeCell,
    fmt::{self, Write},
    hint::spin_loop,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use impls::{Console, SyscallContext};
use riscv::register::*;
use tg_console::log;
use tg_kernel_context::LocalContext;
use tg_syscall::{Caller, SyscallId};

const BOOT_HART_ID: usize = 0;
const HART_COUNT: usize = 4;
const MAX_HARTS: usize = 8;
const STACK_SIZE: usize = 8 * 4096;
const PRIMARY_WAITING: usize = 0x5A5A_0001;
const PRIMARY_READY: usize = 0x5A5A_0002;

static PRIMARY_STATE: AtomicUsize = AtomicUsize::new(PRIMARY_WAITING);
static SECONDARY_PARKED: AtomicUsize = AtomicUsize::new(0);
static CONSOLE_LOCK: SpinLock<()> = SpinLock::new(());

#[cfg(target_arch = "riscv64")]
core::arch::global_asm!(include_str!(env!("APP_ASM")));

#[cfg(target_arch = "riscv64")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
unsafe extern "C" fn _start() -> ! {
    #[unsafe(link_section = ".boot.stack")]
    static mut STACKS: [u8; STACK_SIZE * MAX_HARTS] = [0u8; STACK_SIZE * MAX_HARTS];

    core::arch::naked_asm!(
        "mv tp, a0",
        "li t0, {max_harts}",
        "bgeu a0, t0, 2f",
        "la sp, {stack}",
        "li t0, {stack_size}",
        "mul t1, a0, t0",
        "add sp, sp, t1",
        "add sp, sp, t0",
        "j {main}",
        "2:",
        "wfi",
        "j 2b",
        max_harts = const MAX_HARTS,
        stack = sym STACKS,
        stack_size = const STACK_SIZE,
        main = sym rust_main,
    )
}

extern "C" fn rust_main(hart_id: usize) -> ! {
    if hart_id == BOOT_HART_ID {
        primary_rust_main()
    } else {
        secondary_rust_main(hart_id)
    }
}

fn primary_rust_main() -> ! {
    unsafe { tg_linker::KernelLayout::locate().zero_bss() };

    tg_console::init_console(&Console);
    tg_console::set_log_level(option_env!("LOG"));
    tg_console::test_log();

    tg_syscall::init_io(&SyscallContext);
    tg_syscall::init_process(&SyscallContext);

    PRIMARY_STATE.store(PRIMARY_READY, Ordering::Release);

    while SECONDARY_PARKED.load(Ordering::Acquire) < HART_COUNT - 1 {
        spin_loop();
    }
    log::info!("all secondary harts are parked; boot hart continues with apps");

    run_apps();
    tg_sbi::shutdown(false)
}

fn secondary_rust_main(hart_id: usize) -> ! {
    while PRIMARY_STATE.load(Ordering::Acquire) != PRIMARY_READY {
        spin_loop();
    }

    announce_secondary_hart(hart_id);
    SECONDARY_PARKED.fetch_add(1, Ordering::SeqCst);
    park()
}

fn announce_secondary_hart(hart_id: usize) {
    let _guard = CONSOLE_LOCK.lock();
    let mut writer = RawConsole;
    writeln!(
        &mut writer,
        "hart {hart_id} is online but parked before batch execution"
    )
    .unwrap();
}

fn park() -> ! {
    loop {
        unsafe {
            core::arch::asm!("wfi");
        }
    }
}

fn run_apps() {
    for (i, app) in tg_linker::AppMeta::locate().iter().enumerate() {
        let app_base = app.as_ptr() as usize;
        log::info!("load app{i} to {app_base:#x}");

        let mut ctx = LocalContext::user(app_base);
        let mut user_stack: core::mem::MaybeUninit<[usize; 512]> = core::mem::MaybeUninit::uninit();
        let user_stack_ptr = user_stack.as_mut_ptr() as *mut usize;
        *ctx.sp_mut() = unsafe { user_stack_ptr.add(512) } as usize;

        loop {
            unsafe { ctx.execute() };

            use scause::{Exception, Trap};
            match scause::read().cause() {
                Trap::Exception(Exception::UserEnvCall) => {
                    use SyscallResult::*;
                    match handle_syscall(&mut ctx) {
                        Done => continue,
                        Exit(code) => log::info!("app{i} exit with code {code}"),
                        Error(id) => {
                            log::error!("app{i} call an unsupported syscall {}", id.0)
                        }
                    }
                }
                trap => log::error!("app{i} was killed because of {trap:?}"),
            }
            unsafe { core::arch::asm!("fence.i") };
            break;
        }

        let _ = core::hint::black_box(&user_stack);
        println!();
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let _guard = CONSOLE_LOCK.lock();
    let mut writer = RawConsole;
    writeln!(&mut writer, "{info}").unwrap();
    tg_sbi::shutdown(true)
}

enum SyscallResult {
    Done,
    Exit(usize),
    Error(SyscallId),
}

fn handle_syscall(ctx: &mut LocalContext) -> SyscallResult {
    use tg_syscall::{SyscallId as Id, SyscallResult as Ret};

    let id = ctx.a(7).into();
    let args = [ctx.a(0), ctx.a(1), ctx.a(2), ctx.a(3), ctx.a(4), ctx.a(5)];

    match tg_syscall::handle(Caller { entity: 0, flow: 0 }, id, args) {
        Ret::Done(ret) => match id {
            Id::EXIT => SyscallResult::Exit(ctx.a(0)),
            _ => {
                *ctx.a_mut(0) = ret as _;
                ctx.move_next();
                SyscallResult::Done
            }
        },
        Ret::Unsupported(id) => SyscallResult::Error(id),
    }
}

struct RawConsole;

impl Write for RawConsole {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            tg_sbi::console_putchar(byte);
        }
        Ok(())
    }
}

struct SpinLock<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for SpinLock<T> {}

impl<T> SpinLock<T> {
    const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    fn lock(&self) -> SpinLockGuard<'_, T> {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            while self.locked.load(Ordering::Relaxed) {
                spin_loop();
            }
        }
        SpinLockGuard { lock: self }
    }
}

struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

mod impls {
    use super::CONSOLE_LOCK;
    use tg_syscall::{STDDEBUG, STDOUT};

    pub struct Console;

    impl tg_console::Console for Console {
        fn put_char(&self, c: u8) {
            let _guard = CONSOLE_LOCK.lock();
            tg_sbi::console_putchar(c);
        }

        fn put_str(&self, s: &str) {
            let _guard = CONSOLE_LOCK.lock();
            for c in s.bytes() {
                tg_sbi::console_putchar(c);
            }
        }
    }

    pub struct SyscallContext;

    impl tg_syscall::IO for SyscallContext {
        fn write(&self, _caller: tg_syscall::Caller, fd: usize, buf: usize, count: usize) -> isize {
            match fd {
                STDOUT | STDDEBUG => {
                    print!("{}", unsafe {
                        core::str::from_utf8_unchecked(core::slice::from_raw_parts(
                            buf as *const u8,
                            count,
                        ))
                    });
                    count as _
                }
                _ => {
                    tg_console::log::error!("unsupported fd: {fd}");
                    -1
                }
            }
        }
    }

    impl tg_syscall::Process for SyscallContext {
        fn exit(&self, _caller: tg_syscall::Caller, _status: usize) -> isize {
            0
        }
    }
}

#[cfg(not(target_arch = "riscv64"))]
mod stub {
    #[unsafe(no_mangle)]
    pub extern "C" fn main() -> i32 {
        0
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn __libc_start_main() -> i32 {
        0
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn rust_eh_personality() {}
}
