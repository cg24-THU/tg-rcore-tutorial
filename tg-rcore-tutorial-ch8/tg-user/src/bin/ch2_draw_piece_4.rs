#![no_std]
#![no_main]

use core::panic::PanicInfo;
use tg_syscall::{draw_piece, exit};

const PIECE_ID: usize = 4;

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
extern "C" fn _start() -> ! {
    let code = if draw_piece(PIECE_ID) < 0 {
        127
    } else {
        PIECE_ID as i32
    };
    exit(code);
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    exit(127);
    loop {
        core::hint::spin_loop();
    }
}
