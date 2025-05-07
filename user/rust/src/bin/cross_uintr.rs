#![no_std]
#![no_main]
#![feature(naked_functions)]

#[macro_use]
extern crate user_lib;

use user_lib::{init_cross_uintr, senduipi, stui, uintr_register_handler, uintr_register_sender, TrapFrame};
use core::sync::atomic::{AtomicBool, Ordering};

static INTERRUPT_RECEIVED: AtomicBool = AtomicBool::new(false);

/// 中断处理函数
#[no_mangle]
pub extern "C" fn naked_uintr_handler(trap_frame: &mut TrapFrame) {
    INTERRUPT_RECEIVED.store(true, Ordering::SeqCst);
    println!("Received interrupt in user mode, uvec: {}",(*trap_frame).uirrv);
    stui();
}

make_uintr_entry!(uintr_handler, naked_uintr_handler);

#[no_mangle]
pub fn main() -> i32 {
    println!("Hello world from user mode program!");

    let handler_address = uintr_handler as usize;
    let upid_addr = uintr_register_handler(handler_address);
    println!("upid_addr: {:x}", upid_addr);
    stui();

    let linux_upid = init_cross_uintr(upid_addr);

    loop {
        if INTERRUPT_RECEIVED.load(Ordering::SeqCst) {
            println!("[Nimbos User] User interrupt received...");
            break;
        }
    }

    
    if linux_upid == 0 {
        println!("Nimbos failed to get linux upid");
        return -1;
    }

    let entry = uintr_register_sender(linux_upid, 0);
    if entry < 0 {
        println!("Sender register failed: {}", entry);
        return -1;
    }
    println!("Sender register success, entry: {}", entry);
    unsafe { senduipi(entry.try_into().unwrap()) };

    println!("Done!");
    0
}
