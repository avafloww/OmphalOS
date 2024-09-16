use crate::platform::{
    context::{thread_create, ThreadContext},
    memory_fence,
};
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::{
    cell::RefCell,
    sync::atomic::{AtomicU32, Ordering},
};
use critical_section::Mutex;
use fugit::HertzU64;
use log::{info, trace};

pub mod exec;

pub type ProcessId = u32;
pub type ThreadId = u32;

static NEXT_PROCESS_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_THREAD_ID: AtomicU32 = AtomicU32::new(0);
static TASK_MANAGER: Mutex<RefCell<Option<TaskManager>>> = Mutex::new(RefCell::new(None));
static mut CURRENT_CTX: Mutex<RefCell<*mut ThreadContext>> =
    Mutex::new(RefCell::new(core::ptr::null_mut()));

const MAX_STACK_SIZE: usize = 4096;

pub const TIME_SLICE_FREQUENCY: HertzU64 = HertzU64::from_raw(100);
pub const TICKS_PER_SECOND: u64 = 1_000_000;

pub struct TaskManager {
    processes: Vec<Process>,
    current_process_id: ProcessId,
    current_process_index: usize,
    current_thread_id: ThreadId,
}
unsafe impl Send for TaskManager {}
unsafe impl Sync for TaskManager {}

impl TaskManager {
    fn new(kernel_process: Process) -> Self {
        let process_id = kernel_process.id;
        let thread_id = kernel_process.threads[0].id;
        Self {
            processes: {
                let mut vec = Vec::with_capacity(1);
                vec.push(kernel_process);
                vec
            },
            current_process_id: process_id,
            current_process_index: 0,
            current_thread_id: thread_id,
        }
    }

    pub fn create_process(&mut self, name: String, initial_thread: Thread) {
        let process = Process::new(name, initial_thread);
        memory_fence();
        self.processes.push(process);
        memory_fence();
    }

    fn get_current_process(&mut self) -> &mut Process {
        self.processes
            .iter_mut()
            .find(|p| p.id == self.current_process_id)
            .unwrap()
    }

    fn next_process(&mut self) -> &mut Process {
        self.current_process_index = (self.current_process_index + 1) % self.processes.len();
        let process = &mut self.processes[self.current_process_index];
        self.current_process_id = process.id;
        self.current_thread_id = process.current_thread_id;

        process
    }
}

pub struct Process {
    pub id: ProcessId,
    pub name: String,
    pub threads: Vec<Thread>,
    current_thread_id: ThreadId,
    current_thread_index: usize,
}

impl Process {
    fn new(name: String, initial_thread: Thread) -> Self {
        let current_thread_id = initial_thread.id;
        Self {
            id: NEXT_PROCESS_ID.fetch_add(1, Ordering::Relaxed),
            name,
            threads: {
                let mut vec = Vec::with_capacity(1);
                vec.push(initial_thread);
                vec
            },
            current_thread_id: current_thread_id,
            current_thread_index: 0,
        }
    }

    pub fn add_thread(&mut self, thread: Thread) {
        memory_fence();
        self.threads.push(thread);
        memory_fence();
    }

    pub fn get_current_thread(&mut self) -> &mut Thread {
        &mut self.threads[self.current_thread_index]
    }

    pub fn next_thread(&mut self) -> &mut Thread {
        self.current_thread_index = (self.current_thread_index + 1) % self.threads.len();
        let thread = &mut self.threads[self.current_thread_index];
        self.current_thread_id = thread.id;

        thread
    }
}

pub struct Thread {
    pub id: ThreadId,
    pub context: *mut ThreadContext,
}

impl Thread {
    unsafe fn new_unsafe() -> Self {
        Self {
            id: NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed),
            context: ThreadContext::allocate(MAX_STACK_SIZE),
        }
    }

    unsafe fn create_unsafe(task: extern "C" fn(), param: *mut ()) -> Self {
        Self {
            id: NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed),
            context: thread_create(task, param, MAX_STACK_SIZE),
        }
    }

    pub fn create(task: extern "C" fn(), param: *mut ()) -> Self {
        critical_section::with(|_| unsafe { Self::create_unsafe(task, param) })
    }
}

pub(crate) fn init_kernel_task() {
    critical_section::with(|cs| unsafe {
        let mut kernel_process = Process::new("kernel".to_string(), Thread::new_unsafe());
        let kernel_thread_ctx = kernel_process.get_current_thread().context.clone();

        // create the task manager object
        let mut task_manager = TASK_MANAGER.borrow_ref_mut(cs);
        if task_manager.is_some() {
            panic!("task manager is already initialized");
        }

        task_manager.replace(TaskManager::new(kernel_process));

        // set up the current context
        let mut current_ctx = CURRENT_CTX.borrow_ref_mut(cs);
        if *current_ctx != core::ptr::null_mut() {
            panic!("current context is already initialized");
        }

        *current_ctx = kernel_thread_ctx;
    });

    unsafe {
        crate::platform::context::setup_multitasking(
            TIME_SLICE_FREQUENCY,
            current_thread_context,
            next_task,
        );
    }

    info!("kernel task initialized");
}

pub unsafe fn current_thread_context() -> *mut ThreadContext {
    critical_section::with(|cs| unsafe { *CURRENT_CTX.borrow_ref(cs) })
}

pub fn with_task_manager<F, R>(f: F) -> R
where
    F: FnOnce(&mut TaskManager) -> R,
{
    critical_section::with(|cs| {
        let mut task_manager = TASK_MANAGER.borrow_ref_mut(cs);
        let task_manager = task_manager.as_mut().unwrap();
        f(task_manager)
    })
}

pub(crate) fn next_task() {
    // basic round-robin scheduler
    // first, switch to another thread in the same process
    // if there's no more threads in the process, switch to the next process
    // if there's no other threads or processes to switch to, don't switch (keep running the current thread)

    critical_section::with(|cs| {
        let mut task_manager = TASK_MANAGER.borrow_ref_mut(cs);
        let task_manager = task_manager.as_mut().unwrap();

        let mut current_ctx = unsafe { CURRENT_CTX.borrow_ref_mut(cs) };

        let current_thread_id = task_manager.current_thread_id;
        let current_process = task_manager.get_current_process();

        let next_thread = current_process.next_thread();
        if next_thread.id != current_thread_id {
            // switch to the next thread in the same process
            *current_ctx = next_thread.context;
            trace!("switched to thread {}", next_thread.id);
            return;
        }

        // switch to the next process
        let current_process_id = task_manager.current_process_id;
        let next_process = task_manager.next_process();
        if next_process.id != current_process_id {
            let next_process_id = next_process.id;
            let next_thread = next_process.get_current_thread();
            *current_ctx = next_thread.context;
            trace!(
                "switched to process {}, thread {}",
                next_process_id,
                next_thread.id
            );
            return;
        }
    });
}
