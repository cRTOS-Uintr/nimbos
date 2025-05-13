use crate::task::CurrentTask;
use crate::config::{UINTR_MAX_UITT_NR, UPID_NIMBOS_MEM_VIRT_START, UPID_NIMBOS_MEM_VIRT_END};
use alloc::sync::Arc;
use crate::task::manager::TASK_MANAGER;
use crate::sync::Mutex;
use alloc::boxed::Box;
use crate::arch::TaskContext;
use core::arch::asm;
use core::ptr::NonNull;
use core::marker::PhantomData;
use core::ops::Deref;
use core::ops::DerefMut;
use crate::drivers::interrupt::apic::get_apic_id;

pub const UINTR_UITT_MASK_WORDS: usize = (UINTR_MAX_UITT_NR + 63) / 64;

// TODO: move this to arch
const MSR_IA32_UINTR_HANDLER: u32 = 0x986;
const MSR_IA32_UINTR_STACKADJUST: u32 = 0x987;
const MSR_IA32_UINTR_MISC: u32 = 0x988;
const MSR_IA32_UINTR_PD: u32 = 0x989;
const MSR_IA32_UINTR_TT: u32 = 0x98a;

const OS_ABI_REDZONE: u64 = 0;
pub const UINTR_NOTIFICATION_VECTOR: u8 = 0xec;
pub const EFAULT: isize = 14;
pub const EBUSY: isize = 16;
pub const EINVAL: isize = 22;
pub const ENOSPC: isize = 28;
pub const EOPNOTSUPP: isize = 95;

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct UintrNc {
    status: u8, // bit 0: ON, bit 1: SN, bit 2-7: reserved
    reserved1: u8, // Reserved
    nv: u8, // Notification vector
    reserved2: u8, // Reserved
    ndst: u32, // Notification destination
} // Notification control

#[repr(C, align(64))]
#[derive(Debug, Copy, Clone)]
pub struct UintrUpid {
    pub nc: UintrNc,
    pub puir: u64,
}

// 固定内存区域
const UPID_MEM_START: usize = UPID_NIMBOS_MEM_VIRT_START;
const UPID_MEM_END: usize = UPID_NIMBOS_MEM_VIRT_END;
const BLOCK_SIZE: usize = core::mem::size_of::<UintrUpid>();
const MAX_BLOCKS: usize = (UPID_MEM_END - UPID_MEM_START) / BLOCK_SIZE;

// 专用内存分配器
pub struct UintrAllocator {
    used_blocks: [bool; MAX_BLOCKS], // 跟踪每个块是否被使用
}

#[allow(dead_code)]
impl UintrAllocator {
    /// 创建新的分配器，初始化所有块为未使用状态
    pub const fn new() -> Self {
        UintrAllocator {
            used_blocks: [false; MAX_BLOCKS],
        }
    }

    /// 分配一个UintrUpid结构
    /// 返回NonNull指针，方便后续使用
    pub fn allocate(&mut self) -> Option<NonNull<UintrUpid>> {
        for (i, used) in self.used_blocks.iter_mut().enumerate() {
            if !*used {
                *used = true;
                let addr = UPID_MEM_START + i * BLOCK_SIZE;
                // 清0
                unsafe {
                    core::ptr::write_bytes(addr as *mut UintrUpid, 0, 1);
                }
                // 安全保证：地址对齐且在我们控制的范围内
                unsafe {
                    return Some(NonNull::new_unchecked(addr as *mut UintrUpid));
                }
            }
        }
        None
    }

    /// 释放一个UintrUpid结构
    /// 参数必须是之前通过allocate分配的指针
    pub fn deallocate(&mut self, ptr: NonNull<UintrUpid>) {
        let addr = ptr.as_ptr() as usize;
        
        // 检查指针是否在有效范围内
        if addr < UPID_MEM_START || addr >= UPID_MEM_END {
            warn!("Out of range UPID freed");
            return;
        }
        
        // 检查是否对齐
        if (addr - UPID_MEM_START) % BLOCK_SIZE != 0 {
            warn!("Unaligned UPID freed");
            return;
        }
        
        let index = (addr - UPID_MEM_START) / BLOCK_SIZE;
        if index < MAX_BLOCKS {
            self.used_blocks[index] = false;
        }
    }

    /// 获取当前已分配的块数
    pub fn allocated_count(&self) -> usize {
        self.used_blocks.iter().filter(|&&used| used).count()
    }

    /// 获取剩余可分配的块数
    pub fn available_count(&self) -> usize {
        MAX_BLOCKS - self.allocated_count()
    }
}

static UPID_ALLOCATOR: Mutex<UintrAllocator> = Mutex::new(UintrAllocator {
    used_blocks: [false; MAX_BLOCKS],
});

// 公共接口
pub fn alloc_uintr_upid() -> Option<NonNull<UintrUpid>> {
    UPID_ALLOCATOR.lock().allocate()
}

pub fn free_uintr_upid(ptr: NonNull<UintrUpid>) {
    UPID_ALLOCATOR.lock().deallocate(ptr)
}

#[allow(dead_code)]
pub fn allocated_count() -> usize {
    UPID_ALLOCATOR.lock().allocated_count()
}

#[allow(dead_code)]
pub fn available_count() -> usize {
    UPID_ALLOCATOR.lock().available_count()
}

// 线程安全的智能指针
#[derive(Debug)]
pub struct UintrBox {
    ptr: NonNull<UintrUpid>,
    // 添加标记表示所有权
    _marker: PhantomData<*mut ()>, // 表示 !Send + !Sync
}

// 明确实现 Send 和 !Sync
unsafe impl Send for UintrBox {}  // 可以跨线程转移所有权

#[allow(dead_code)]
impl UintrBox {
    pub fn new() -> Option<Self> {
        alloc_uintr_upid().map(|ptr| UintrBox {
            ptr,
            _marker: core::marker::PhantomData,
        })
    }

    pub fn into_raw(this: Self) -> NonNull<UintrUpid> {
        let ptr = this.ptr;
        core::mem::forget(this);
        ptr
    }

    pub unsafe fn from_raw(ptr: NonNull<UintrUpid>) -> Self {
        UintrBox {
            ptr,
            _marker: core::marker::PhantomData,
        }
    }
}

impl Deref for UintrBox {
    type Target = UintrUpid;

    fn deref(&self) -> &Self::Target {
        unsafe { self.ptr.as_ref() }
    }
}

impl DerefMut for UintrBox {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.ptr.as_mut() }
    }
}

impl Drop for UintrBox {
    fn drop(&mut self) {
        // 使用全局锁释放内存
        free_uintr_upid(self.ptr);
    }
}

#[derive(Debug)]
pub struct UintrUpidCtx {
    // task: Arc<Task>, // Receiver task
    // uvec_mask: u64, // track registered vectors per bit
    #[cfg(feature = "rvm")]
    pub upid: UintrBox,
    #[cfg(not(feature = "rvm"))]
    pub upid: Box<UintrUpid>,
    // receiver_active: bool, // Flag for UPID being mapped to a receiver
    // waiting: bool, // Flag for UPID blocked in the kernel
    // waiting_cost: u32, // Flags for who pays the waiting cost
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
/* User Interrupt Target Table Entry (UITTE) */
pub struct UnalignedUintrUittEntry {
    pub valid: u8, // bit 0: valid, bit 1-7: reserved
    pub user_vec: u8,
    pub reserved: [u8; 6],
    pub target_upid_addr: u64,
}

#[repr(align(16))]
#[derive(Debug, Copy, Clone)]
pub struct UintrUittEntry(pub UnalignedUintrUittEntry);

#[derive(Debug, Clone)]
pub struct UintrUittCtx {
    pub uitt: [UintrUittEntry; UINTR_MAX_UITT_NR],
    pub uitt_mask: BitSet<UINTR_MAX_UITT_NR, UINTR_UITT_MASK_WORDS>,
}

// 位集合实现
#[derive(Debug, Clone)]
pub struct BitSet<const N: usize, const M: usize> {
    bits: [u64; M],
}

impl<const N: usize, const M: usize> BitSet<N, M> {
    pub const fn new() -> Self {
        Self {
            bits: [0; M],
        }
    }

    // 设置指定位
    #[allow(dead_code)]
    pub fn set(&mut self, index: usize) {
        assert!(index < N);
        let word = index / 64;
        let bit = index % 64;
        self.bits[word] |= 1 << bit;
    }

    // 清除指定位
    #[allow(dead_code)]
    pub fn clear(&mut self, index: usize) {
        assert!(index < N);
        let word = index / 64;
        let bit = index % 64;
        self.bits[word] &= !(1 << bit);
    }

    // 测试指定位是否设置
    #[allow(dead_code)]
    pub fn test(&self, index: usize) -> bool {
        assert!(index < N);
        let word = index / 64;
        let bit = index % 64;
        (self.bits[word] & (1 << bit)) != 0
    }

    // 查找第一个未设置的位
    #[allow(dead_code)]
    pub fn find_first_zero(&self) -> Option<usize> {
        for (word_idx, &word) in self.bits.iter().enumerate() {
            if word != u64::MAX {
                let bit_idx = word.trailing_ones() as usize;
                let index = word_idx * 64 + bit_idx;
                if index < N {
                    return Some(index)
                }
            }
        }
        None
    }

    // 位集合是否全为0
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.bits.iter().all(|&x| x == 0)
    }

    // 位集合是否全为1
    #[allow(dead_code)]
    pub fn is_full(&self) -> bool {
        self.bits.iter().take(N / 64).all(|&x| x == u64::MAX) &&
        if N % 64 != 0 {
            self.bits[N / 64] == (1 << (N % 64)) - 1
        } else {
            true
        }
    }
}

fn uintr_init_sender() {
    let current_task = CurrentTask::get().0;
    let mut ctx = unsafe{&mut *current_task.context().as_ptr()};
    if let None = ctx.uitt {
        warn!("Initializing sender");
        ctx.uitt = Some(Arc::new(Mutex::new(UintrUittCtx {
            uitt: [UintrUittEntry(UnalignedUintrUittEntry {
                valid: 0,
                user_vec: 0,
                reserved: [0; 6],
                target_upid_addr: 0,
            }); UINTR_MAX_UITT_NR],
            uitt_mask: BitSet::<UINTR_MAX_UITT_NR, UINTR_UITT_MASK_WORDS>::new(),
        })));
    }
}

#[inline]
pub fn genmask_ull(h: u64, l: u64) -> u64 {
    if h >= 64 || l >= 64 || h < l {
        panic!("Invalid input for genmask_ull: h={}, l={}", h, l);
    }
    (!0_u64).wrapping_shl(l as u32) & (!0_u64).wrapping_shr((63 - h) as u32)
}

fn uintr_set_sender_msrs(ctx: &mut TaskContext) {
    let uitt_ptr = ctx.uitt.as_ref().unwrap().lock().uitt.as_ptr() as usize;

    warn!("Setting MSRs for sender");
    unsafe {
        // Write to MSR_IA32_UINTR_TT
        asm!(
            "wrmsr",
            in("ecx") MSR_IA32_UINTR_TT,
            in("rax") uitt_ptr as u32 | 1,
            in("rdx") (uitt_ptr >> 32) as u32,
            options(nostack, nomem),
        );

        // Read from MSR_IA32_UINTR_MISC
        let mut msr64_low: u32;
        let mut msr64_high: u32;
        asm!(
            "rdmsr",
            out("eax") msr64_low,
            out("edx") msr64_high,
            in("ecx") MSR_IA32_UINTR_MISC,
            options(nostack, nomem),
        );

        // Modify msr64_low and msr64_high directly
        msr64_high &= (genmask_ull(63, 32) >> 32) as u32; // Keep high 32 bits of high part
        msr64_low |= (UINTR_MAX_UITT_NR - 1) as u32;    // Set low 32 bits of low part

        // Write back to MSR_IA32_UINTR_MISC
        asm!(
            "wrmsr",
            in("ecx") MSR_IA32_UINTR_MISC,
            in("rax") msr64_low,
            in("rdx") msr64_high,
            options(nostack, nomem),
        );
    }
    warn!("MSRs set for sender");

    ctx.uitt_activated = true;
}

fn do_uintr_register_sender(uvec: usize, upid: *mut UintrUpid) -> isize {
    if upid.is_null() {
        warn!("Invalid UPID address");
        return -EINVAL;
    }

    uintr_init_sender();
    warn!("Registering sender: uvec={:#x}, upid={:#x}", uvec, upid as usize);
    warn!("UPID content: {:#?}", unsafe{*(upid as *const UintrUpid)});
    let current_task = CurrentTask::get().0;
    let ctx = unsafe{&mut *current_task.context().as_ptr()};
    let uitt_ctx = ctx.uitt.as_mut().unwrap();
    let mut uitt_ctx = uitt_ctx.lock();
    if let Some(entry) = uitt_ctx.uitt_mask.find_first_zero() {
        let uitt_entry = &mut uitt_ctx.uitt[entry].0;
        uitt_entry.valid = 1;
        uitt_entry.user_vec = (uvec & 0xFF) as u8;
        uitt_entry.target_upid_addr = upid as u64;
        drop(uitt_entry);
        uitt_ctx.uitt_mask.set(entry);
        warn!("UITT entry {} registered", entry);
    
        if !ctx.uitt_activated {
            drop(uitt_ctx);
            uintr_set_sender_msrs(ctx);
        }

        entry as isize
    } else {
        warn!("No available UITT entry");
        -ENOSPC
    }
}

pub fn sys_uintr_register_sender(upid_addr: u64, uvec: usize) -> isize {
    let _manager = TASK_MANAGER.lock();
    warn!("sys_uintr_register_sender called");
    do_uintr_register_sender(uvec, upid_addr as *mut UintrUpid)
}

fn do_uintr_register_handler(handler: u64) -> u64 {
    // TODO: check validity
    let current_task = CurrentTask::get().0;
    let ctx = unsafe{&mut *current_task.context().as_ptr()};

    if ctx.upid_activated {
        warn!("UPID already activated");
        // return -EBUSY as u64
        return 0
    }

    if let None = ctx.uintr_upid_ctx {
        #[cfg(feature = "rvm")]
        {
            ctx.uintr_upid_ctx = Some(
                Box::new(UintrUpidCtx {
                    upid: UintrBox::new().unwrap(),
                })
            );
        }
        #[cfg(not(feature = "rvm"))]
        {
            ctx.uintr_upid_ctx = Some(Box::new(UintrUpidCtx {
                // uvec_mask: 0,
                upid: Box::new(UintrUpid {
                    nc: UintrNc {
                        status: 0,
                        reserved1: 0,
                        nv: 0,
                        reserved2: 0,
                        ndst: 0,
                    },
                    puir: 0,
                }),
                // receiver_active: true,
            }));
        }
        warn!("UPID initialized");
    } else {
        warn!("Handler already registered");
    }
    warn!("Registering handler: {:#x}", handler);
    let upid: &mut UintrUpid = ctx.uintr_upid_ctx.as_mut().unwrap().upid.deref_mut();
    upid.nc.nv = UINTR_NOTIFICATION_VECTOR as u8;
    upid.nc.ndst = get_apic_id() << 8;

    let upid_addr = (&*ctx.uintr_upid_ctx.as_ref().unwrap().upid) as *const UintrUpid as u64;

    let handler_low: u32 = handler as u32;
    let handler_high: u32 = (handler >> 32) as u32;
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") MSR_IA32_UINTR_HANDLER,
            in("eax") handler_low,
            in("edx") handler_high,
            options(nostack, nomem) // Basic options: no stack manipulation, no memory access besides MSR
        );
    }

    // Write UPID physical address to MSR_IA32_UINTR_PD
    // Assume upid_addr (u64) holds the correct physical address from the preparation step.
    let upid_addr_low: u32 = upid_addr as u32;
    let upid_addr_high: u32 = (upid_addr >> 32) as u32;
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") MSR_IA32_UINTR_PD,
            in("eax") upid_addr_low,
            in("edx") upid_addr_high,
            options(nostack, nomem)
        );
    }

    // Write stack adjustment value to MSR_IA32_UINTR_STACKADJUST
    let stackadjust_low: u32 = OS_ABI_REDZONE as u32;
    let stackadjust_high: u32 = (OS_ABI_REDZONE >> 32) as u32;
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") MSR_IA32_UINTR_STACKADJUST,
            in("eax") stackadjust_low,
            in("edx") stackadjust_high,
            options(nostack, nomem)
        );
    }

    let mut misc_msr_low: u32;
    let mut misc_msr_high: u32;

    // 1. Read current MSR value into high and low parts
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") MSR_IA32_UINTR_MISC,
            out("eax") misc_msr_low,
            out("edx") misc_msr_high,
            options(nostack, readonly) // Keep options as corrected before
        );
    }

    misc_msr_high |= UINTR_NOTIFICATION_VECTOR as u32;

    // 3. Write the original low part and the modified high part back
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") MSR_IA32_UINTR_MISC,
            in("eax") misc_msr_low,     // Pass the original low part back
            in("edx") misc_msr_high,    // Pass the modified high part back
            options(nostack, nomem)     // Keep options as before for wrmsr
        );
    }

    ctx.upid_activated = true;
    warn!("Handler registered");
    upid_addr
}

// fn my_kernel_handler() {
//     warn!("Kernel handler called");
// }

pub fn sys_uintr_register_handler(handler: u64) -> u64 {
    // register_handler(UINTR_NOTIFICATION_VECTOR as usize, || {
    //     my_kernel_handler();
    //     IrqHandlerResult::Reschedule
    // });
    let _manager = TASK_MANAGER.lock();
    warn!("sys_uintr_register_handler called");
    if handler == 0 {
        // return -EFAULT as u64;
        return 0
    }
    do_uintr_register_handler(handler)
}

#[cfg(not(feature = "rvm"))]
pub fn sys_init_cross_uintr(_upid_addr: u64, _desc_addr: u64) -> usize {
    warn!("sys_init_cross_uintr called but rvm is off");
    0
}