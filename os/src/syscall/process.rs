//! Process management syscalls
//!
use alloc::sync::Arc;

use crate::{
    config::MAX_SYSCALL_NUM,
    fs::{open_file, OpenFlags},
    mm::{translated_refmut, translated_str, PageTable, VirtAddr, PhysAddr, VirtPageNum},
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next, TaskStatus,
        mmap_current, munmap_current, get_task_info,
    },
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
/// 进程运行时间
pub struct TimeVal {
    /// 秒
    pub sec: usize,
    /// 微秒
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    pub status: TaskStatus,
    /// The numbers of syscall called by task
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    pub time: usize,
}

/// 进程退出
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// 进程让出cpu
pub fn sys_yield() -> isize {
    //trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// 获取进程号
pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

/// fork
pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

/// exec
pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        task.exec(all_data.as_slice());
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    //trace!("kernel: sys_waitpid");
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_get_time NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let us = get_time_us();
    //println!("us: {}", us);
    let p = v_to_p(_ts);
    unsafe{
        *p = TimeVal {
            sec: us / 1_000_000,
            usec: us % 1_000_000,    
        };
    }
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    trace!(
        "kernel:pid[{}] sys_task_info NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    // let p: *mut TaskInfo = v_to_p(_ti);
    get_task_info();
    0
}

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_mmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let start_va: VirtAddr = _start.into();

    if !start_va.aligned() {
        debug!("Map failed: address not aligned");
        return -1
    }

    if _port & !0x7 != 0  || _port & 0x7 == 0 {
        return -1
    }

    let end_va: VirtAddr = VirtAddr(_start + _len);
    mmap_current(start_va, end_va, _port)
}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_munmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let start_va: VirtAddr = _start.into();
    if !start_va.aligned() {
        debug!("Unmap failed: address not aligned");
        return -1
    }

    let end_va: VirtAddr = VirtAddr(_start + _len);
    munmap_current(start_va, end_va)
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(_path: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_spawn NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let current_task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, _path);

    if let Some(inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let data = inode.read_all();

        let new_task = current_task.fork();
        new_task.exec(data.as_slice());
        let new_pid = new_task.pid.0;
    
        // add new task to scheduler
        add_task(new_task);
        new_pid as isize    
    } else {
        -1
    }

}

/// YOUR JOB: Set task priority.
pub fn sys_set_priority(_prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    if _prio >= 2 {
        let task = current_task().unwrap();
        task.inner_exclusive_access().priority = _prio as usize;
        _prio
    }
    else {
        -1
    }
}


/// 虚拟地址转换成物理地址
fn v_to_p<T>(user_va: *const T) -> *mut T {
    // 获取当前进程页表
    let page_table = PageTable::from_token(current_user_token());
    // 计算出vpn
    let vpn: VirtPageNum = VirtAddr(user_va as usize).floor();
    // 计算出offset
    let offset: usize = VirtAddr(user_va as usize).page_offset();
    // 通过页表找出ppn
    let ppn: PhysAddr = page_table.translate(vpn).unwrap().ppn().into();
    // 转换成usize
    let user_ppn: usize = ppn.into();
    // 加上offset形成物理地址
    let user_pa: *mut T = (user_ppn + offset) as *mut T;
    user_pa
}