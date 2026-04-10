//! VirtIO GPU framebuffer driver.

use crate::{KERNEL_SPACE, Sv39, build_flags};
use alloc::{
    alloc::{alloc_zeroed, dealloc},
    sync::Arc,
};
use core::{alloc::Layout, ptr::NonNull, slice};
use spin::{Lazy, Mutex};
use tg_kernel_vm::page_table::{MmuMeta, VAddr, VmFlags};
use virtio_drivers::{Hal, MmioTransport, VirtIOGpu, VirtIOHeader};

/// VirtIO GPU MMIO base.
const VIRTIO_GPU0: usize = 0x1000_2000;

/// Kernel-managed framebuffer instance.
pub static GPU_DEVICE: Lazy<Arc<Mutex<GpuDevice>>> = Lazy::new(|| {
    Arc::new(Mutex::new(unsafe {
        let transport = MmioTransport::new(NonNull::new(VIRTIO_GPU0 as *mut VirtIOHeader).unwrap())
            .expect("failed to create GPU transport");
        let mut gpu =
            VirtIOGpu::<VirtioHal, MmioTransport>::new(transport).expect("failed to init GPU");
        let (width, height) = gpu.resolution().expect("failed to query GPU resolution");
        let (framebuffer, framebuffer_len) = {
            let framebuffer = gpu
                .setup_framebuffer()
                .expect("failed to setup GPU framebuffer");
            framebuffer.fill(0);
            (framebuffer.as_mut_ptr(), framebuffer.len())
        };
        gpu.flush().expect("failed to flush initial framebuffer");
        GpuDevice {
            gpu,
            framebuffer,
            framebuffer_len,
            width: width as usize,
            height: height as usize,
            stride: width as usize * 4,
        }
    }))
});

/// GPU state shared by framebuffer syscalls.
pub struct GpuDevice {
    gpu: VirtIOGpu<'static, VirtioHal, MmioTransport>,
    framebuffer: *mut u8,
    framebuffer_len: usize,
    /// Screen width in pixels.
    pub width: usize,
    /// Screen height in pixels.
    pub height: usize,
    /// Bytes per scanline.
    pub stride: usize,
}

unsafe impl Send for GpuDevice {}
unsafe impl Sync for GpuDevice {}

impl GpuDevice {
    #[inline]
    pub fn framebuffer(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.framebuffer, self.framebuffer_len) }
    }

    #[inline]
    pub fn flush(&mut self) {
        self.gpu.flush().expect("failed to flush GPU framebuffer");
    }
}

struct VirtioHal;

impl Hal for VirtioHal {
    fn dma_alloc(pages: usize) -> usize {
        unsafe {
            alloc_zeroed(Layout::from_size_align_unchecked(
                pages << Sv39::PAGE_BITS,
                1 << Sv39::PAGE_BITS,
            )) as _
        }
    }

    fn dma_dealloc(paddr: usize, pages: usize) -> i32 {
        unsafe {
            dealloc(
                paddr as _,
                Layout::from_size_align_unchecked(pages << Sv39::PAGE_BITS, 1 << Sv39::PAGE_BITS),
            )
        }
        0
    }

    #[inline]
    fn phys_to_virt(paddr: usize) -> usize {
        paddr
    }

    fn virt_to_phys(vaddr: usize) -> usize {
        const VALID: VmFlags<Sv39> = build_flags("__V");
        let ptr: NonNull<u8> = unsafe {
            KERNEL_SPACE
                .assume_init_ref()
                .translate(VAddr::new(vaddr), VALID)
                .unwrap()
        };
        ptr.as_ptr() as usize
    }
}
