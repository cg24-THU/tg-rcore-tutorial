//! # 第一章 SMP 扩展：多核最小执行环境
//!
//! 本章在原始 ch1 的基础上扩展为 4 核启动：
//! - 每个 hart 在 S 态拥有独立启动栈；
//! - 默认通过自旋锁串行化整行输出；
//! - 启用 `no-lock-demo` feature 时故意关闭锁，便于观察字符交错。

#![no_std]
#![no_main]
#![cfg_attr(target_arch = "riscv64", deny(warnings, missing_docs))]
#![cfg_attr(not(target_arch = "riscv64"), allow(dead_code))]

use core::{
    fmt::{self, Write},
    hint::spin_loop,
    sync::atomic::{AtomicUsize, Ordering},
};
#[cfg(not(feature = "no-lock-demo"))]
use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::atomic::AtomicBool,
};
use tg_sbi::{console_putchar, shutdown};

const BOOT_HART_ID: usize = 0;
const HART_COUNT: usize = 4;
const MAX_HARTS: usize = 8;
const STACK_SIZE: usize = 4096;

#[cfg(not(feature = "no-lock-demo"))]
static PRINT_LOCK: SpinLock<()> = SpinLock::new(());
static PRINTED_HARTS: AtomicUsize = AtomicUsize::new(0);

#[cfg(target_arch = "riscv64")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
unsafe extern "C" fn _start() -> ! {
    #[unsafe(link_section = ".bss.uninit")]
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
    print_hello(hart_id);
    PRINTED_HARTS.fetch_add(1, Ordering::SeqCst);

    if hart_id == BOOT_HART_ID {
        while PRINTED_HARTS.load(Ordering::Acquire) < HART_COUNT {
            spin_loop();
        }
        shutdown(false)
    } else {
        park()
    }
}

fn print_hello(hart_id: usize) {
    #[cfg(not(feature = "no-lock-demo"))]
    {
        let _guard = PRINT_LOCK.lock();
        let mut writer = RawConsole;
        writeln!(&mut writer, "Hello from hart {hart_id}").unwrap();
    }

    #[cfg(feature = "no-lock-demo")]
    {
        let mut writer = RawConsole;
        writeln!(&mut writer, "Hello from hart {hart_id}").unwrap();
    }
}

fn park() -> ! {
    loop {
        // 这里没有启用中断，wfi 主要作为教学上的“从核待机”表示。
        unsafe {
            core::arch::asm!("wfi");
        }
    }
}

struct RawConsole;

impl Write for RawConsole {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            console_putchar(byte);
            #[cfg(feature = "no-lock-demo")]
            for _ in 0..512 {
                spin_loop();
            }
        }
        Ok(())
    }
}

#[cfg(not(feature = "no-lock-demo"))]
struct SpinLock<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

#[cfg(not(feature = "no-lock-demo"))]
unsafe impl<T: Send> Sync for SpinLock<T> {}

#[cfg(not(feature = "no-lock-demo"))]
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

#[cfg(not(feature = "no-lock-demo"))]
struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
}

#[cfg(not(feature = "no-lock-demo"))]
impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.value.get() }
    }
}

#[cfg(not(feature = "no-lock-demo"))]
impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.value.get() }
    }
}

#[cfg(not(feature = "no-lock-demo"))]
impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    shutdown(true)
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
