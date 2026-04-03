#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use core::{
    ffi::{c_char, c_int, c_long, CStr},
    slice,
    sync::atomic::{AtomicBool, Ordering},
};
use user_lib::{
    clock_gettime, exit, framebuffer_flush, framebuffer_get_info, getpid, input_next_event, lseek,
    open, read, sbrk, sleep, write, ClockId, FramebufferInfo, InputKeyEvent, OpenFlags, TimeSpec,
};

unsafe extern "C" {
    static DG_ScreenBuffer: *mut u32;
}

const DOOM_WIDTH: usize = 640;
const DOOM_HEIGHT: usize = 400;
const DOOM_STRIDE: usize = DOOM_WIDTH * 4;

const O_ACCMODE: c_int = 0o3;
const O_RDONLY: c_int = 0;
const O_WRONLY: c_int = 0o1;
const O_RDWR: c_int = 0o2;
const O_CREAT: c_int = 0o100;
const O_TRUNC: c_int = 0o1000;

#[repr(C)]
struct Timeval {
    tv_sec: c_long,
    tv_usec: c_long,
}

static FIRST_FRAME_LOGGED: AtomicBool = AtomicBool::new(false);

fn map_open_flags(flags: c_int) -> OpenFlags {
    let mut out = match flags & O_ACCMODE {
        O_WRONLY => OpenFlags::WRONLY,
        O_RDWR => OpenFlags::RDWR,
        _ => OpenFlags::RDONLY,
    };
    if flags & O_CREAT != 0 {
        out |= OpenFlags::CREATE;
    }
    if flags & O_TRUNC != 0 {
        out |= OpenFlags::TRUNC;
    }
    out
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_Init() {
    let mut info = FramebufferInfo {
        width: 0,
        height: 0,
        stride: 0,
        format: 0,
    };
    if framebuffer_get_info(&mut info) >= 0 {
        println!(
            "doom: framebuffer {}x{} stride={} format={}",
            info.width, info.height, info.stride, info.format
        );
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_DrawFrame() {
    let screen = unsafe { DG_ScreenBuffer as *const u8 };
    if screen.is_null() {
        return;
    }
    let ret = framebuffer_flush(screen, DOOM_STRIDE * DOOM_HEIGHT, DOOM_WIDTH, DOOM_HEIGHT, DOOM_STRIDE);
    if ret >= 0 && !FIRST_FRAME_LOGGED.swap(true, Ordering::Relaxed) {
        println!("doom: first frame flushed");
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_SleepMs(ms: u32) {
    sleep(ms as usize);
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_GetTicksMs() -> u32 {
    let mut ts = TimeSpec::ZERO;
    if clock_gettime(ClockId::CLOCK_MONOTONIC, &mut ts as *mut _) < 0 {
        0
    } else {
        (ts.tv_sec * 1000 + ts.tv_nsec / 1_000_000) as u32
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_GetKey(pressed: *mut c_int, doom_key: *mut u8) -> c_int {
    let mut event = InputKeyEvent { key: 0, pressed: 0 };
    if input_next_event(&mut event) <= 0 {
        return 0;
    }
    unsafe {
        *pressed = event.pressed as c_int;
        *doom_key = event.key as u8;
    }
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn DG_SetWindowTitle(_title: *const c_char) {}

#[unsafe(no_mangle)]
pub extern "C" fn __tg_open(path: *const c_char, flags: c_int) -> isize {
    if path.is_null() {
        return -1;
    }
    let path = unsafe { CStr::from_ptr(path) };
    let Ok(path) = core::str::from_utf8(path.to_bytes_with_nul()) else {
        return -1;
    };
    open(path, map_open_flags(flags))
}

#[unsafe(no_mangle)]
pub extern "C" fn __tg_close(fd: c_int) -> isize {
    user_lib::close(fd as usize)
}

#[unsafe(no_mangle)]
pub extern "C" fn __tg_read(fd: c_int, buf: *mut u8, len: usize) -> isize {
    if buf.is_null() {
        return -1;
    }
    read(fd as usize, unsafe { slice::from_raw_parts_mut(buf, len) })
}

#[unsafe(no_mangle)]
pub extern "C" fn __tg_write(fd: c_int, buf: *const u8, len: usize) -> isize {
    if buf.is_null() {
        return -1;
    }
    write(fd as usize, unsafe { slice::from_raw_parts(buf, len) })
}

#[unsafe(no_mangle)]
pub extern "C" fn __tg_lseek(fd: c_int, offset: c_long, whence: c_int) -> isize {
    lseek(fd as usize, offset as isize, whence as usize)
}

#[unsafe(no_mangle)]
pub extern "C" fn __tg_sbrk(increment: c_int) -> isize {
    sbrk(increment)
}

#[unsafe(no_mangle)]
pub extern "C" fn __tg_gettimeofday(tv: *mut Timeval) -> isize {
    if tv.is_null() {
        return -1;
    }
    let mut ts = TimeSpec::ZERO;
    if clock_gettime(ClockId::CLOCK_MONOTONIC, &mut ts as *mut _) < 0 {
        return -1;
    }
    unsafe {
        (*tv).tv_sec = ts.tv_sec as c_long;
        (*tv).tv_usec = (ts.tv_nsec / 1_000) as c_long;
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn __tg_getpid() -> isize {
    getpid()
}

#[unsafe(no_mangle)]
pub extern "C" fn __tg_exit(status: c_int) -> ! {
    exit(status);
    unreachable!()
}
