//! Intel Local APIC and IO APIC.

#![allow(dead_code)]

use x2apic::ioapic::IoApic;
use x2apic::lapic::{xapic_base, LocalApic, LocalApicBuilder, TimerDivide, TimerMode};

use super::IrqHandlerResult;
use crate::config::TICKS_PER_SEC;
use crate::mm::PhysAddr;
use crate::percpu::PerCpuData;
use crate::sync::LazyInit;

const APIC_TIMER_VECTOR: usize = 0xf0;
const APIC_SPURIOUS_VECTOR: usize = 0xf1;
const APIC_ERROR_VECTOR: usize = 0xf2;
const APIC_LDR_OFFSET: u32 = 24;

const IO_APIC_BASE: PhysAddr = PhysAddr::new(0xfec0_0000);

pub const IRQ_COUNT: usize = 256;

pub static LOCAL_APIC: LazyInit<PerCpuData<LocalApic>> = LazyInit::new();

fn lapic_eoi() {
    unsafe { LOCAL_APIC.as_mut().end_of_interrupt() };
}

pub fn set_enable(_vector: usize, _enable: bool) {
    // TODO: implement IOAPIC
}

pub fn handle_irq(vector: usize) -> IrqHandlerResult {
    lapic_eoi();
    super::HANDLERS.handle(vector)
}

pub fn send_ipi(irq_num: usize) {
    let mut io_apic = unsafe { IoApic::new(IO_APIC_BASE.into_kvaddr().as_usize() as _) };
    let entry = unsafe { io_apic.table_entry(irq_num as _) };
    let vector = entry.vector();
    let dest = entry.dest();
    // unsafe {
    //     let ptr = &mut entry as *mut RedirectionTableEntry;
    //     dest = (((*(ptr as *mut u64)) >> 48) % 0x100) as u8;
    // }
    if vector >= 0x20 {
        warn!("send_ipi {} {}", vector, dest);
    }
    else {
        warn!("send_ipi anyway: invalid vector {} dest {}", vector, dest);
    }
        #[cfg(feature = "uintr")]
        unsafe { LOCAL_APIC.as_mut().send_ipi(vector, (dest as u32) << APIC_LDR_OFFSET) };
        #[cfg(not(feature = "uintr"))]
        unsafe { LOCAL_APIC.as_mut().send_ipi(vector, dest as _) };
}

pub fn send_ipi_raw(vector: u8, dest: usize) {
    unsafe { LOCAL_APIC.as_mut().send_ipi(vector, dest as _) };
}

pub fn init() {
    super::i8259_pic::init();

    let base_vaddr = PhysAddr::new(unsafe { xapic_base() } as usize).into_kvaddr();
    let mut lapic = LocalApicBuilder::new()
        .timer_vector(APIC_TIMER_VECTOR)
        .error_vector(APIC_ERROR_VECTOR)
        .spurious_vector(APIC_SPURIOUS_VECTOR)
        .timer_mode(TimerMode::Periodic)
        .timer_divide(TimerDivide::Div256) // divide by 1
        .timer_initial((1_000_000_000 / TICKS_PER_SEC) as u32) // FIXME: need to calibrate
        .set_xapic_base(base_vaddr.as_usize() as u64)
        .ipi_destination_mode(x2apic::lapic::IpiDestMode::Logical) // Use logical for now
        .build()
        .unwrap();
    trace!("APIC init 1");
    unsafe {
        lapic.enable();
        // APIC may be software disabled when enable the timer at the first time, we need to re-enable it.
        lapic.enable_timer();
    }
    trace!("APIC init 2");
    LOCAL_APIC.init_by(PerCpuData::new(lapic));
    // #[cfg(feature = "uintr")]
    // unsafe {
    //     LOCAL_APIC.as_mut().set_logical_id(get_logical_dest());
    // }
    trace!("APIC init 3");
    super::register_handler(APIC_TIMER_VECTOR, || {
        crate::drivers::timer::timer_tick();
        IrqHandlerResult::Reschedule
    });
    trace!("APIC init 4");
}

pub fn init_local_apic_ap() {
    unsafe { LOCAL_APIC.as_mut().enable() };
}

pub fn get_apic_id() -> u32 {
    unsafe { LOCAL_APIC.as_ref().id() >> APIC_LDR_OFFSET }
}

pub fn get_logical_dest() -> u32 {
    let apic_id = get_apic_id();
    let logical_dest = 1 << (apic_id + APIC_LDR_OFFSET);
    logical_dest
}