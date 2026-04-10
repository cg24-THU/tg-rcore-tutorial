#![no_std]
#![no_main]

use core::panic::PanicInfo;
use cg_tg_rcore_tutorial_t3l2_user_apps::{exit_call, run_piece};

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
extern "C" fn _start() -> ! {
    run_piece(6)
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    let _ = exit_call(127);
    loop {
        core::hint::spin_loop();
    }
}
