use std::{cell::UnsafeCell, ptr::null_mut};

const ARENA_SIZE: usize = 1024 * 1024; // 1 MiB

#[repr(C, align(16))]
#[derive(Debug)]
struct Block {
    size: usize,
    next: *mut Block,
    free: bool,
}

#[repr(C, align(16))]
struct Arena([u8; ARENA_SIZE]);

unsafe impl Sync for Arena {}

struct ArenaAllocator {
    area: UnsafeCell<Arena>,
}
unsafe impl Sync for ArenaAllocator {}

struct FreeListHead(UnsafeCell<*mut Block>);
unsafe impl Sync for FreeListHead {}

static ARENA: ArenaAllocator = ArenaAllocator {
    area: UnsafeCell::new(Arena([0; ARENA_SIZE])),
};
static FREE_LIST_HEAD: FreeListHead = FreeListHead(UnsafeCell::new(null_mut()));

unsafe fn init_arena() {
    let base_ptr = ARENA.area.get() as usize;
    let align = align_of::<Block>();
    let aligned = (base_ptr + align - 1) & !(align - 1);
    let block_ptr = aligned as *mut Block;

    (*block_ptr).size = ARENA_SIZE - (aligned - base_ptr);
    (*block_ptr).next = null_mut();
    (*block_ptr).free = true;

    FREE_LIST_HEAD.0.get().write(block_ptr);
}
fn align_up(x: usize, align: usize) -> usize {
    (x + align - 1) & !(align - 1)
}

unsafe fn alloc(size: usize) -> *mut u8 {
    if size == 0 {
        return null_mut();
    }
    let align = align_of::<Block>();
    let needed = align_up(size + std::mem::size_of::<Block>(), align);

    let mut prev: *mut Block = null_mut();
    let mut current = *FREE_LIST_HEAD.0.get();
    println!("Allocating size: {}, needed: {}", size, needed);

    while !current.is_null() {
        if (*current).free && (*current).size >= needed {
            // found block!!
            (*current).free = false;

            if prev.is_null() {
                FREE_LIST_HEAD.0.get().write((*current).next);
            } else {
                (*prev).next = (*current).next;
                println!("Found block: {:?}", current);
            }

            println!(
                "Allocating block: ptr: {:p}, size: {}, free: {}",
                current,
                (*current).size,
                (*current).free
            );
            return (current as *mut u8).add(std::mem::size_of::<Block>());
        }
        println!(
            "Checking block: ptr: {:p}, size: {}, free: {}",
            current,
            (*current).size,
            (*current).free
        );
        prev = current;
        current = (*current).next;
    }
    println!("No suitable block found for allocation of size: {}", size);
    null_mut() // allocation attempts failed block not found
}
/// 現在のフリーリストの状態を標準出力に出す（debug用）
pub unsafe fn print_free_list() {
    let mut current = *FREE_LIST_HEAD.0.get();
    let mut i = 0;

    println!("---- Free List ----");
    while !current.is_null() {
        println!(
            "#{:<2}  ptr: {:p}, size: {:>6}, free: {}, next: {:p}",
            i,
            current,
            (*current).size,
            (*current).free,
            (*current).next,
        );
        current = (*current).next;
        i += 1;
    }
    if i == 0 {
        println!("(empty)");
    }
    println!("-------------------");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_allocator() {
        unsafe {
            init_arena();
            let arena = &ARENA.area.get();
            assert!(!arena.is_null(), "Arena should be initialized");
            let block = FREE_LIST_HEAD.0.get().read();
            assert!(!block.is_null());
            assert!(block.read().free);

            let addr = block as usize;
            assert!(addr % align_of::<Block>() == 0, "Block should be aligned");
        }
    }

    #[test]
    fn test_alloc_return_aligned_pointer() {
        unsafe {
            init_arena();
            let ptr = alloc(64);
            assert!(!ptr.is_null(), "Allocation should not return null");
            let addr = ptr as usize;
            assert!(
                addr % align_of::<Block>() == 0,
                "Allocated pointer should be aligned"
            );
        }
    }

    #[test]
    fn test_alloc_zero_size() {
        unsafe {
            init_arena();
            let ptr = alloc(0);
            assert!(ptr.is_null(), "Allocation of zero size should return null");
        }
    }

    #[test]
    fn test_alloc_multiple() {
        unsafe {
            init_arena();
            let ptr1 = alloc(128);
            let ptr2 = alloc(256);
            print_free_list();
            assert_ne!(ptr1, ptr2, "Allocations should return different pointers");
            assert!(!ptr1.is_null(), "First allocation should not return null");
            assert!(
                ptr2.is_null(),
                "Second allocation should return null. split not impletemented yet."
            );
        }
    }

    #[test]
    fn test_alloc_exceeding_size() {
        unsafe {
            init_arena();
            let ptr = alloc(ARENA_SIZE + 1);
            assert!(
                ptr.is_null(),
                "Allocation exceeding arena size should return null"
            );
        }
    }
}
