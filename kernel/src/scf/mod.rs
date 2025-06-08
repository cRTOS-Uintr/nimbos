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
use crate::drivers::interrupt::IrqHandlerResult;
use crate::sync::Mutex;
use crate::task::manager::TASK_MANAGER;
use crate::task::CurrentTask;
use crate::drivers::interrupt::apic::send_ipi_raw;
#[cfg(feature = "uintr")]
use crate::syscall::uintr::{UintrUpid, get_upid_mem_start};

// pub fn notify(irq_num: usize) {
//     crate::drivers::interrupt::send_ipi(irq_num);
// }

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

    // pub fn irq_num(&self) -> usize {
    //     SYSCALL_IPI_IRQ_NUM + self.slot_num
    // }

    #[cfg(not(feature = "uintr"))]
    pub fn ipi_notify(&self) {
        let _manager = TASK_MANAGER.lock();
        let current_task = CurrentTask::get().0;
        let ctx = unsafe{&mut *current_task.context().as_ptr()};
        debug!("SCF::ipi_notify slot_num: {}, ipi_vector: {}, ipi_dest: {}", self.slot_num, ctx.ipi_vector, ctx.ipi_dest);
        send_ipi_raw(ctx.ipi_vector as u8, ctx.ipi_dest as usize);
    }
}

pub fn handle_irq() {
    trace!("handle_irq");
    for slot_num in 0..SYSCALL_MAX_SLOT_NUM {
        while let Some(rsp) = get_queue(slot_num).pop_response() {
            if rsp.token.is_valid() {
                rsp.token.as_cond_var().signal(rsp.ret_val);
            }
        }
    }
}

const APIC_LINUX_IPI_VECTOR: usize = 40;
#[cfg(feature = "uintr")]
const APIC_LINUX_IPI_VECTOR2: usize = 41;
pub fn init() {
    queue::init_all_queues();
    crate::drivers::interrupt::register_handler(APIC_LINUX_IPI_VECTOR, ||  {
        handle_irq();
        IrqHandlerResult::Reschedule
    });
    #[cfg(feature = "uintr")]
    {
        crate::drivers::interrupt::register_handler(APIC_LINUX_IPI_VECTOR2, ||  {
            handle_irq();
            let upid_addr = get_upid_mem_start();
            let upid = unsafe { &mut *(upid_addr as *mut UintrUpid) };
            upid.nc.status = 0; // Reset the outstanding notification bit
            IrqHandlerResult::Reschedule
        });
    }
    // crate::drivers::timer::add_timer_event(handle_irq);
}