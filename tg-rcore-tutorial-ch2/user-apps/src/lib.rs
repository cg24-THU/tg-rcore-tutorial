#![no_std]

pub const SYSCALL_EXIT: usize = 93;
pub const SYSCALL_DRAW_PIECE: usize = 1043;

#[inline]
pub fn draw_piece(piece_id: usize) -> isize {
    unsafe { syscall1(SYSCALL_DRAW_PIECE, piece_id) }
}

#[inline]
pub fn exit_call(code: i32) -> isize {
    unsafe { syscall1(SYSCALL_EXIT, code as usize) }
}

#[inline]
pub fn run_piece(piece_id: usize) -> ! {
    let code = if draw_piece(piece_id) < 0 {
        127
    } else {
        piece_id as i32
    };
    let _ = exit_call(code);
    loop {
        core::hint::spin_loop();
    }
}

#[inline(always)]
unsafe fn syscall1(id: usize, arg0: usize) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "ecall",
            inlateout("a0") arg0 as isize => ret,
            in("a7") id,
            options(nostack),
        );
    }
    ret
}
