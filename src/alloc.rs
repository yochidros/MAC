use std::ptr::null_mut;

use crate::{align_up::*, Block, ALLOCATED, FREE_LIST_HEAD};

pub unsafe fn alloc(size: usize) -> *mut u8 {
    if size == 0 {
        return null_mut();
    }
    let align = align_of::<Block>();
    let needed = align_up(size + std::mem::size_of::<Block>(), align);

    let mut prev = *FREE_LIST_HEAD.0.get();
    let mut current = (*prev).next;

    loop {
        if (*current).free && (*current).size >= needed {
            let remainder = (*current).size - needed;
            let min_split = std::mem::size_of::<Block>() + align;
            if remainder >= min_split {
                (*current).free = false;
                split_block(prev, current, needed);
            } else {
                (*current).free = false;
                (*prev).next = (*current).next;
                if current == FREE_LIST_HEAD.0.get().read() {
                    FREE_LIST_HEAD.0.get().write(prev);
                }
            }
            ALLOCATED.add(current);
            #[cfg(debug_assertions)]
            {
                println!(
                    "Allocated!! block: {:?} with size: {}",
                    current,
                    (*current).size
                );
            }
            return (current as *mut u8).add(std::mem::size_of::<Block>());
        }
        if current == FREE_LIST_HEAD.0.get().read() {
            break;
        }
        prev = current;
        current = (*current).next;
    }
    println!("No suitable block found for allocation of size: {}", size);
    null_mut() // allocation attempts failed block not found
}

unsafe fn split_block(prev: *mut Block, current: *mut Block, needed: usize) {
    let new_block = (current as *mut u8).add(needed) as *mut Block;
    (*new_block).size = (*current).size - needed;
    (*new_block).free = true;
    (*new_block).next = (*current).next;
    if prev == current {
        (*new_block).next = new_block; // if we are splitting the head, point to itself
    } else {
        (*prev).next = new_block;
    }

    (*current).size = needed;

    if current == *FREE_LIST_HEAD.0.get() {
        // if we are freeing the head, update the head
        FREE_LIST_HEAD.0.get().write(new_block);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{init_arena, ARENA_SIZE};

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
            assert_ne!(ptr1, ptr2, "Allocations should return different pointers");
            assert!(!ptr1.is_null(), "First allocation should not return null");
            assert!(!ptr2.is_null(), "Second allocation should not return null.");
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
