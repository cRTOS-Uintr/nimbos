use crate::arch::TrapFrame;
use crate::mm::UserInPtr;
use crate::task::{spawn_task, CurrentTask};

const MAX_STR_LEN: usize = 256;

pub fn sys_exit(exit_code: i32) -> ! {
    trace!("exit task_id: {}", CurrentTask::get().pid().as_usize());
    CurrentTask::get().scf_exit();
    CurrentTask::get().exit(exit_code);
}

pub fn sys_yield() -> isize {
    CurrentTask::get().yield_now();
    0
}

pub fn sys_getpid() -> isize {
    CurrentTask::get().pid().as_usize() as isize
}

pub fn sys_clone(newsp: usize, tf: &TrapFrame) -> isize {
    let new_task = CurrentTask::get().new_clone(newsp, tf);
    let pid = new_task.pid().as_usize() as isize;
    CurrentTask::get().scf_clone();
    spawn_task(new_task);
    pid
}

pub fn sys_fork(tf: &TrapFrame) -> isize {
    debug!("sys_fork: task_id: {}", CurrentTask::get().pid().as_usize());
    let new_irq_num = CurrentTask::get().scf_syncfork();
    assert!(new_irq_num > 0);
    let new_task = CurrentTask::get().new_fork_scf(tf, new_irq_num as _);
    let pid = new_task.pid().as_usize() as isize;
    spawn_task(new_task);
    trace!("sys_fork: new task_id: {}", pid);
    pid
}

pub fn sys_exec(path: UserInPtr<u8>, tf: &mut TrapFrame) -> isize {
    let (path_buf, len) = path.read_str::<MAX_STR_LEN>();
    let path_str = core::str::from_utf8(&path_buf[..len]).unwrap();
    let ret = CurrentTask::get().scf_lexec(path_str, tf);
    if ret < 0 {
        CurrentTask::get().scf_rexec(path.as_ptr(), tf)
    } else {
        ret
    }
}
