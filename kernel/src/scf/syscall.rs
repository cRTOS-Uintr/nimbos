use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use alloc::vec::Vec;

use super::queue::ScfRequestToken;
use super::SCF;
use crate::config::KERNEL_HEAP_SIZE;
use crate::scf::queue::get_queue;
use crate::task::CurrentTask;

numeric_enum_macro::numeric_enum! {
    #[repr(u8)]
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub enum ScfOpcode {
        // Nop = 0,
        Read = 0,
        Write = 1,
        Open = 2,
        Close = 3,
        Stat = 4,
        Clone = 56,
        Fork = 57,
        Exit = 60,
        SyncMap = 253,
        SyncUnmap = 254,
        Unknown = 0xff,
    }
}

const CHUNK_SIZE: usize = 256;

pub struct SyscallCondVar {
    ok: AtomicBool,
    ret_val: AtomicU64,
}

impl SyscallCondVar {
    pub fn new() -> Self {
        Self {
            ok: AtomicBool::new(false),
            ret_val: AtomicU64::new(0),
        }
    }

    pub fn signal(&self, ret_val: u64) {
        self.ret_val.store(ret_val, Ordering::Release);
        self.ok.store(true, Ordering::Release);
    }

    pub fn wait(&self) -> u64 {
        while !self.ok.load(Ordering::Acquire) {
            CurrentTask::get().yield_now();
        }
        self.ret_val.load(Ordering::Acquire)
    }
}

impl SCF {
    fn send_request(&mut self, opcode: ScfOpcode, args: [u64; 4], token: ScfRequestToken) {
        while !self.queue().send(opcode, args, token) {
            CurrentTask::get().yield_now();
        }
        super::notify(self.irq_num());
    }

    fn send_request_kernel(&mut self, opcode: ScfOpcode, args: [u64; 4], token: ScfRequestToken) {
        while !self.queue().send(opcode, args, token) {
            core::hint::spin_loop();
        }
        super::notify(self.irq_num());
    }

    pub fn write(&mut self, fd: isize, buf: *const u8, len: usize) -> isize {
        debug!("sys_write: fd={}, buf={:#x}, len={}, slot={}", fd, buf as usize, len, self.slot_num);
        assert!(len < CHUNK_SIZE);
        let cond = SyscallCondVar::new();
        self.send_request(
            ScfOpcode::Write,
            [fd as _, buf as _, len as _, 0],
            ScfRequestToken::from(&cond),
        );
        let ret = cond.wait();
        ret as _
    }

    pub fn read(&mut self, fd: isize, buf: *mut u8, len: usize) -> isize {
        debug!("sys_read: fd={}, buf={:#x}, len={}, slot={}", fd, buf as usize, len, self.slot_num);
        // assert!(len < CHUNK_SIZE);
        if fd < 0 {
            return fd;
        }
        let cond = SyscallCondVar::new();
        self.send_request(
            ScfOpcode::Read,
            [fd as _, buf as _, len as _, 0],
            ScfRequestToken::from(&cond),
        );
        let ret = cond.wait();
        debug!("sys_read: ret={}", ret);
        ret as _
    }

    pub fn open(&mut self, path: *const u8, flags: usize, mode: usize) -> isize {
        debug!("sys_open: path={:#x}, flags={:#x}, mode={:#x}, slot={}", path as usize, flags, mode, self.slot_num);
        let cond = SyscallCondVar::new();
        self.send_request(
            ScfOpcode::Open,
            [path as _, flags as _, mode as _, 0],
            ScfRequestToken::from(&cond),
        );
        let ret = cond.wait();
        ret as _
    }

    pub fn close(&mut self, fd: isize) -> isize {
        debug!("sys_close: fd={}, slot={}", fd, self.slot_num);
        let cond = SyscallCondVar::new();
        self.send_request(
            ScfOpcode::Close,
            [fd as _, 0, 0, 0],
            ScfRequestToken::from(&cond),
        );
        let ret = cond.wait();
        ret as _
    }

    pub fn syncmap(&mut self, vaddr: usize, len: usize, paddr: usize, prot: usize) -> isize {
        debug!("sys_syncmap: vaddr={:#x}, len={:#x}, paddr={:#x}, prot={:#x}, slot={}", vaddr, len, paddr, prot, self.slot_num);
        let cond = SyscallCondVar::new();
        self.send_request_kernel(
            ScfOpcode::SyncMap,
            [vaddr as _, len as _, paddr as _, prot as _],
            ScfRequestToken::from(&cond),
        );

        // Better waiting strategy?
        loop {
            let response = self.queue().pop_response();
            if response.is_some() {
                let scf_response = response.unwrap();
                let ret = scf_response.ret_val;
                debug!("sys_syncmap: response received: ret={:#x}", ret);
                return ret as _;
            }
        }
    }

    pub fn syncunmap(&mut self, vaddr: usize, len: usize) -> isize {
        debug!("sys_syncunmap: vaddr={:#x}, len={:#x}, slot={}", vaddr, len, self.slot_num);
        let cond = SyscallCondVar::new();
        self.send_request_kernel(
            ScfOpcode::SyncUnmap,
            [vaddr as _, len as _, 0, 0],
            ScfRequestToken::from(&cond),
        );

        // Better waiting strategy?
        loop {
            let response = self.queue().pop_response();
            if response.is_some() {
                let scf_response = response.unwrap();
                let ret = scf_response.ret_val;
                debug!("sys_syncunmap: response received: ret={:#x}", ret);
                return ret as _;
            }
        }
    }

    pub fn syncfork(&mut self) -> isize {
        debug!("sys_syncfork: slot={}", self.slot_num);
        let cond = SyscallCondVar::new();
        self.send_request(
            ScfOpcode::Fork,
            [0; 4],
            ScfRequestToken::from(&cond),
        );

        // Better waiting strategy?
        loop {
            let response = self.queue().pop_response();
            if response.is_some() {
                let scf_response = response.unwrap();
                let ret = scf_response.ret_val;
                debug!("sys_syncfork: response received: ret={}", ret);
                return ret as _;
            }
        }
    }

    pub fn stat(&mut self, path: *const u8) -> isize {
        debug!("sys_stat: path={:#x}, slot={}", path as usize, self.slot_num);
        let cond = SyscallCondVar::new();
        self.send_request(
            ScfOpcode::Stat,
            [path as _, 0, 0, 0],
            ScfRequestToken::from(&cond),
        );
        let ret = cond.wait();
        debug!("sys_stat: ret={}", ret);
        ret as _
    }

    pub fn exec(&mut self, path: *const u8) -> Option<Vec<u8>> {
        debug!("sys_exec: path={:#x}, slot={}", path as usize, self.slot_num);

        // Use stat to aquire size of the file
        let size = self.stat(path);
        if size <= 0 || size as usize > KERNEL_HEAP_SIZE {
            return None;
        }

        let size = size as usize;

        // Open file
        let fd = self.open(path, 0, 0);
        if fd < 0 {
            return None;
        }

        let mut data = Vec::<u8>::with_capacity(size);
        unsafe { data.set_len(size);}

        let buf = data.as_mut_ptr();

        // Read file
        let read = self.read(fd, buf, size);
        if read as usize != size {
            return None;
        }
        
        // Close file
        self.close(fd as _);

        Some(data)
    }

    pub fn clone_(&mut self) -> isize {
        debug!("sys_clone: slot={}", self.slot_num);
        let cond = SyscallCondVar::new();
        self.send_request(
            ScfOpcode::Clone,
            [0; 4],
            ScfRequestToken::from(&cond),
        );
        let ret = cond.wait();
        ret as _
    }

    pub fn exit(&mut self) -> isize {
        debug!("sys_exit: slot={}", self.slot_num);
        let cond = SyscallCondVar::new();
        self.send_request(
            ScfOpcode::Exit,
            [0; 4],
            ScfRequestToken::from(&cond),
        );
        let ret = cond.wait();
        get_queue(self.slot_num).reset(); // Or to reset in linux?
        ret as _
    }
}
