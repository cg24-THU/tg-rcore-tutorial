#![cfg_attr(target_arch = "riscv64", no_std)]
#![cfg_attr(target_arch = "riscv64", no_main)]
#![cfg_attr(not(target_arch = "riscv64"), allow(dead_code))]

#[cfg(target_arch = "riscv64")]
extern crate alloc;

#[cfg(target_arch = "riscv64")]
use core::{
    arch::{asm, global_asm},
    cell::UnsafeCell,
    mem::MaybeUninit,
    ptr::NonNull,
    sync::atomic::{AtomicBool, AtomicU16, Ordering},
};
#[cfg(target_arch = "riscv64")]
use riscv::register::{
    scause::{self, Interrupt, Trap},
    sepc, sie, sstatus, sscratch,
    stvec::{self, TrapMode},
};
#[cfg(target_arch = "riscv64")]
use tg_sbi::shutdown;
#[cfg(target_arch = "riscv64")]
use virtio_drivers::{BlkResp, DeviceType, MmioTransport, RespStatus, Transport, VirtIOBlk, VirtIOHeader};

#[cfg(target_arch = "riscv64")]
#[global_allocator]
static GLOBAL_ALLOCATOR: memory::KernelAllocator = memory::KernelAllocator;

#[cfg(target_arch = "riscv64")]
const VIRTIO0: usize = 0x1000_1000;
#[cfg(target_arch = "riscv64")]
const VIRTIO0_IRQ: u32 = 1;
#[cfg(target_arch = "riscv64")]
const TEST_BLOCK: usize = 1;
#[cfg(target_arch = "riscv64")]
const BLOCK_SIZE: usize = 512;

#[cfg(target_arch = "riscv64")]
type VirtioBlockDevice = VirtIOBlk<memory::VirtioHal, MmioTransport>;

#[cfg(target_arch = "riscv64")]
static BLOCK_DEVICE: StaticCell<Option<VirtioBlockDevice>> = StaticCell::new(None);
#[cfg(target_arch = "riscv64")]
static WRITE_BUF: StaticCell<[u8; BLOCK_SIZE]> = StaticCell::new([0; BLOCK_SIZE]);
#[cfg(target_arch = "riscv64")]
static READ_BUF: StaticCell<[u8; BLOCK_SIZE]> = StaticCell::new([0; BLOCK_SIZE]);
#[cfg(target_arch = "riscv64")]
static RESPONSE: StaticCell<MaybeUninit<BlkResp>> = StaticCell::new(MaybeUninit::uninit());
#[cfg(target_arch = "riscv64")]
static REQUEST_DONE: AtomicBool = AtomicBool::new(false);
#[cfg(target_arch = "riscv64")]
static REQUEST_TOKEN: AtomicU16 = AtomicU16::new(0);

#[cfg(target_arch = "riscv64")]
global_asm!(
    r#"
    .section .text.trap, "ax"
    .globl __trap_vector
    .align 2
__trap_vector:
    csrrw sp, sscratch, sp
    addi sp, sp, -264
    sd ra, 0*8(sp)
    sd gp, 1*8(sp)
    sd tp, 2*8(sp)
    sd t0, 3*8(sp)
    sd t1, 4*8(sp)
    sd t2, 5*8(sp)
    sd s0, 6*8(sp)
    sd s1, 7*8(sp)
    sd a0, 8*8(sp)
    sd a1, 9*8(sp)
    sd a2, 10*8(sp)
    sd a3, 11*8(sp)
    sd a4, 12*8(sp)
    sd a5, 13*8(sp)
    sd a6, 14*8(sp)
    sd a7, 15*8(sp)
    sd s2, 16*8(sp)
    sd s3, 17*8(sp)
    sd s4, 18*8(sp)
    sd s5, 19*8(sp)
    sd s6, 20*8(sp)
    sd s7, 21*8(sp)
    sd s8, 22*8(sp)
    sd s9, 23*8(sp)
    sd s10, 24*8(sp)
    sd s11, 25*8(sp)
    sd t3, 26*8(sp)
    sd t4, 27*8(sp)
    sd t5, 28*8(sp)
    sd t6, 29*8(sp)
    csrr t0, sstatus
    csrr t1, sepc
    csrr t2, sscratch
    sd t0, 30*8(sp)
    sd t1, 31*8(sp)
    sd t2, 32*8(sp)
    call rust_trap_handler
    ld t0, 30*8(sp)
    ld t1, 31*8(sp)
    csrw sstatus, t0
    csrw sepc, t1
    addi t0, sp, 264
    csrw sscratch, t0
    ld ra, 0*8(sp)
    ld gp, 1*8(sp)
    ld tp, 2*8(sp)
    ld t0, 3*8(sp)
    ld t1, 4*8(sp)
    ld t2, 5*8(sp)
    ld s0, 6*8(sp)
    ld s1, 7*8(sp)
    ld a0, 8*8(sp)
    ld a1, 9*8(sp)
    ld a2, 10*8(sp)
    ld a3, 11*8(sp)
    ld a4, 12*8(sp)
    ld a5, 13*8(sp)
    ld a6, 14*8(sp)
    ld a7, 15*8(sp)
    ld s2, 16*8(sp)
    ld s3, 17*8(sp)
    ld s4, 18*8(sp)
    ld s5, 19*8(sp)
    ld s6, 20*8(sp)
    ld s7, 21*8(sp)
    ld s8, 22*8(sp)
    ld s9, 23*8(sp)
    ld s10, 24*8(sp)
    ld s11, 25*8(sp)
    ld t3, 26*8(sp)
    ld t4, 27*8(sp)
    ld t5, 28*8(sp)
    ld t6, 29*8(sp)
    ld sp, 32*8(sp)
    sret

    .section .bss.uninit
    .align 12
    .globl __trap_stack
__trap_stack:
    .space 4096
    .globl __trap_stack_top
__trap_stack_top:
"#
);

#[cfg(target_arch = "riscv64")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
unsafe extern "C" fn _start() -> ! {
    const STACK_SIZE: usize = 4096;

    #[unsafe(link_section = ".bss.uninit")]
    static mut STACK: [u8; STACK_SIZE] = [0u8; STACK_SIZE];

    core::arch::naked_asm!(
        "la sp, {stack} + {stack_size}",
        "j  {main}",
        stack_size = const STACK_SIZE,
        stack = sym STACK,
        main = sym rust_main,
    )
}

#[cfg(target_arch = "riscv64")]
extern "C" fn rust_main() -> ! {
    console::puts("booting tg-rcore-tutorial-storage\n");
    unsafe {
        init_trap();
    }
    plic::init();
    init_block_device();
    fill_write_buffer();
    console::puts("submit write request\n");
    submit_write(TEST_BLOCK);
    wait_for_completion();
    ensure_success("write");
    clear_read_buffer();
    console::puts("submit read request\n");
    submit_read(TEST_BLOCK);
    wait_for_completion();
    ensure_success("read");
    verify_buffers();
    console::puts("interrupt-driven storage check passed\n");
    shutdown(false)
}

#[cfg(target_arch = "riscv64")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    console::puts("panic\n");
    shutdown(true)
}

#[cfg(target_arch = "riscv64")]
#[unsafe(no_mangle)]
extern "C" fn rust_trap_handler() {
    match scause::read().cause() {
        Trap::Interrupt(Interrupt::SupervisorExternal) => handle_external_interrupt(),
        _ => {
            console::puts("unexpected trap\n");
            console::puts("sepc=");
            console::put_hex(sepc::read());
            console::puts("\n");
            shutdown(true);
        }
    }
}

#[cfg(target_arch = "riscv64")]
unsafe fn init_trap() {
    unsafe extern "C" {
        fn __trap_vector();
        static __trap_stack_top: u8;
    }

    unsafe {
        sscratch::write((&raw const __trap_stack_top) as usize);
        stvec::write(__trap_vector as *const () as usize, TrapMode::Direct);
        sie::set_sext();
        sstatus::set_sie();
    }
}

#[cfg(target_arch = "riscv64")]
fn init_block_device() {
    let header = NonNull::new(VIRTIO0 as *mut VirtIOHeader).unwrap();
    let transport = unsafe { MmioTransport::new(header) }.expect("failed to create transport");
    if transport.device_type() != DeviceType::Block {
        console::puts("virtio block device not found\n");
        shutdown(true);
    }
    let device = VirtIOBlk::<memory::VirtioHal, MmioTransport>::new(transport)
        .expect("failed to create block device");
    unsafe {
        *BLOCK_DEVICE.get() = Some(device);
    }
}

#[cfg(target_arch = "riscv64")]
fn fill_write_buffer() {
    let buf = unsafe { &mut *WRITE_BUF.get() };
    buf.fill(0);
    let banner = b"tg-storage interrupt path";
    buf[..banner.len()].copy_from_slice(banner);
    for (index, byte) in buf.iter_mut().enumerate().skip(banner.len()) {
        *byte = (index & 0xff) as u8;
    }
}

#[cfg(target_arch = "riscv64")]
fn clear_read_buffer() {
    unsafe {
        (*READ_BUF.get()).fill(0);
    }
}

#[cfg(target_arch = "riscv64")]
fn submit_write(block_id: usize) {
    REQUEST_DONE.store(false, Ordering::SeqCst);
    unsafe {
        (*RESPONSE.get()).write(BlkResp::default());
    }
    with_block_device(|device| {
        let token = unsafe {
            device.write_block_nb(
                block_id,
                &*WRITE_BUF.get(),
                (*RESPONSE.get()).assume_init_mut(),
            )
        }
            .expect("failed to submit write request");
        REQUEST_TOKEN.store(token, Ordering::SeqCst);
    });
}

#[cfg(target_arch = "riscv64")]
fn submit_read(block_id: usize) {
    REQUEST_DONE.store(false, Ordering::SeqCst);
    unsafe {
        (*RESPONSE.get()).write(BlkResp::default());
    }
    with_block_device(|device| {
        let token = unsafe {
            device.read_block_nb(
                block_id,
                &mut *READ_BUF.get(),
                (*RESPONSE.get()).assume_init_mut(),
            )
        }
            .expect("failed to submit read request");
        REQUEST_TOKEN.store(token, Ordering::SeqCst);
    });
}

#[cfg(target_arch = "riscv64")]
fn wait_for_completion() {
    while !REQUEST_DONE.load(Ordering::SeqCst) {
        unsafe {
            asm!("wfi");
        }
    }
}

#[cfg(target_arch = "riscv64")]
fn ensure_success(stage: &str) {
    let status = unsafe { (*RESPONSE.get()).assume_init_ref().status() };
    if status != RespStatus::Ok {
        console::puts(stage);
        console::puts(" failed\n");
        shutdown(true);
    }
}

#[cfg(target_arch = "riscv64")]
fn verify_buffers() {
    let write_buf = unsafe { &*WRITE_BUF.get() };
    let read_buf = unsafe { &*READ_BUF.get() };
    if write_buf != read_buf {
        console::puts("buffer mismatch\n");
        shutdown(true);
    }
}

#[cfg(target_arch = "riscv64")]
fn handle_external_interrupt() {
    let irq = plic::claim();
    if irq == VIRTIO0_IRQ {
        with_block_device(|device| {
            if device.ack_interrupt() {
                while let Ok(token) = device.pop_used() {
                    if token == REQUEST_TOKEN.load(Ordering::SeqCst) {
                        REQUEST_DONE.store(true, Ordering::SeqCst);
                    }
                }
            }
        });
    }
    if irq != 0 {
        plic::complete(irq);
    }
}

#[cfg(target_arch = "riscv64")]
fn with_block_device<R>(f: impl FnOnce(&mut VirtioBlockDevice) -> R) -> R {
    let sie_enabled = sstatus::read().sie();
    unsafe {
        sstatus::clear_sie();
    }
    let result = unsafe { f((*BLOCK_DEVICE.get()).as_mut().unwrap()) };
    if sie_enabled {
        unsafe {
            sstatus::set_sie();
        }
    }
    result
}

#[cfg(target_arch = "riscv64")]
struct StaticCell<T>(UnsafeCell<T>);

#[cfg(target_arch = "riscv64")]
unsafe impl<T> Sync for StaticCell<T> {}

#[cfg(target_arch = "riscv64")]
impl<T> StaticCell<T> {
    const fn new(value: T) -> Self {
        Self(UnsafeCell::new(value))
    }

    fn get(&self) -> *mut T {
        self.0.get()
    }
}

#[cfg(target_arch = "riscv64")]
mod plic {
    use crate::VIRTIO0_IRQ;
    use core::ptr::{read_volatile, write_volatile};

    const PLIC_BASE: usize = 0x0c00_0000;
    const PLIC_PRIORITY: usize = PLIC_BASE;
    const PLIC_ENABLE: usize = PLIC_BASE + 0x2000;
    const PLIC_CONTEXT: usize = PLIC_BASE + 0x20_0000;
    const SUPERVISOR_CONTEXT: usize = 1;

    pub fn init() {
        unsafe {
            write_volatile((PLIC_PRIORITY + VIRTIO0_IRQ as usize * 4) as *mut u32, 1);
            let enable_ptr =
                (PLIC_ENABLE + SUPERVISOR_CONTEXT * 0x80 + ((VIRTIO0_IRQ as usize / 32) * 4))
                    as *mut u32;
            let value = read_volatile(enable_ptr);
            write_volatile(enable_ptr, value | (1 << (VIRTIO0_IRQ % 32)));
            write_volatile((PLIC_CONTEXT + SUPERVISOR_CONTEXT * 0x1000) as *mut u32, 0);
        }
    }

    pub fn claim() -> u32 {
        unsafe { read_volatile((PLIC_CONTEXT + SUPERVISOR_CONTEXT * 0x1000 + 4) as *const u32) }
    }

    pub fn complete(irq: u32) {
        unsafe {
            write_volatile(
                (PLIC_CONTEXT + SUPERVISOR_CONTEXT * 0x1000 + 4) as *mut u32,
                irq,
            );
        }
    }
}

#[cfg(target_arch = "riscv64")]
mod console {
    use tg_sbi::console_putchar;

    pub fn puts(text: &str) {
        for byte in text.bytes() {
            console_putchar(byte);
        }
    }

    pub fn put_hex(mut value: usize) {
        puts("0x");
        let mut digits = [0u8; 16];
        for index in (0..16).rev() {
            digits[index] = (value & 0xf) as u8;
            value >>= 4;
        }
        for digit in digits {
            let ch = match digit {
                0..=9 => b'0' + digit,
                _ => b'a' + (digit - 10),
            };
            console_putchar(ch);
        }
    }
}

#[cfg(target_arch = "riscv64")]
mod memory {
    use core::{
        alloc::{GlobalAlloc, Layout},
        ptr::addr_of_mut,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use virtio_drivers::{Hal, PhysAddr, VirtAddr};

    const PAGE_SIZE: usize = 4096;
    const HEAP_SIZE: usize = 256 * 1024;
    const DMA_SIZE: usize = 4 * 1024 * 1024;

    #[allow(dead_code)]
    #[repr(align(16))]
    struct HeapSpace([u8; HEAP_SIZE]);

    #[allow(dead_code)]
    #[repr(align(4096))]
    struct DmaSpace([u8; DMA_SIZE]);

    static mut HEAP_SPACE: HeapSpace = HeapSpace([0; HEAP_SIZE]);
    static mut DMA_SPACE: DmaSpace = DmaSpace([0; DMA_SIZE]);

    static HEAP_NEXT: AtomicUsize = AtomicUsize::new(0);
    static DMA_NEXT: AtomicUsize = AtomicUsize::new(0);

    pub struct KernelAllocator;

    fn align_up(value: usize, align: usize) -> usize {
        (value + align - 1) & !(align - 1)
    }

    fn reserve(
        base: usize,
        capacity: usize,
        next: &AtomicUsize,
        size: usize,
        align: usize,
    ) -> usize {
        let size = size.max(1);
        loop {
            let current = next.load(Ordering::Relaxed);
            let Some(start) = base.checked_add(current) else {
                return 0;
            };
            let start = align_up(start, align);
            let offset = start - base;
            let Some(end) = offset.checked_add(size) else {
                return 0;
            };
            if end > capacity {
                return 0;
            }
            if next
                .compare_exchange(current, end, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                return start;
            }
        }
    }

    fn heap_base() -> usize {
        addr_of_mut!(HEAP_SPACE) as usize
    }

    fn dma_base() -> usize {
        addr_of_mut!(DMA_SPACE) as usize
    }

    unsafe impl GlobalAlloc for KernelAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            reserve(
                heap_base(),
                HEAP_SIZE,
                &HEAP_NEXT,
                layout.size(),
                layout.align(),
            ) as *mut u8
        }

        unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
    }

    pub struct VirtioHal;

    impl Hal for VirtioHal {
        fn dma_alloc(pages: usize) -> PhysAddr {
            reserve(
                dma_base(),
                DMA_SIZE,
                &DMA_NEXT,
                pages * PAGE_SIZE,
                PAGE_SIZE,
            )
        }

        fn dma_dealloc(_paddr: PhysAddr, _pages: usize) -> i32 {
            0
        }

        fn phys_to_virt(paddr: PhysAddr) -> VirtAddr {
            paddr
        }

        fn virt_to_phys(vaddr: VirtAddr) -> PhysAddr {
            vaddr
        }
    }
}

#[cfg(not(target_arch = "riscv64"))]
fn main() {}
