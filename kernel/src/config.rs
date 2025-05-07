pub use crate::arch::config::*;
pub use crate::platform::config::*;

// Memory size

pub const BOOT_KERNEL_STACK_SIZE: usize = 4096 * 4; // 16K
pub const USER_STACK_SIZE: usize = 4096 * 4; // 16K

#[cfg(feature = "rvm")]
pub const USER_STACK_BASE: usize = 0x1000_0000 - USER_STACK_SIZE;

#[cfg(not(feature = "rvm"))]
pub const USER_STACK_BASE: usize = 0x7fff_0000_0000 - USER_STACK_SIZE;

pub const KERNEL_STACK_SIZE: usize = 4096 * 4; // 16K
pub const KERNEL_HEAP_SIZE: usize = 0x40_0000; // 4M

pub const UINTR_MAX_UITT_NR: usize = 16;
pub const UPID_SHARE_MEM_SIZE: usize = 0x2000; // 8K
pub const UPID_NIMBOS_MEM_SIZE: usize = 0x1000; // 4K
pub const UPID_MEM_OFFSET: usize =  0xffff_ff90_0000_0000;
pub const UPID_SHARE_MEM_PHYS_END: usize = 0x4200_0000;
pub const UPID_NIMBOS_MEM_PHYS_START: usize = UPID_SHARE_MEM_PHYS_END - UPID_NIMBOS_MEM_SIZE;
pub const UPID_SHARE_MEM_PHYS_START: usize =  UPID_SHARE_MEM_PHYS_END - UPID_SHARE_MEM_SIZE;
pub const UPID_SHARE_MEM_VIRT_START: usize = UPID_SHARE_MEM_PHYS_START + UPID_MEM_OFFSET;
pub const UPID_SHARE_MEM_VIRT_END: usize = UPID_SHARE_MEM_PHYS_END + UPID_MEM_OFFSET;
pub const UPID_NIMBOS_MEM_VIRT_START: usize = UPID_NIMBOS_MEM_PHYS_START + UPID_MEM_OFFSET;
pub const UPID_NIMBOS_MEM_VIRT_END: usize = UPID_SHARE_MEM_VIRT_END;

// SMP

pub const MAX_CPUS: usize = 1;

// Timer

pub const TICKS_PER_SEC: u64 = 100;

// Syscall IPC

cfg_if! {
    if #[cfg(feature = "rvm")] {
        pub const PHYS_MEMORY_END: usize = PHYS_MEMORY_BASE + PHYS_MEMORY_SIZE
            - scf::SYSCALL_QUEUE_BUF_SIZE * scf::SYSCALL_MAX_SLOT_NUM;

        pub mod scf {
            pub const SYSCALL_IPI_IRQ_NUM: usize = 13;

            // pub const SYSCALL_DATA_BUF_SIZE: usize = 0x10_0000; // 1M
            pub const SYSCALL_QUEUE_BUF_SIZE: usize = 4096; // 4K
            pub const SYSCALL_MAX_SLOT_NUM: usize = 4;

            // pub const SYSCALL_DATA_BUF_PADDR: usize = super::PHYS_MEMORY_END;
            pub const SYSCALL_QUEUE_BUF_BASE_PADDR: usize = super::PHYS_MEMORY_END;
        }
    } else {
        pub const PHYS_MEMORY_END: usize = PHYS_MEMORY_BASE + PHYS_MEMORY_SIZE;
    }
}
