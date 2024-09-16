use core::mem;
use core::{
    alloc::{GlobalAlloc, Layout},
    cell::RefCell,
};
use critical_section::Mutex;
use log::warn;

pub type MemoryAddress = usize;

/// Align downwards. Returns the greatest x with alignment `align`
/// so that x <= addr. The alignment must be a power of 2.
pub const fn align_down(size: usize, align: usize) -> usize {
    assert!(align.is_power_of_two(), "align must be a power of 2");
    if align.is_power_of_two() {
        size & !(align - 1)
    } else if align == 0 {
        size
    } else {
        unreachable!();
    }
}

/// Align the given address upwards to the given alignment.
///
/// Requires that the alignment is a power of two.
pub const fn align_up(addr: usize, align: usize) -> usize {
    assert!(align.is_power_of_two(), "align must be a power of 2");
    (addr + align - 1) & !(align - 1)
}

pub const LIST_NODE_SIZE: usize = mem::size_of::<ListNode>();

pub(crate) struct HeapAllocatorInner {
    head: ListNode,
}

impl HeapAllocatorInner {
    pub const fn new() -> Self {
        Self {
            head: ListNode::new(0),
        }
    }

    pub unsafe fn add_free_region(&mut self, addr: MemoryAddress, size: usize) {
        assert_eq!(align_up(addr, mem::align_of::<ListNode>()), addr);

        const MIN_SIZE: usize = LIST_NODE_SIZE;
        if size < MIN_SIZE {
            warn!(
                "region too small to fit a marker (at {:#x}): {} < {}",
                addr, size, MIN_SIZE
            );
            return;
        }

        let mut node = ListNode::new(size);
        node.next = self.head.next.take();
        let node_ptr = addr as *mut ListNode;
        node_ptr.write(node);
        self.head.next = Some(&mut *node_ptr)
    }

    /// Finds a free region with the given size and alignment, removes it from the list, and returns
    /// the list node and its start address.
    fn find_region(&mut self, size: usize, align: usize) -> Option<(&'static mut ListNode, usize)> {
        let mut current = &mut self.head;

        while let Some(ref mut region) = current.next {
            if let Ok(alloc_start) = Self::alloc_from_region(&region, size, align) {
                // we can allocate this region, so remove it from the list
                let next = region.next.take();
                let ret = Some((current.next.take().unwrap(), alloc_start));
                current.next = next;
                return ret;
            } else {
                // try the next region
                current = current.next.as_mut().unwrap();
            }
        }

        None
    }

    /// Tries to allocate a region of the given size and alignment from the given region.
    /// Returns the start address of the allocated region if successful.
    fn alloc_from_region(region: &ListNode, size: usize, align: usize) -> Result<usize, ()> {
        let alloc_start = align_up(region.start_addr(), align);
        let alloc_end = alloc_start.checked_add(size).ok_or(())?;

        if alloc_end > region.end_addr() {
            // region too small
            return Err(());
        }

        let excess_size = region.end_addr() - alloc_end;

        // either excess_size == 0 (perfect fit), or excess_size >= sizeof(ListNode) (gives us
        // room to continue the linked list); if neither, we can't allocate this region
        if excess_size > 0 && excess_size < LIST_NODE_SIZE {
            return Err(());
        }

        Ok(alloc_start)
    }

    unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let (size, align) = HeapAllocatorInner::size_align(layout);
        if let Some((region, alloc_start)) = self.find_region(size, align) {
            let alloc_end = alloc_start.checked_add(size).expect("overflow");
            let excess_size = region.end_addr() - alloc_end;
            if excess_size > 0 {
                // todo: this is sus...
                self.add_free_region(alloc_end, excess_size);
            }

            alloc_start as *mut u8
        } else {
            core::ptr::null_mut()
        }
    }

    unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        let (size, _) = HeapAllocatorInner::size_align(layout);
        self.add_free_region(ptr as MemoryAddress, size);
    }
}

pub struct HeapAllocator {
    pub(crate) sram: Mutex<RefCell<HeapAllocatorInner>>,
    pub(crate) psram: Option<Mutex<RefCell<HeapAllocatorInner>>>,
}

unsafe impl GlobalAlloc for HeapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        critical_section::with(|cs| self.sram.borrow_ref_mut(cs).alloc(layout))
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        critical_section::with(|cs| self.sram.borrow_ref_mut(cs).dealloc(ptr, layout))
    }
}

/// The given regions must be valid and not overlap with each other.
///
/// # Safety
/// Must be called only once and before any allocation is made.
macro_rules! init_allocators {
    ($sram_words:expr, $psram_start:expr, $psram_size:expr) => {
        #[global_allocator]
        static mut HEAP_ALLOCATOR: crate::allocator::HeapAllocator =
            crate::allocator::HeapAllocator {
                sram: critical_section::Mutex::new(core::cell::RefCell::new(
                    crate::allocator::HeapAllocatorInner::new(),
                )),
                psram: None,
            };

        unsafe {
            static mut SRAM_HEAP: core::mem::MaybeUninit<[usize; $sram_words]> =
                core::mem::MaybeUninit::uninit();
            critical_section::with(|cs| {
                HEAP_ALLOCATOR.sram.borrow_ref_mut(cs).add_free_region(
                    SRAM_HEAP.as_mut_ptr() as usize,
                    $sram_words * core::mem::size_of::<usize>(),
                );
                if ($psram_size > 0) {
                    let psram_allocator = critical_section::Mutex::new(core::cell::RefCell::new(
                        crate::allocator::HeapAllocatorInner::new(),
                    ));
                    psram_allocator
                        .borrow_ref_mut(cs)
                        .add_free_region($psram_start, $psram_size);
                    HEAP_ALLOCATOR.psram = Some(psram_allocator);
                }
                log::info!("heap allocator initialized");
            });
        }
    };
}
pub(crate) use init_allocators as init;

/// Represents a node of the linked list allocator.
struct ListNode {
    next: Option<&'static mut ListNode>,
    size: usize,
}

impl ListNode {
    /// Creates a new node with the given size.
    const fn new(size: usize) -> Self {
        Self { next: None, size }
    }

    /// Returns the start address of this memory region.
    fn start_addr(&self) -> usize {
        self as *const Self as usize
    }

    /// Returns the end address of this memory region.
    fn end_addr(&self) -> usize {
        self.start_addr() + self.size
    }
}

impl HeapAllocatorInner {
    /// Adjusts the given layout so that the resulting allocated region can also store a ListNode.
    ///
    /// Returns the adjusted size and alignment.
    fn size_align(layout: Layout) -> (usize, usize) {
        let layout = layout
            .align_to(mem::align_of::<ListNode>())
            .expect("adjusting alignment failed")
            .pad_to_align();
        let size = layout.size().max(LIST_NODE_SIZE);
        (size, layout.align())
    }
}
