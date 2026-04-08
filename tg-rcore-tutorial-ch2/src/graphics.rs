#[cfg(target_arch = "riscv64")]
use core::{
    alloc::{GlobalAlloc, Layout},
    ptr::{NonNull, addr_of_mut},
    slice,
    sync::atomic::{AtomicUsize, Ordering},
};
#[cfg(target_arch = "riscv64")]
use spin::Mutex;
#[cfg(target_arch = "riscv64")]
use tg_console::log;
#[cfg(target_arch = "riscv64")]
use virtio_drivers::{DeviceType, Hal, MmioTransport, Transport, VirtIOGpu, VirtIOHeader};

#[cfg(target_arch = "riscv64")]
const VIRTIO_MMIO_START: usize = 0x1000_1000;
#[cfg(target_arch = "riscv64")]
const VIRTIO_MMIO_END: usize = 0x1000_8000;
#[cfg(target_arch = "riscv64")]
const VIRTIO_MMIO_STRIDE: usize = 0x1000;
#[cfg(target_arch = "riscv64")]
const PAGE_SIZE: usize = 4096;
#[cfg(target_arch = "riscv64")]
const HEAP_SIZE: usize = 512 * 1024;
#[cfg(target_arch = "riscv64")]
const DMA_SIZE: usize = 16 * 1024 * 1024;
#[cfg(target_arch = "riscv64")]
const STEP_FRAME_SPIN: usize = 8_000_000;
#[cfg(target_arch = "riscv64")]
const FINAL_FRAME_SPIN: usize = 20_000_000;

#[cfg(target_arch = "riscv64")]
static GPU_DEVICE: Mutex<Option<GpuDevice>> = Mutex::new(None);

#[cfg(target_arch = "riscv64")]
#[allow(dead_code)]
#[repr(align(16))]
struct HeapSpace([u8; HEAP_SIZE]);

#[cfg(target_arch = "riscv64")]
#[allow(dead_code)]
#[repr(align(4096))]
struct DmaSpace([u8; DMA_SIZE]);

#[cfg(target_arch = "riscv64")]
static mut HEAP_SPACE: HeapSpace = HeapSpace([0; HEAP_SIZE]);
#[cfg(target_arch = "riscv64")]
static mut DMA_SPACE: DmaSpace = DmaSpace([0; DMA_SIZE]);
#[cfg(target_arch = "riscv64")]
static HEAP_NEXT: AtomicUsize = AtomicUsize::new(0);
#[cfg(target_arch = "riscv64")]
static DMA_NEXT: AtomicUsize = AtomicUsize::new(0);

#[cfg(target_arch = "riscv64")]
pub struct KernelAllocator;

#[cfg(target_arch = "riscv64")]
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

#[cfg(target_arch = "riscv64")]
pub fn init() -> Result<(), &'static str> {
    let mut gpu_slot = GPU_DEVICE.lock();
    if gpu_slot.is_some() {
        return Ok(());
    }

    let transport = find_gpu_transport().ok_or("VirtIO-GPU device not found")?;
    let mut gpu =
        VirtIOGpu::<VirtioHal, MmioTransport>::new(transport).map_err(|_| "GPU init failed")?;
    let (width, height) = gpu.resolution().map_err(|_| "failed to query resolution")?;

    let (framebuffer, framebuffer_len) = {
        let framebuffer = gpu
            .setup_framebuffer()
            .map_err(|_| "failed to setup framebuffer")?;
        framebuffer.fill(0);
        (framebuffer.as_mut_ptr(), framebuffer.len())
    };

    let mut device = GpuDevice {
        gpu,
        framebuffer,
        framebuffer_len,
        width: width as usize,
        height: height as usize,
    };
    clear_background(&mut device);
    device.flush()?;
    log::info!("VirtIO-GPU framebuffer ready: {}x{}", width, height);
    *gpu_slot = Some(device);
    Ok(())
}

#[cfg(not(target_arch = "riscv64"))]
pub fn init() -> Result<(), &'static str> {
    Ok(())
}

#[cfg(target_arch = "riscv64")]
pub fn draw_piece(piece_id: usize) -> isize {
    log::info!("request draw piece: {}", piece_id);
    let mut gpu_slot = GPU_DEVICE.lock();
    let Some(gpu) = gpu_slot.as_mut() else {
        log::error!("draw_piece called before graphics init");
        return -1;
    };
    if piece_id >= STEPS.len() {
        log::error!("invalid tangram piece id: {}", piece_id);
        return -1;
    }

    render_step(gpu, piece_id);
    match gpu.flush() {
        Ok(()) => {
            log::info!("gpu.flush() success");
            hold_frame(STEP_FRAME_SPIN);
            0
        }
        Err(reason) => {
            log::error!("gpu.flush() failed: {}", reason);
            -1
        }
    }
}

#[cfg(not(target_arch = "riscv64"))]
pub fn draw_piece(_piece_id: usize) -> isize {
    0
}

#[cfg(target_arch = "riscv64")]
pub fn hold_final_frame() {
    hold_frame(FINAL_FRAME_SPIN);
}

#[cfg(not(target_arch = "riscv64"))]
pub fn hold_final_frame() {}

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

#[cfg(target_arch = "riscv64")]
fn clear_background(gpu: &mut GpuDevice) {
    let (width, height) = (gpu.width, gpu.height);
    let framebuffer = gpu.framebuffer();
    let mut canvas = Canvas::new(framebuffer, width, height);
    canvas.clear(BACKGROUND);
}

#[cfg(target_arch = "riscv64")]
fn render_step(gpu: &mut GpuDevice, piece_id: usize) {
    let (width, height) = (gpu.width, gpu.height);
    let framebuffer = gpu.framebuffer();
    let mut canvas = Canvas::new(framebuffer, width, height);
    let step = STEPS[piece_id];

    let scale = core::cmp::min(
        (width as i32 * SCALE_ONE) / DESIGN_W,
        (height as i32 * SCALE_ONE) / DESIGN_H,
    );
    let offset_x = ((width as i32 * SCALE_ONE) - DESIGN_W * scale) / (2 * SCALE_ONE);
    let offset_y = ((height as i32 * SCALE_ONE) - DESIGN_H * scale) / (2 * SCALE_ONE);
    let mut points = [Point::new(0, 0); 4];

    let count = transform_shape(step.piece.shape, scale, offset_x, offset_y, &mut points);
    canvas.fill_polygon(&points[..count], step.piece.color);
    canvas.draw_polygon_outline(&points[..count], OUTLINE);

    for cutout in step.cutouts {
        let count = transform_shape(*cutout, scale, offset_x, offset_y, &mut points);
        canvas.fill_polygon(&points[..count], BACKGROUND);
    }
}

#[cfg(target_arch = "riscv64")]
struct GpuDevice {
    gpu: VirtIOGpu<'static, VirtioHal, MmioTransport>,
    framebuffer: *mut u8,
    framebuffer_len: usize,
    width: usize,
    height: usize,
}

#[cfg(target_arch = "riscv64")]
unsafe impl Send for GpuDevice {}
#[cfg(target_arch = "riscv64")]
unsafe impl Sync for GpuDevice {}

#[cfg(target_arch = "riscv64")]
impl GpuDevice {
    fn framebuffer(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.framebuffer, self.framebuffer_len) }
    }

    fn flush(&mut self) -> Result<(), &'static str> {
        self.gpu.flush().map_err(|_| "framebuffer flush failed")
    }
}

#[cfg(target_arch = "riscv64")]
fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

#[cfg(target_arch = "riscv64")]
fn reserve(base: usize, capacity: usize, next: &AtomicUsize, size: usize, align: usize) -> usize {
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

#[cfg(target_arch = "riscv64")]
fn heap_base() -> usize {
    addr_of_mut!(HEAP_SPACE) as usize
}

#[cfg(target_arch = "riscv64")]
fn dma_base() -> usize {
    addr_of_mut!(DMA_SPACE) as usize
}

#[cfg(target_arch = "riscv64")]
struct VirtioHal;

#[cfg(target_arch = "riscv64")]
impl Hal for VirtioHal {
    fn dma_alloc(pages: usize) -> usize {
        reserve(
            dma_base(),
            DMA_SIZE,
            &DMA_NEXT,
            pages * PAGE_SIZE,
            PAGE_SIZE,
        )
    }

    fn dma_dealloc(_paddr: usize, _pages: usize) -> i32 {
        0
    }

    fn phys_to_virt(paddr: usize) -> usize {
        paddr
    }

    fn virt_to_phys(vaddr: usize) -> usize {
        vaddr
    }
}

#[cfg(target_arch = "riscv64")]
const DESIGN_W: i32 = 960;
#[cfg(target_arch = "riscv64")]
const DESIGN_H: i32 = 540;
#[cfg(target_arch = "riscv64")]
const SCALE_ONE: i32 = 1024;

#[cfg(target_arch = "riscv64")]
#[derive(Clone, Copy)]
struct Color {
    r: u8,
    g: u8,
    b: u8,
}

#[cfg(target_arch = "riscv64")]
impl Color {
    const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

#[cfg(target_arch = "riscv64")]
#[derive(Clone, Copy)]
struct Point {
    x: i32,
    y: i32,
}

#[cfg(target_arch = "riscv64")]
impl Point {
    const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[cfg(target_arch = "riscv64")]
#[derive(Clone, Copy)]
enum Shape {
    Triangle([Point; 3]),
    Quad([Point; 4]),
}

#[cfg(target_arch = "riscv64")]
#[derive(Clone, Copy)]
struct Piece {
    color: Color,
    shape: Shape,
}

#[cfg(target_arch = "riscv64")]
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

#[cfg(target_arch = "riscv64")]
#[derive(Clone, Copy)]
struct Step {
    piece: Piece,
    cutouts: &'static [Shape],
}

#[cfg(target_arch = "riscv64")]
impl Step {
    const fn new(piece: Piece, cutouts: &'static [Shape]) -> Self {
        Self { piece, cutouts }
    }
}

#[cfg(target_arch = "riscv64")]
const fn p(x: i32, y: i32) -> Point {
    Point::new(x, y)
}

#[cfg(target_arch = "riscv64")]
const BACKGROUND: Color = Color::rgb(15, 23, 42);
#[cfg(target_arch = "riscv64")]
const OUTLINE: Color = Color::rgb(2, 6, 23);

#[cfg(target_arch = "riscv64")]
const CUTOUT_O: [Shape; 1] = [Shape::Quad([
    p(300, 230),
    p(360, 290),
    p(300, 350),
    p(240, 290),
])];
#[cfg(target_arch = "riscv64")]
const CUTOUT_S_TOP: [Shape; 1] = [Shape::Quad([
    p(530, 100),
    p(680, 100),
    p(620, 220),
    p(530, 250),
])];
#[cfg(target_arch = "riscv64")]
const CUTOUT_S_MID: [Shape; 1] = [Shape::Quad([
    p(780, 240),
    p(930, 160),
    p(930, 340),
    p(820, 360),
])];
#[cfg(target_arch = "riscv64")]
const CUTOUT_S_BOTTOM: [Shape; 1] = [Shape::Quad([
    p(530, 500),
    p(690, 500),
    p(650, 350),
    p(530, 350),
])];
#[cfg(target_arch = "riscv64")]
const CUTOUT_S_FINAL: [Shape; 2] = [CUTOUT_S_MID[0], CUTOUT_S_BOTTOM[0]];

#[cfg(target_arch = "riscv64")]
const STEPS: [Step; 7] = [
    Step::new(
        Piece::triangle(
            Color::rgb(239, 83, 80),
            [p(80, 270), p(220, 130), p(220, 410)],
        ),
        &[],
    ),
    Step::new(
        Piece::quad(
            Color::rgb(255, 202, 40),
            [p(220, 130), p(300, 50), p(380, 130), p(300, 210)],
        ),
        &[],
    ),
    Step::new(
        Piece::triangle(
            Color::rgb(255, 167, 38),
            [p(380, 130), p(520, 270), p(380, 410)],
        ),
        &[],
    ),
    Step::new(
        Piece::quad(
            Color::rgb(102, 187, 106),
            [p(220, 410), p(300, 330), p(460, 410), p(380, 490)],
        ),
        &CUTOUT_O,
    ),
    Step::new(
        Piece::quad(
            Color::rgb(38, 198, 218),
            [p(650, 100), p(930, 100), p(810, 220), p(530, 220)],
        ),
        &CUTOUT_S_TOP,
    ),
    Step::new(
        Piece::triangle(
            Color::rgb(66, 165, 245),
            [p(540, 300), p(830, 180), p(830, 380)],
        ),
        &[],
    ),
    Step::new(
        Piece::quad(
            Color::rgb(171, 71, 188),
            [p(630, 320), p(890, 320), p(790, 500), p(530, 500)],
        ),
        &CUTOUT_S_FINAL,
    ),
];

#[cfg(target_arch = "riscv64")]
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

#[cfg(target_arch = "riscv64")]
fn transform_point(point: Point, scale: i32, offset_x: i32, offset_y: i32) -> Point {
    Point::new(
        offset_x + point.x * scale / SCALE_ONE,
        offset_y + point.y * scale / SCALE_ONE,
    )
}

#[cfg(target_arch = "riscv64")]
struct Canvas<'a> {
    framebuffer: &'a mut [u8],
    width: usize,
    height: usize,
}

#[cfg(target_arch = "riscv64")]
impl<'a> Canvas<'a> {
    fn new(framebuffer: &'a mut [u8], width: usize, height: usize) -> Self {
        Self {
            framebuffer,
            width,
            height,
        }
    }

    fn clear(&mut self, color: Color) {
        let pixels = core::cmp::min(
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
        let min_x = core::cmp::max(0, core::cmp::min(a.x, core::cmp::min(b.x, c.x)));
        let max_x = core::cmp::min(
            self.width as i32 - 1,
            core::cmp::max(a.x, core::cmp::max(b.x, c.x)),
        );
        let min_y = core::cmp::max(0, core::cmp::min(a.y, core::cmp::min(b.y, c.y)));
        let max_y = core::cmp::min(
            self.height as i32 - 1,
            core::cmp::max(a.y, core::cmp::max(b.y, c.y)),
        );

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

#[cfg(target_arch = "riscv64")]
fn edge(a: Point, b: Point, p: Point) -> i64 {
    (p.x - a.x) as i64 * (b.y - a.y) as i64 - (p.y - a.y) as i64 * (b.x - a.x) as i64
}

#[cfg(target_arch = "riscv64")]
fn hold_frame(iterations: usize) {
    for _ in 0..iterations {
        core::hint::spin_loop();
    }
}
