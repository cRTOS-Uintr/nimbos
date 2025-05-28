//! Syscall Forwarding.

pub mod queue;

mod syscall;

pub mod fs;
pub mod task;

#[cfg(feature = "rvm")]
#[cfg(feature = "uintr")]
pub mod uintr;

pub use fs::*;
pub use task::*;

use queue::{get_queue, SyscallQueueBuffer};
use crate::config::scf::{SYSCALL_IPI_IRQ_NUM, SYSCALL_MAX_SLOT_NUM};
use crate::drivers::interrupt::{IrqHandler, IrqHandlerResult};
use crate::sync::Mutex;

pub fn notify(irq_num: usize) {
    crate::drivers::interrupt::send_ipi(irq_num);
}

// #[derive(Copy, Clone)]
pub struct SCF {
    pub slot_num: usize,
    pub ref_cnt: Mutex<usize>,
}

impl SCF {
    pub fn new(slot_num: usize) -> Self {
        Self {
            slot_num,
            ref_cnt: Mutex::new(1),
        }
    }

    pub fn queue(&self) -> &'static mut SyscallQueueBuffer {
        get_queue(self.slot_num)
    }

    pub fn irq_num(&self) -> usize {
        SYSCALL_IPI_IRQ_NUM + self.slot_num
    }
}

pub fn handle_irq() {
    for slot_num in 0..SYSCALL_MAX_SLOT_NUM {
        while let Some(rsp) = get_queue(slot_num).pop_response() {
            if rsp.token.is_valid() {
                rsp.token.as_cond_var().signal(rsp.ret_val);
            }
        }
    }
}

const APIC_LINUX_IPI_VECTOR: usize = 40;
const APIC_LINUX_IPI_VECTOR2: usize = 41;
pub fn init() {
    queue::init_all_queues();
    crate::drivers::interrupt::register_handler(APIC_LINUX_IPI_VECTOR, ||  {
        handle_irq();
        IrqHandlerResult::Reschedule
    });
    crate::drivers::interrupt::register_handler(APIC_LINUX_IPI_VECTOR2, ||  {
        handle_irq();
        IrqHandlerResult::Reschedule
    });
    // crate::drivers::timer::add_timer_event(handle_irq);
}