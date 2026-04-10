/// Framebuffer pixel format identifiers shared by user and kernel space.
pub const FRAMEBUFFER_FORMAT_BGRX8888: u32 = 1;

/// Basic framebuffer metadata returned by the kernel.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FramebufferInfo {
    /// Visible width in pixels.
    pub width: u32,
    /// Visible height in pixels.
    pub height: u32,
    /// Bytes per scanline.
    pub stride: u32,
    /// One of the `FRAMEBUFFER_FORMAT_*` constants.
    pub format: u32,
}

/// A single translated key event for Doom's platform layer.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct InputKeyEvent {
    /// Doom key code or ASCII byte.
    pub key: u16,
    /// Non-zero when the key is pressed, zero on release.
    pub pressed: u16,
}
