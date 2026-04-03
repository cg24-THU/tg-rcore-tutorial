//! VirtIO keyboard input driver.

use crate::{build_flags, Sv39, KERNEL_SPACE};
use alloc::{
    alloc::{alloc_zeroed, dealloc},
    sync::Arc,
};
use core::{alloc::Layout, ptr::NonNull};
use spin::{Lazy, Mutex};
use tg_kernel_vm::page_table::{MmuMeta, VAddr, VmFlags};
use virtio_drivers::{Hal, InputEvent, MmioTransport, VirtIOHeader, VirtIOInput};

/// VirtIO keyboard MMIO base.
const VIRTIO_INPUT0: usize = 0x1000_3000;

/// Global keyboard device.
pub static INPUT_DEVICE: Lazy<Arc<Mutex<InputDevice>>> = Lazy::new(|| {
    Arc::new(Mutex::new(unsafe {
        let transport =
            MmioTransport::new(NonNull::new(VIRTIO_INPUT0 as *mut VirtIOHeader).unwrap())
                .expect("failed to create input transport");
        InputDevice {
            input: VirtIOInput::<VirtioHal, MmioTransport>::new(transport)
                .expect("failed to init keyboard"),
        }
    }))
});

/// Keyboard input state.
pub struct InputDevice {
    input: VirtIOInput<VirtioHal, MmioTransport>,
}

unsafe impl Send for InputDevice {}
unsafe impl Sync for InputDevice {}

impl InputDevice {
    #[inline]
    pub fn pop_pending_event(&mut self) -> Option<InputEvent> {
        self.input.pop_pending_event()
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
