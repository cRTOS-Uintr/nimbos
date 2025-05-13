use crate::task::CurrentTask;

pub fn sys_init_cross_uintr(upid_addr: u64, desc_addr: u64) -> usize {
    CurrentTask::get().scf_init_cross_uintr(upid_addr, desc_addr)
}
