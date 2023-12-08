use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;
use alloc::vec;
/// sleep syscall
pub fn sys_sleep(ms: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_sleep",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}
/// mutex create syscall
pub fn sys_mutex_create(blocking: bool) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;

        process_inner.mutex_available[id] = 1;
        if process_inner.mutex_need.is_empty() {
            let mutex_count = process_inner.mutex_list.len();
            process_inner.mutex_allocation.push(vec![0; mutex_count]);
            process_inner.mutex_need.push(vec![0; mutex_count]);
        }
        for i in process_inner.mutex_allocation.iter_mut() { i[id] = 0; };
        for i in process_inner.mutex_need.iter_mut() { i[id] = 0; };

        id as isize
    } else {
        if process_inner.mutex_need.is_empty() {
            let mutex_count = process_inner.mutex_list.len();
            process_inner.mutex_allocation.push(vec![0; mutex_count]);
            process_inner.mutex_need.push(vec![0; mutex_count]);
        }
        process_inner.mutex_list.push(mutex);
        process_inner.mutex_available.push(1);
        let id = process_inner.mutex_list.len() as isize - 1;

        for i in process_inner.mutex_allocation.iter_mut() { i.push(0); }
        for i in process_inner.mutex_need.iter_mut() { i.push(0); }

        id as isize
    }
}
/// mutex lock syscall
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_lock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();

    // Need[i,j] = Max[i,j] - mutex_allocation[i, j]
    // 如果需要分配
    // Available[j] = Available[j] - Request[i,j];
    // Allocation[i,j] = Allocation[i,j] + Request[i,j];
    // Need[i,j] = Need[i,j] - Request[i,j];
    if process_inner.deadlock_detect {
        let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
        process_inner.mutex_need[tid][mutex_id] += 1;
        let task_count = process_inner.tasks.len();
        // 安全性算法
        // 设置两个向量:工作向量Work，表示操作系统可提供给线程继续运行所需的各类资源数目，
        // 它含有m个元素，初始时，Work = Available；结束向量Finish，表示系统是否有足够的资源分配给线程，使之运行完成。
        // 初始时 Finish[0..n-1] = false，表示所有线程都没结束；当有足够资源分配给线程时，设置Finish[i] = true
        // 第一步
        let mut finish = vec![false; task_count];
        let mut work = process_inner.mutex_available.clone();
        loop {
            let mut is_safe = true;
            for i in 0..process_inner.mutex_need.len() {
                if !finish[i] { 
                    is_safe = false; 
                    break;
                }
            }

            // 第四步
            // 如果finish里面全是true说明安全了
            if is_safe { break; }
            
            for i in 0..process_inner.mutex_need.len() {
                //第二步
                if process_inner.mutex_need[tid][i] <= work[i] {
                    // 第三步
                    work[i] += process_inner.mutex_allocation[tid][i];
                    finish[i] = true;
                    is_safe = true;
                } else { is_safe = false; break; }
            } 
            if !is_safe { break; }   
              
        }

        for i in finish {
            if !i {
                return -0xDEAD;
            }
        }     
       
    }

    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    mutex.lock();

    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
    process_inner.mutex_available[mutex_id] -= 1;
    process_inner.mutex_need[tid][mutex_id] -= 1;
    process_inner.mutex_allocation[tid][mutex_id] += 1;

    0
}
/// mutex unlock syscall
pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_mutex_unlock",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();

    let tid: usize = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
    // 释放资源不会导致死锁
    process_inner.mutex_available[mutex_id] += 1;
    process_inner.mutex_allocation[tid][mutex_id] -= 1;

    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    mutex.unlock();
    0
}
/// semaphore create syscall
pub fn sys_semaphore_create(res_count: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        process_inner.semaphore_available[id] = res_count;
        if process_inner.semaphore_need.is_empty() {
            let sem_count = process_inner.semaphore_list.len();
            process_inner.semaphore_allocation.push(vec![0; sem_count]);
            process_inner.semaphore_need.push(vec![0; sem_count]);
        }
        for i in process_inner.semaphore_allocation.iter_mut() { i[id] = 0; };
        for i in process_inner.semaphore_need.iter_mut() { i[id] = 0; };

        id
    } else {
        if process_inner.semaphore_need.is_empty() {
            let sem_count = process_inner.semaphore_list.len();
            process_inner.semaphore_allocation.push(vec![0; sem_count]);
            process_inner.semaphore_need.push(vec![0; sem_count]);
        }
        
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));

        process_inner.semaphore_available.push(res_count);
        for i in process_inner.semaphore_allocation.iter_mut() { i.push(0); };
        for i in process_inner.semaphore_need.iter_mut() { i.push(0); };
    
        process_inner.semaphore_list.len() - 1
    };
    id as isize
}
/// semaphore up syscall
pub fn sys_semaphore_up(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_up",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());

    let tid: usize = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
    // 释放资源不会导致死锁
    process_inner.semaphore_available[sem_id] += 1;
    process_inner.semaphore_allocation[tid][sem_id] -= 1;

    drop(process_inner);
    sem.up();
    0
}
/// semaphore down syscall
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_semaphore_down",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());

        // Need[i,j] = Max[i,j] - mutex_allocation[i, j]
    // 如果需要分配
    // Available[j] = Available[j] - Request[i,j];
    // Allocation[i,j] = Allocation[i,j] + Request[i,j];
    // Need[i,j] = Need[i,j] - Request[i,j];
    if process_inner.deadlock_detect {
        let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
        process_inner.semaphore_need[tid][sem_id] += 1;
        let task_count = process_inner.tasks.len();
        
        // 安全性算法
        // 设置两个向量:工作向量Work，表示操作系统可提供给线程继续运行所需的各类资源数目，
        // 它含有m个元素，初始时，Work = Available；结束向量Finish，表示系统是否有足够的资源分配给线程，使之运行完成。
        // 初始时 Finish[0..n-1] = false，表示所有线程都没结束；当有足够资源分配给线程时，设置Finish[i] = true
        // 第一步
        let mut finish = vec![false; task_count];
        let mut work = process_inner.semaphore_available.clone();
        loop {  
            let mut is_safe = true;      
            for i in 0..process_inner.semaphore_need.len() {
                if !finish[i] {
                    let mut is_safe_2 = true;
                    for j in 0..work.len() {
                        //第二步
                        if process_inner.semaphore_need[i][j] > work[j] {
                            is_safe_2 = false; 
                            break; 
                        } 
                    }
                    if is_safe_2 {
                        is_safe = false;
                        finish[i] = true;
                        for j in 0..work.len() {
                            // 第三步
                            work[j] += process_inner.semaphore_allocation[i][j];
                        }
                    }
                }
            }

            // 第四步
            // 如果finish里面全是true说明安全了
            if is_safe { break; }
        }
        
        for i in finish {
            if !i {
                return -0xDEAD;
            }
        }        
    }

    drop(process_inner);
    sem.down();

    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
    process_inner.semaphore_available[sem_id] -= 1;
    process_inner.semaphore_need[tid][sem_id] -= 1;
    process_inner.semaphore_allocation[tid][sem_id] += 1;

    0
}
/// condvar create syscall
pub fn sys_condvar_create() -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_create",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    id as isize
}
/// condvar signal syscall
pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_signal",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    drop(process_inner);
    condvar.signal();
    0
}
/// condvar wait syscall
pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    trace!(
        "kernel:pid[{}] tid[{}] sys_condvar_wait",
        current_task().unwrap().process.upgrade().unwrap().getpid(),
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid
    );
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    condvar.wait(mutex);
    0
}
/// enable deadlock detection syscall
///
/// YOUR JOB: Implement deadlock detection, but might not all in this syscall
pub fn sys_enable_deadlock_detect(_enabled: usize) -> isize {
    trace!("kernel: sys_enable_deadlock_detect NOT IMPLEMENTED");
    let current = current_process();
    let mut process_inner = current.inner_exclusive_access();
    if _enabled == 1{
        process_inner.deadlock_detect = true
    } else {
        process_inner.deadlock_detect = false
    }
    0
}

//pub fn is_deadlocked() {

// }