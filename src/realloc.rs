use std::ptr::{self, null_mut};

use crate::{
    align_up::align_up, alloc::alloc, free::free, Block, ALLOCATED, ARENA, ARENA_SIZE,
    FREE_LIST_HEAD,
};

// Minimum size to split a block
const MIN_SPLIT: usize = std::mem::size_of::<Block>() + 16;

/*
- ptr == NULL → alloc
-	new_size == 0 → free + NULL
-	shrink は（余りが十分なら）分割して余りを free に渡す
-	grow はまず 右隣ブロックが free で拡張可能か試す（in-place）
-	可能なら隣を free-list から外して結合、必要なら分割して余りを free
-	不可能なら alloc して memcpy、古いブロックを free（OOM の場合は元ブロックを残す）
*/
pub unsafe fn realloc(ptr: *mut u8, new_size: usize) -> *mut u8 {
    if ptr.is_null() {
        return unsafe { alloc(new_size) };
    }
    if new_size == 0 {
        unsafe {
            free(ptr);
        }
        return null_mut();
    }
    let header_size = std::mem::size_of::<Block>();
    let align = align_of::<Block>();
    let needed = align_up(new_size + header_size, align);

    let block = unsafe { (ptr as *mut u8).sub(header_size) as *mut Block };
    let old_size = (*block).size;
    let old_data = old_size - header_size;

    if old_size >= needed {
        let remainder = old_size - needed;
        if remainder >= MIN_SPLIT {
            unsafe {
                let new_block = (block as *mut u8).add(needed) as *mut Block;
                (*new_block).size = remainder;
                (*new_block).free = true;
                (*new_block).next = null_mut();
                (*block).size = needed;

                let data = (new_block as *mut u8).add(header_size);
                free(data);
            }
        }

        return ptr;
    }

    if try_in_place_extend_next_free_block(block, needed) {
        return ptr;
    }

    let new_ptr = alloc(new_size);
    if new_ptr.is_null() {
        // Allocation failed, return null
        return null_mut();
    }

    unsafe {
        ptr::copy_nonoverlapping(ptr, new_ptr, core::cmp::min(old_data, new_size));

        let old_block_ptr = block;
        ALLOCATED.remove(old_block_ptr);
        free(ptr);
    }

    new_ptr
}

// Try to extend 'block' by merging with the immediate next physical block if it's free.
// Return true if after operation block.size >= needed.
unsafe fn try_in_place_extend_next_free_block(block: *mut Block, needed: usize) -> bool {
    let base = ARENA.area.get() as usize;
    let arena_end = base + ARENA_SIZE;

    let block_end = (block as *mut u8).add((*block).size) as *mut Block;
    let block_end_addr = block_end as usize;

    if block_end_addr < base || block_end_addr >= arena_end {
        return false; // Block end is out of bounds
    }
    if block_end == block {
        return false; // Block is self-referential, cannot extend
    }

    let next = block_end;
    if !(*next).free {
        return false; // Next block is not free
    }

    // can combine
    // remove next from free list
    remove_free_block(next);

    (*block).size = (*block).size + (*next).size;
    if (*block).size >= needed {
        let remainder = (*block).size - needed;
        if remainder >= MIN_SPLIT {
            let new_block = (block as *mut u8).add(needed) as *mut Block;
            (*new_block).size = remainder;
            (*new_block).free = true;
            (*new_block).next = null_mut();
            (*block).size = needed;

            let data = (new_block as *mut u8).add(std::mem::size_of::<Block>());
            free(data);
        }
        return true;
    }

    false
}

unsafe fn remove_free_block(block: *mut Block) {
    let headp = FREE_LIST_HEAD.0.get();
    let head = *headp;

    if head.is_null() {
        return; // No free blocks
    }

    if head == block && (*head).next == head {
        headp.write(null_mut()); // Only one block, now removed
        return;
    }

    let mut current = head;
    loop {
        if (*current).next == block {
            (*current).next = (*block).next;
            if block == head {
                headp.write(current); // Update head if we removed the head block
            }
            return;
        }
        current = (*current).next;
        if current == head {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_arena;

    #[test]
    fn test_realloc() {
        unsafe {
            init_arena();
            let ptr = alloc(64);
            assert!(!ptr.is_null(), "Allocation failed");

            let new_ptr = realloc(ptr, 128);
            assert!(!new_ptr.is_null(), "Reallocation failed");
            assert_eq!(new_ptr, ptr, "Reallocation did not return the same pointer");

            free(new_ptr);
        }
    }

    #[test]
    fn test_realloc_zero_size() {
        unsafe {
            init_arena();
            let ptr = alloc(64);
            assert!(!ptr.is_null(), "Allocation failed");

            let new_ptr = realloc(ptr, 0);
            assert!(
                new_ptr.is_null(),
                "Reallocation to zero size did not return null"
            );
        }
    }
    #[test]
    fn grow_shrink_realloc() {
        unsafe {
            init_arena();
            let ptr = alloc(128);
            assert!(!ptr.is_null(), "Allocation failed");
            ptr::write_bytes(ptr, 0xAA, 128);

            let new_ptr = realloc(ptr, 512);
            assert!(!new_ptr.is_null(), "Reallocation failed");
            assert_eq!(*(new_ptr as *const u8), 0xAA);
            let p3 = realloc(new_ptr, 64);
            assert_eq!(*(p3 as *const u8), 0xAA);

            free(p3);
        }
    }
}
