//! # 第一章：应用程序与基本执行环境
//!
//! 本章在最小 S 态裸机程序的基础上，增加 VirtIO-GPU framebuffer 输出，
//! 在 QEMU 图形窗口中静态渲染七巧板风格的 “OS” 图案。

// 不使用标准库，因为裸机环境没有操作系统提供系统调用支持
#![cfg_attr(target_arch = "riscv64", no_std)]
// 不使用标准入口，因为裸机环境没有 C runtime 进行初始化
#![cfg_attr(target_arch = "riscv64", no_main)]
// RISC-V64 架构下启用严格警告和文档检查
#![cfg_attr(target_arch = "riscv64", deny(warnings, missing_docs))]
// 非 RISC-V64 架构允许死代码（用于 cargo publish --dry-run 在主机上通过编译）
#![cfg_attr(not(target_arch = "riscv64"), allow(dead_code))]

#[cfg(target_arch = "riscv64")]
use tg_sbi::shutdown;

#[cfg(target_arch = "riscv64")]
use core::ptr::NonNull;
#[cfg(target_arch = "riscv64")]
use virtio_drivers::{DeviceType, MmioTransport, Transport, VirtIOGpu, VirtIOHeader};

/// 裸机全局分配器。
#[cfg(target_arch = "riscv64")]
#[global_allocator]
static GLOBAL_ALLOCATOR: memory::KernelAllocator = memory::KernelAllocator;

/// VirtIO-MMIO 设备起始基地址。
#[cfg(target_arch = "riscv64")]
const VIRTIO_MMIO_START: usize = 0x1000_1000;
/// VirtIO-MMIO 设备结束基地址（含）。
#[cfg(target_arch = "riscv64")]
const VIRTIO_MMIO_END: usize = 0x1000_8000;
/// VirtIO-MMIO 槽位步长。
#[cfg(target_arch = "riscv64")]
const VIRTIO_MMIO_STRIDE: usize = 0x1000;

/// S 态程序入口点。
///
/// 这是一个裸函数（naked function），放置在 `.text.entry` 段，
/// 链接脚本将其安排在地址 `0x80200000`。
///
/// 裸函数不生成函数序言和尾声，因此可以在没有栈的情况下执行。
/// 它完成两件事：
/// 1. 设置栈指针 `sp`，指向栈顶（栈从高地址向低地址增长）
/// 2. 跳转到 Rust 主函数 `rust_main`
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
        stack      =   sym STACK,
        main       =   sym rust_main,
    )
}

/// S 态主函数：初始化 VirtIO-GPU，绘制七巧板风格的 “OS” 图案，并保持画面显示。
#[cfg(target_arch = "riscv64")]
extern "C" fn rust_main() -> ! {
    console::puts("Booting ch1-tangram...\n");

    match render_tangram() {
        Ok((width, height)) => {
            console::puts("VirtIO-GPU framebuffer ready: ");
            console::put_u32(width);
            console::puts("x");
            console::put_u32(height);
            console::puts("\nTangram OS rendered.\n");
        }
        Err(reason) => {
            console::puts("ch1-tangram failed: ");
            console::puts(reason);
            console::puts("\n");
            shutdown(true);
        }
    }

    loop {
        core::hint::spin_loop();
    }
}

/// 初始化 VirtIO-GPU，向 framebuffer 写入七巧板图案，并执行 flush。
#[cfg(target_arch = "riscv64")]
fn render_tangram() -> Result<(u32, u32), &'static str> {
    let transport = find_gpu_transport().ok_or("VirtIO-GPU device not found")?;

    let mut gpu = VirtIOGpu::<memory::VirtioHal, MmioTransport>::new(transport)
        .map_err(|_| "GPU init failed")?;
    let (width, height) = gpu.resolution().map_err(|_| "failed to get resolution")?;

    {
        let framebuffer = gpu
            .setup_framebuffer()
            .map_err(|_| "failed to setup framebuffer")?;
        drawing::render(framebuffer, width as usize, height as usize);
    }

    gpu.flush().map_err(|_| "framebuffer flush failed")?;
    Ok((width, height))
}

/// 扫描 QEMU virt 平台上的 VirtIO-MMIO 槽位，找到 GPU 设备。
#[cfg(target_arch = "riscv64")]
fn find_gpu_transport() -> Option<MmioTransport> {
    let mut addr = VIRTIO_MMIO_START;
    while addr <= VIRTIO_MMIO_END {
        let header = NonNull::new(addr as *mut VirtIOHeader).unwrap();
        if let Ok(transport) = unsafe { MmioTransport::new(header) } {
            if transport.device_type() == DeviceType::GPU {
                return Some(transport);
            }
        }
        addr += VIRTIO_MMIO_STRIDE;
    }
    None
}

/// panic 处理函数。
///
/// `#![no_std]` 环境下必须自行实现。发生 panic 时以异常状态关机。
#[cfg(target_arch = "riscv64")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    console::puts("panic\n");
    shutdown(true)
}

/// 极简串口输出工具。
#[allow(missing_docs)]
mod console {
    use tg_sbi::console_putchar;

    pub fn puts(text: &str) {
        for byte in text.bytes() {
            console_putchar(byte);
        }
    }

    pub fn put_u32(mut value: u32) {
        if value == 0 {
            console_putchar(b'0');
            return;
        }

        let mut digits = [0u8; 10];
        let mut len = 0;
        while value != 0 {
            digits[len] = (value % 10) as u8;
            value /= 10;
            len += 1;
        }

        while len != 0 {
            len -= 1;
            console_putchar(b'0' + digits[len]);
        }
    }
}

/// 最小 bump allocator 和 VirtIO DMA HAL。
#[allow(missing_docs)]
mod memory {
    use core::{
        alloc::{GlobalAlloc, Layout},
        ptr::addr_of_mut,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use virtio_drivers::{Hal, PhysAddr, VirtAddr};

    const PAGE_SIZE: usize = 4096;
    const HEAP_SIZE: usize = 512 * 1024;
    const DMA_SIZE: usize = 16 * 1024 * 1024;

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

/// framebuffer 绘制逻辑和七巧板图案数据。
#[allow(missing_docs)]
mod drawing {
    use core::cmp::{max, min};

    const DESIGN_W: i32 = 960;
    const DESIGN_H: i32 = 540;
    const SCALE_ONE: i32 = 1024;

    #[derive(Clone, Copy)]
    struct Color {
        r: u8,
        g: u8,
        b: u8,
    }

    impl Color {
        const fn rgb(r: u8, g: u8, b: u8) -> Self {
            Self { r, g, b }
        }
    }

    #[derive(Clone, Copy)]
    struct Point {
        x: i32,
        y: i32,
    }

    impl Point {
        const fn new(x: i32, y: i32) -> Self {
            Self { x, y }
        }
    }

    #[derive(Clone, Copy)]
    enum Shape {
        Triangle([Point; 3]),
        Quad([Point; 4]),
    }

    #[derive(Clone, Copy)]
    struct Piece {
        color: Color,
        shape: Shape,
    }

    impl Piece {
        const fn triangle(color: Color, points: [Point; 3]) -> Self {
            Self {
                color,
                shape: Shape::Triangle(points),
            }
        }

        const fn quad(color: Color, points: [Point; 4]) -> Self {
            Self {
                color,
                shape: Shape::Quad(points),
            }
        }
    }

    const BACKGROUND: Color = Color::rgb(15, 23, 42);
    const OUTLINE: Color = Color::rgb(2, 6, 23);

    const PIECES: [Piece; 7] = [
        Piece::triangle(
            Color::rgb(239, 83, 80),
            [p(80, 270), p(220, 130), p(220, 410)],
        ),
        Piece::quad(
            Color::rgb(255, 202, 40),
            [p(220, 130), p(300, 50), p(380, 130), p(300, 210)],
        ),
        Piece::triangle(
            Color::rgb(255, 167, 38),
            [p(380, 130), p(520, 270), p(380, 410)],
        ),
        Piece::quad(
            Color::rgb(102, 187, 106),
            [p(220, 410), p(300, 330), p(460, 410), p(380, 490)],
        ),
        Piece::quad(
            Color::rgb(38, 198, 218),
            [p(650, 100), p(930, 100), p(810, 220), p(530, 220)],
        ),
        Piece::triangle(
            Color::rgb(66, 165, 245),
            [p(540, 300), p(830, 180), p(830, 380)],
        ),
        Piece::quad(
            Color::rgb(171, 71, 188),
            [p(630, 320), p(890, 320), p(790, 500), p(530, 500)],
        ),
    ];

    const CUTOUTS: [Shape; 4] = [
        Shape::Quad([p(300, 230), p(360, 290), p(300, 350), p(240, 290)]),
        Shape::Quad([p(530, 100), p(680, 100), p(620, 220), p(530, 250)]),
        Shape::Quad([p(780, 240), p(930, 160), p(930, 340), p(820, 360)]),
        Shape::Quad([p(530, 500), p(690, 500), p(650, 350), p(530, 350)]),
    ];

    const fn p(x: i32, y: i32) -> Point {
        Point::new(x, y)
    }

    pub fn render(framebuffer: &mut [u8], width: usize, height: usize) {
        let mut canvas = Canvas::new(framebuffer, width, height);
        canvas.clear(BACKGROUND);

        let scale = min(
            (width as i32 * SCALE_ONE) / DESIGN_W,
            (height as i32 * SCALE_ONE) / DESIGN_H,
        );
        let offset_x = ((width as i32 * SCALE_ONE) - DESIGN_W * scale) / (2 * SCALE_ONE);
        let offset_y = ((height as i32 * SCALE_ONE) - DESIGN_H * scale) / (2 * SCALE_ONE);

        let mut points = [Point::new(0, 0); 4];

        for piece in PIECES {
            let count = transform_shape(piece.shape, scale, offset_x, offset_y, &mut points);
            canvas.fill_polygon(&points[..count], piece.color);
            canvas.draw_polygon_outline(&points[..count], OUTLINE);
        }

        for cutout in CUTOUTS {
            let count = transform_shape(cutout, scale, offset_x, offset_y, &mut points);
            canvas.fill_polygon(&points[..count], BACKGROUND);
        }
    }

    fn transform_shape(
        shape: Shape,
        scale: i32,
        offset_x: i32,
        offset_y: i32,
        out: &mut [Point; 4],
    ) -> usize {
        match shape {
            Shape::Triangle(points) => {
                for (dst, point) in out.iter_mut().zip(points) {
                    *dst = transform_point(point, scale, offset_x, offset_y);
                }
                3
            }
            Shape::Quad(points) => {
                for (dst, point) in out.iter_mut().zip(points) {
                    *dst = transform_point(point, scale, offset_x, offset_y);
                }
                4
            }
        }
    }

    fn transform_point(point: Point, scale: i32, offset_x: i32, offset_y: i32) -> Point {
        Point::new(
            offset_x + point.x * scale / SCALE_ONE,
            offset_y + point.y * scale / SCALE_ONE,
        )
    }

    struct Canvas<'a> {
        framebuffer: &'a mut [u8],
        width: usize,
        height: usize,
    }

    impl<'a> Canvas<'a> {
        fn new(framebuffer: &'a mut [u8], width: usize, height: usize) -> Self {
            Self {
                framebuffer,
                width,
                height,
            }
        }

        fn clear(&mut self, color: Color) {
            let pixels = min(
                self.width.saturating_mul(self.height),
                self.framebuffer.len() / 4,
            );
            for index in 0..pixels {
                let base = index * 4;
                self.framebuffer[base] = color.b;
                self.framebuffer[base + 1] = color.g;
                self.framebuffer[base + 2] = color.r;
                self.framebuffer[base + 3] = 0xff;
            }
        }

        fn fill_polygon(&mut self, points: &[Point], color: Color) {
            if points.len() < 3 {
                return;
            }
            for index in 1..(points.len() - 1) {
                self.fill_triangle(points[0], points[index], points[index + 1], color);
            }
        }

        fn draw_polygon_outline(&mut self, points: &[Point], color: Color) {
            if points.is_empty() {
                return;
            }
            for index in 0..points.len() {
                let next = (index + 1) % points.len();
                self.draw_line(points[index], points[next], color);
            }
        }

        fn fill_triangle(&mut self, a: Point, b: Point, c: Point, color: Color) {
            let min_x = max(0, min(a.x, min(b.x, c.x)));
            let max_x = min(self.width as i32 - 1, max(a.x, max(b.x, c.x)));
            let min_y = max(0, min(a.y, min(b.y, c.y)));
            let max_y = min(self.height as i32 - 1, max(a.y, max(b.y, c.y)));

            let area = edge(a, b, c);
            if area == 0 {
                return;
            }

            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    let p = Point::new(x, y);
                    let w0 = edge(b, c, p);
                    let w1 = edge(c, a, p);
                    let w2 = edge(a, b, p);
                    if (area > 0 && w0 >= 0 && w1 >= 0 && w2 >= 0)
                        || (area < 0 && w0 <= 0 && w1 <= 0 && w2 <= 0)
                    {
                        self.put_pixel(x, y, color);
                    }
                }
            }
        }

        fn draw_line(&mut self, start: Point, end: Point, color: Color) {
            let mut x0 = start.x;
            let mut y0 = start.y;
            let x1 = end.x;
            let y1 = end.y;

            let dx = (x1 - x0).abs();
            let sx = if x0 < x1 { 1 } else { -1 };
            let dy = -(y1 - y0).abs();
            let sy = if y0 < y1 { 1 } else { -1 };
            let mut err = dx + dy;

            loop {
                self.put_pixel(x0, y0, color);
                if x0 == x1 && y0 == y1 {
                    break;
                }
                let err2 = err * 2;
                if err2 >= dy {
                    err += dy;
                    x0 += sx;
                }
                if err2 <= dx {
                    err += dx;
                    y0 += sy;
                }
            }
        }

        fn put_pixel(&mut self, x: i32, y: i32, color: Color) {
            if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
                return;
            }
            let index = (y as usize * self.width + x as usize) * 4;
            if index + 3 >= self.framebuffer.len() {
                return;
            }
            self.framebuffer[index] = color.b;
            self.framebuffer[index + 1] = color.g;
            self.framebuffer[index + 2] = color.r;
            self.framebuffer[index + 3] = 0xff;
        }
    }

    fn edge(a: Point, b: Point, p: Point) -> i64 {
        (p.x - a.x) as i64 * (b.y - a.y) as i64 - (p.y - a.y) as i64 * (b.x - a.x) as i64
    }
}

/// 主机平台占位入口。
///
/// 用于 `cargo publish` 在宿主机上验证打包内容。
#[cfg(not(target_arch = "riscv64"))]
fn main() {}
