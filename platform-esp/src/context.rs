//! Support code for the Xtensa architecture, as used in the ESP32 series of chips.
//!
//! Reference:
//! https://sachin0x18.github.io/posts/demystifying-xtensa-isa/
//! https://github.com/esp-rs/esp-hal/blob/main/esp-wifi/src/preempt/preempt_xtensa.rs#L7

use alloc::alloc::{alloc, alloc_zeroed, Layout};
use core::{arch::asm, cell::RefCell};
use critical_section::Mutex;
use esp_hal::{
    interrupt::{self, InterruptHandler},
    timer::{ErasedTimer, PeriodicTimer},
    trapframe::TrapFrame,
    xtensa_lx, xtensa_lx_rt,
};
use fugit::HertzU64;
use log::{info, trace};

static TIMER0: Mutex<RefCell<Option<TimeBase>>> = Mutex::new(RefCell::new(None));

// SAFETY: we never call these before the function they're set in, and never change them after they're set
static mut CURRENT_THREAD_CONTEXT_FN: unsafe fn() -> *mut ThreadContext = || core::ptr::null_mut();
static mut NEXT_TASK_FN: fn() = || {};

pub type TimeBase = PeriodicTimer<'static, ErasedTimer>;
pub(crate) fn set_timer0(timer: TimeBase) {
    critical_section::with(|cs| {
        TIMER0.borrow_ref_mut(cs).replace(timer);
    });
}

/// Saved context of a thread.
/// Each thread has a context object created for it upon thread creation.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ThreadContext {
    trap_frame: TrapFrame,
    pub thread_semaphore: u32,
    pub allocated_stack: *const u8,
}
unsafe impl Send for ThreadContext {}
unsafe impl Sync for ThreadContext {}

impl ThreadContext {
    // todo: this is never actually freed anywhere!!
    pub unsafe fn allocate(stack_size: usize) -> *mut ThreadContext {
        // allocate memory for the context itself
        // todo: does this need to be zeroed entirely?
        let ctx: *mut ThreadContext = alloc_zeroed(Layout::new::<ThreadContext>()) as *mut _;

        //
        let layout = Layout::from_size_align(stack_size + 4, 16).unwrap();
        let allocated_size = layout.size();
        let stack = alloc(layout);

        // write the allocated stack size to the first 4 bytes of the stack
        // when we free the stack, we use this size to free the correct amount of memory
        *(stack as *mut usize) = allocated_size;

        // offset the stack pointer by 4 bytes
        let stack = stack.offset(4);

        (*ctx).allocated_stack = stack;

        ctx
    }
}

// impl Drop for ThreadContext {
//     fn drop(&mut self) {
//         trace!("ThreadContext::drop");
//         unsafe {
//             // allocated_stack minus 4 bytes = size of the allocation
//             let stack = self.allocated_stack as *mut u8;
//             let stack = stack.offset(-4);
//             let size = *(stack as *const u32) as usize;
//             trace!("deallocating stack: {:?}, size: {}", stack, size);

//             // we already know it's aligned by 16, just deallocate
//             // the allocated size we retrieve here includes the 4 bytes we wrote at the beginning
//             let layout = alloc::alloc::Layout::from_size_align_unchecked(size, 16);
//             alloc::alloc::dealloc(stack, layout);
//             trace!("deallocated stack: {:?}", stack);
//         }
//     }
// }

pub fn thread_create(
    task: extern "C" fn(),
    param: *mut (),
    thread_stack_size: usize,
) -> *mut ThreadContext {
    unsafe {
        let ctx = ThreadContext::allocate(thread_stack_size);

        (*ctx).trap_frame.PC = task as usize as u32;
        (*ctx).trap_frame.A6 = param as usize as u32;

        // stack must be aligned by 16
        let task_stack_ptr = (*ctx).allocated_stack as usize + thread_stack_size;
        let stack_ptr = task_stack_ptr - (task_stack_ptr % 0x10);
        (*ctx).trap_frame.A1 = stack_ptr as u32;

        (*ctx).trap_frame.PS = 0x00040000 | (1 & 3) << 16; // For windowed ABI set WOE and CALLINC (pretend task was 'call4'd).
        (*ctx).trap_frame.A0 = 0;

        *((task_stack_ptr - 4) as *mut u32) = 0;
        *((task_stack_ptr - 8) as *mut u32) = 0;
        *((task_stack_ptr - 12) as *mut u32) = stack_ptr as u32;
        *((task_stack_ptr - 16) as *mut u32) = 0;

        ctx
    }
}

unsafe fn restore_thread_context(ctx: *mut ThreadContext, trap_frame: &mut TrapFrame) {
    *trap_frame = (*ctx).trap_frame;
}

unsafe fn save_thread_context(ctx: *mut ThreadContext, trap_frame: &TrapFrame) {
    (*ctx).trap_frame = *trap_frame;
}

/// Enables the following interrupts:
/// - Software1, for voluntary task yielding
/// - Level2, for the timer interrupt
/// - Level6 (NMI?)
/// - all other interrupts that were enabled before calling this function
pub unsafe fn setup_multitasking(
    time_slice_frequency: HertzU64,
    current_thread_context_fn: unsafe fn() -> *mut ThreadContext,
    next_task_fn: fn(),
) {
    // set the function pointers
    CURRENT_THREAD_CONTEXT_FN = current_thread_context_fn;
    NEXT_TASK_FN = next_task_fn;

    // set up the timer and timer interrupt
    critical_section::with(|cs| {
        let mut timer = TIMER0.borrow_ref_mut(cs);
        let timer = timer.as_mut().unwrap();

        timer.set_interrupt_handler(InterruptHandler::new(
            unsafe {
                core::mem::transmute::<*const (), extern "C" fn()>(
                    interrupt_timg0_timer0 as *const (),
                )
            },
            interrupt::Priority::Priority2,
        ));
        timer.start(time_slice_frequency.into_duration()).unwrap();
        timer.enable_interrupt(true);
    });

    // set up software interrupts for yielding
    unsafe {
        let enabled = xtensa_lx::interrupt::disable();
        xtensa_lx::interrupt::enable_mask(
            1 << 29 // Software1
                | xtensa_lx_rt::interrupt::CpuInterruptLevel::Level2.mask()
                | xtensa_lx_rt::interrupt::CpuInterruptLevel::Level6.mask()
                | enabled,
        );
    }

    // yield now so we have a context
    yield_task();

    info!("arch-xtensa: multitasking enabled");
}

unsafe fn task_switch(trap_frame: &mut TrapFrame) {
    save_thread_context(CURRENT_THREAD_CONTEXT_FN(), trap_frame);

    // unsafe {
    //     if !SCHEDULED_TASK_TO_DELETE.is_null() {
    //         delete_task(SCHEDULED_TASK_TO_DELETE);
    //         SCHEDULED_TASK_TO_DELETE = core::ptr::null_mut();
    //     }
    // }

    NEXT_TASK_FN();

    restore_thread_context(CURRENT_THREAD_CONTEXT_FN(), trap_frame);
}

unsafe fn do_task_switch(context: &mut TrapFrame) {
    critical_section::with(|cs| {
        let mut timer = TIMER0.borrow_ref_mut(cs);
        let timer = timer.as_mut().unwrap();
        timer.clear_interrupt();
    });

    task_switch(context);
}

unsafe extern "C" fn interrupt_timg0_timer0(context: &mut TrapFrame) {
    do_task_switch(context);
}

#[allow(non_snake_case)]
#[no_mangle]
unsafe extern "C" fn Software1(_level: u32, context: &mut TrapFrame) {
    let intr = 1 << 29;
    unsafe {
        asm!("wsr.intclear {0}", in(reg) intr, options(nostack));
    }

    do_task_switch(context);
}

pub fn yield_task() {
    trace!("yield_task on core: {:?}", esp_hal::get_core());
    let intr = 1 << 29;
    unsafe {
        asm!("wsr.intset {0}", in(reg) intr, options(nostack));
    }
}
