#![no_std]
#![feature(linkage)]
#![feature(asm_const)]
#![feature(naked_functions)]
#![feature(panic_info_message)]

use core::sync::atomic::{AtomicBool, Ordering};
use core::cell::UnsafeCell;

#[macro_use]
pub mod console;

mod arch;
mod lang_items;
mod syscall;
use core::arch::asm;

#[repr(C)]
pub struct TimeSpec {
    /// seconds
    pub sec: usize,
    /// nano seconds
    pub nsec: usize,
}

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start() -> ! {
    exit(main());
}

#[linkage = "weak"]
#[no_mangle]
fn main() -> i32 {
    panic!("Cannot find main!");
}

use syscall::*;

pub fn read(fd: usize, buf: &mut [u8]) -> isize {
    sys_read(fd, buf)
}

pub fn write(fd: usize, buf: &[u8]) -> isize {
    // sys_write(fd, buf)
    let scf = get_uintr_scf();
    let ret = scf.send_request(1, [fd as u64, buf.as_ptr() as u64, buf.len() as u64, 0]);
    release_uintr_scf();
    ret
}

pub fn reset_scf() {
    let scf = get_uintr_scf();
    scf.init_done = false;
    release_uintr_scf();
}

pub fn exit(exit_code: i32) -> ! {
    sys_exit(exit_code)
}

pub fn sched_yield() -> isize {
    sys_yield()
}

pub fn get_time() -> isize {
    sys_get_time()
}

pub fn getpid() -> isize {
    sys_getpid()
}

pub fn fork() -> isize {
    let ret = sys_fork();
    if ret == 0 {
        // child process
        reset_scf();
    }
    ret
}

pub fn exec(path: &str) -> isize {
    sys_exec(path)
}

pub fn wait(exit_code: &mut i32) -> isize {
    loop {
        match sys_waitpid(-1, exit_code as *mut _) {
            -2 => {
                sched_yield();
            }
            // -1 or a real pid
            exit_pid => return exit_pid,
        }
    }
}

pub fn waitpid(pid: usize, exit_code: &mut i32) -> isize {
    loop {
        match sys_waitpid(pid as isize, exit_code as *mut _) {
            -2 => {
                sched_yield();
            }
            // -1 or a real pid
            exit_pid => return exit_pid,
        }
    }
}

pub fn sleep(period_ms: usize) {
    sys_nanosleep(&TimeSpec {
        sec: period_ms / 1000,
        nsec: (period_ms % 1000) * 1_000_000,
    });
}

pub fn thread_spawn(entry: fn(usize) -> i32, arg: usize) -> usize {
    use core::sync::atomic::AtomicUsize;
    const MAX_THREADS: usize = 16;
    const THREAD_STACK_SIZE: usize = 4096 * 4; // 16K
    static mut THREAD_STACKS: [[u8; THREAD_STACK_SIZE]; MAX_THREADS] =
        [[0; THREAD_STACK_SIZE]; MAX_THREADS];
    static THREAD_COUNT: AtomicUsize = AtomicUsize::new(0);

    let thread_id = THREAD_COUNT.fetch_add(1, Ordering::AcqRel);
    let newsp = unsafe { THREAD_STACKS[thread_id].as_ptr_range().end as usize };
    sys_clone(entry, arg, newsp)
}

pub fn init_cross_uintr(upid_addr: usize, desc_addr: usize) -> usize {
    sys_init_cross_uintr(upid_addr, desc_addr)
}

/// 开启中断 UIF
#[inline(always)]
pub fn stui() {
    unsafe {asm!("stui", options(nostack))};
}

/// 关闭中断 UIF
#[inline(always)]
pub fn clui() {
    unsafe {asm!("clui", options(nostack))};
}

pub fn uintr_register_sender(upid_addr: usize, uvec: u8) -> isize {
    sys_uintr_register_sender(upid_addr, uvec)
}

pub fn uintr_register_handler(handler: usize) -> usize {
    sys_uintr_register_handler(handler)
}

// 发送用户中断
#[inline(always)]
pub unsafe fn senduipi(upid_addr: u64) {
    asm!(
        "senduipi rax",
        in("rax") upid_addr,
        options(nostack),
    );
}

// 定义中断帧结构体，用于描述栈上保存的寄存器布局
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct TrapFrame {
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rax: u64,
    pub xsave_area: u64,  // 设置为 NULL
    pub pad: u64,         // 填充对齐
    pub uirrv: u64,       // 中断请求值
    // 注意：RIP 和 RSP 由硬件自动保存
}

#[macro_export]
macro_rules! make_uintr_entry {
    ($name:ident, $handler:ident) => {
        #[naked]
        pub unsafe extern "C" fn $name() {
            asm!(
                // CFI 指令用于调试和栈展开
                ".cfi_startproc",
                ".cfi_signal_frame",
                ".cfi_def_cfa rsp, 32",
                ".cfi_offset rsp, -8",
                ".cfi_offset rip, -24",
        
                // 跳过填充 (8字节)
                "sub rsp, 8",
                ".cfi_adjust_cfa_offset 8",
        
                // 设置 xsave_area 为 NULL
                "push 0",
                ".cfi_adjust_cfa_offset 8",
        
                // 保存所有通用寄存器到栈上 (构建中断帧)
                "push rax",
                ".cfi_adjust_cfa_offset 8",
                ".cfi_rel_offset rax, 0",
        
                "push r15",
                ".cfi_adjust_cfa_offset 8",
                ".cfi_rel_offset r15, 0",
        
                "push r14",
                ".cfi_adjust_cfa_offset 8",
                ".cfi_rel_offset r14, 0",
        
                "push r13",
                ".cfi_adjust_cfa_offset 8",
                ".cfi_rel_offset r13, 0",
        
                "push r12",
                ".cfi_adjust_cfa_offset 8",
                ".cfi_rel_offset r12, 0",
        
                "push rbp",
                ".cfi_adjust_cfa_offset 8",
                ".cfi_rel_offset rbp, 0",
        
                "push rbx",
                ".cfi_adjust_cfa_offset 8",
                ".cfi_rel_offset rbx, 0",
        
                "push r11",
                ".cfi_adjust_cfa_offset 8",
                ".cfi_rel_offset r11, 0",
        
                "push r10",
                ".cfi_adjust_cfa_offset 8",
                ".cfi_rel_offset r10, 0",
        
                "push r9",
                ".cfi_adjust_cfa_offset 8",
                ".cfi_rel_offset r9, 0",
        
                "push r8",
                ".cfi_adjust_cfa_offset 8",
                ".cfi_rel_offset r8, 0",
        
                "push rcx",
                ".cfi_adjust_cfa_offset 8",
                ".cfi_rel_offset rcx, 0",
        
                "push rdx",
                ".cfi_adjust_cfa_offset 8",
                ".cfi_rel_offset rdx, 0",
        
                "push rsi",
                ".cfi_adjust_cfa_offset 8",
                ".cfi_rel_offset rsi, 0",
        
                "push rdi",
                ".cfi_adjust_cfa_offset 8",
                ".cfi_rel_offset rdi, 0",
        
                // 设置第一个参数为中断帧指针
                "mov rdi, rsp",
        
                // 对齐栈指针 (16字节对齐)
                "sub rsp, 8",
                ".cfi_adjust_cfa_offset 8",
        
                // 调用 Rust 中断处理函数
                concat!("call ", stringify!($handler)),
        
                // 恢复栈指针
                "add rsp, 8",
                ".cfi_adjust_cfa_offset -8",
                
                // 恢复所有通用寄存器
                "pop rdi",
                ".cfi_adjust_cfa_offset -8",
                
                "pop rsi",
                ".cfi_adjust_cfa_offset -8",
                
                "pop rdx",
                ".cfi_adjust_cfa_offset -8",
                
                "pop rcx",
                ".cfi_adjust_cfa_offset -8",
                
                "pop r8",
                ".cfi_adjust_cfa_offset -8",
                
                "pop r9",
                ".cfi_adjust_cfa_offset -8",
                
                "pop r10",
                ".cfi_adjust_cfa_offset -8",
                
                "pop r11",
                ".cfi_adjust_cfa_offset -8",
                
                "pop rbx",
                ".cfi_adjust_cfa_offset -8",
                
                "pop rbp",
                ".cfi_adjust_cfa_offset -8",
                
                "pop r12",
                ".cfi_adjust_cfa_offset -8",
                
                "pop r13",
                ".cfi_adjust_cfa_offset -8",
                
                "pop r14",
                ".cfi_adjust_cfa_offset -8",
                
                "pop r15",
                ".cfi_adjust_cfa_offset -8",
                
                "pop rax",
                ".cfi_adjust_cfa_offset -8",
        
                // 移除 pad, uirrv 和 xsave_area (共24字节)
                "add rsp, 24",
                ".cfi_adjust_cfa_offset -24",
        
                // 用户中断返回指令
                "uiret",
                "nop",
        
                ".cfi_endproc",
                
                options(noreturn)
            )
        }
    };
}

static SYSCALL_DONE: AtomicBool = AtomicBool::new(false);
static UINTR_SCF_INITIALIZED: AtomicBool = AtomicBool::new(false);

#[no_mangle]
pub extern "C" fn naked_scf_response_handler(trap_frame: &mut TrapFrame) {
    match trap_frame.uirrv {
        0 => {
            UINTR_SCF_INITIALIZED.store(true, Ordering::SeqCst);
        }
        1 => {
            SYSCALL_DONE.store(true, Ordering::SeqCst);
        }
        _ => {
            panic!("Unrecognized uirrv");
        }
    }
    stui();
}

make_uintr_entry!(scf_response_handler, naked_scf_response_handler);

#[repr(C)]
#[derive(Debug)]
struct UintrScfDescriptor {
    opcode: u8,
    args: [u64; 4],
    ret_val: u64,
}

struct UintrSCF {
    desc: UintrScfDescriptor,
    linux_upid: usize,
    uitte: isize,
    pub init_done: bool,
}

impl UintrSCF {
    const fn new() -> Self {
        Self {
            desc: UintrScfDescriptor {
                opcode: 0,
                args: [0; 4],
                ret_val: 0,
            },
            linux_upid: 0,
            uitte: -1,
            init_done: false,
        }
    }

    fn init(&mut self) -> isize {
        let handler_address = scf_response_handler as usize;
        let upid_addr = uintr_register_handler(handler_address);
        stui();

        self.linux_upid = init_cross_uintr(upid_addr, &self.desc as *const UintrScfDescriptor as usize);
        loop {
            if UINTR_SCF_INITIALIZED.load(Ordering::SeqCst) {
                break;
            }
        }
        if self.linux_upid == 0 {
            return -1;
        }

        // let entry = uintr_register_sender(self.linux_upid, 0);
        // if entry < 0 {
        //     return -1;
        // }
        // unsafe { senduipi(entry.try_into().unwrap()) };
        
        self.uitte = uintr_register_sender(self.linux_upid, 1);
        if self.uitte < 0 {
            return -1;
        }
        self.init_done = true;
        return 0;
    }

    fn send_request(&mut self, opcode: u8, args: [u64; 4]) -> isize {
        if !self.init_done {
            self.init();
        }
        if !self.init_done {
            panic!("SCF not initialized");
        }
        // self.desc.opcode = opcode;
        // self.desc.args = args;
        // self.desc.ret_val = 0;

        // 强制写入 desc，避免优化
        unsafe {
            core::ptr::write_volatile(&mut self.desc.opcode, opcode);
            core::ptr::write_volatile(&mut self.desc.args, args);
            core::ptr::write_volatile(&mut self.desc.ret_val, 0);
        }

        // 确保写入在 senduipi 之前完成
        core::sync::atomic::fence(Ordering::Release);

        SYSCALL_DONE.store(false, Ordering::SeqCst);

        unsafe {senduipi(self.uitte.try_into().unwrap());}
        while !SYSCALL_DONE.load(Ordering::SeqCst) {
            sched_yield();
        }
        self.desc.ret_val as _
    }
}

static SCF_LOCK: AtomicBool = AtomicBool::new(false);
struct SyncUnsafeCell(UnsafeCell<UintrSCF>);
unsafe impl Sync for SyncUnsafeCell {}
static UINTR_SCF: SyncUnsafeCell = SyncUnsafeCell(UnsafeCell::new(UintrSCF::new()));

fn get_uintr_scf() -> &'static mut UintrSCF {
    // 自旋等待锁释放
    while SCF_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        sched_yield();
    }
    
    unsafe { &mut *UINTR_SCF.0.get() }
}

fn release_uintr_scf() {
    SCF_LOCK.store(false, Ordering::Release);
}