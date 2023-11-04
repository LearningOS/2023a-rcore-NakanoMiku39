//! Process management syscalls
use crate::{
    config::{MAX_SYSCALL_NUM, },
    mm::VirtAddr,
    task::{
        change_program_brk, exit_current_and_run_next, suspend_current_and_run_next, TaskStatus, //current_user_token,
        mmap_current, munmap_current,
    },
    //timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    /*
    let us = get_time_us();
    
    unsafe{
        let sec_va = &((*_ts).sec) as *const usize;
        let usec_va = &((*_ts).usec) as *const usize;
        let sec_pa = v_to_p(sec_va as usize) as *mut usize;
        let usec_pa = v_to_p(usec_va as usize) as *mut usize;
        *sec_pa = us / 1_000_000;
        *usec_pa = us % 1_000_000;
    }
    */
    0
}


/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    // 参数指针来源于用户空间，是虚拟地址，要转换成物理地址
    // let status_pa = &(VtoP((*_ti).status)) as *const TaskStatus;
    // let time_va = &(VtoP((*_ti).time)) as *mut usize;
    
    trace!("kernel: sys_task_info NOT IMPLEMENTED YET!");
    0
}


// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    // 先检查权限，然后找到当前任务，最后映射
    let start_va: VirtAddr = _start.into();

    if !start_va.aligned() {
        debug!("Map failed: address not aligned");
        return -1
    }

    if _port & !0x7 != 0  || _port & 0x7 == 0 {
        return -1
    }

    if _len == 0 {
        return 0
    }

    let end_va: VirtAddr = VirtAddr(_start + _len);
    mmap_current(start_va, end_va, _port)
}


// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    let start_va: VirtAddr = _start.into();
    if !start_va.aligned() {
        debug!("Unmap failed: address not aligned");
        return -1
    }

    if _len == 0 {
        return 0
    }

    let end_va: VirtAddr = VirtAddr(_start + _len);
    munmap_current(start_va, end_va)
}


/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/*
/// 虚拟地址转换成物理地址
fn v_to_p(user_va: usize) -> usize {
    let page_table = PageTable::from_token(current_user_token());
    let vpn = VirtAddr(user_va).floor();
    let offset = VirtAddr(user_va).page_offset();
    let ppn = page_table.translate(vpn).unwrap().ppn();
    ppn.0 << PAGE_SIZE_BITS + offset
}
*/