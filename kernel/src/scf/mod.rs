//! Syscall Forwarding.

mod queue;

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

pub fn notify(irq_num: usize) {
    crate::drivers::interrupt::send_ipi(irq_num);
}

#[derive(Copy, Clone)]
pub struct SCF {
    pub slot_num: usize,
}

impl SCF {
    pub fn new(slot_num: usize) -> Self {
        Self {
            slot_num,
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

pub fn init() {
    queue::init_all_queues();
    crate::drivers::timer::add_timer_event(handle_irq);
}