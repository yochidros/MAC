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
    (*block_ptr).next = block_ptr;
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

    let mut prev = *FREE_LIST_HEAD.0.get();
    let mut current = (*prev).next;
    let start = current;

    loop {
        if (*current).free && (*current).size >= needed {
            // found block!!
            (*current).free = false;
            if (*current).size == needed {
                (*prev).next = (*current).next;

                if current == *FREE_LIST_HEAD.0.get() {
                    // if we are freeing the head, update the head
                    FREE_LIST_HEAD.0.get().write(prev);
                }
            } else {
                let new_block = (current as *mut u8).add(needed) as *mut Block;
                (*new_block).size = (*current).size - needed;
                (*new_block).free = true;
                println!(
                    "Splitting block: {:?}({:?}) into new block: {:?}({:?})",
                    current,
                    (*current).size,
                    new_block,
                    (*new_block).size
                );

                (*new_block).next = new_block;
                (*prev).next = new_block;

                (*current).size = needed;
                (*current).next = null_mut();

                if current == *FREE_LIST_HEAD.0.get() {
                    // if we are freeing the head, update the head
                    FREE_LIST_HEAD.0.get().write(new_block);
                }
            }

            println!(
                "Allocated block: {:?} with size: {}",
                current,
                (*current).size
            );
            return (current as *mut u8).add(std::mem::size_of::<Block>());
        }
        if current == FREE_LIST_HEAD.0.get().read() {
            println!("Reached the end of the free list without finding a suitable block.");
            break;
        }
        prev = current;
        current = (*current).next;
    }
    println!("No suitable block found for allocation of size: {}", size);
    null_mut() // allocation attempts failed block not found
}

unsafe fn split_block(prev: *mut Block, block: *mut Block, needed: usize) {
    let total = (*block).size;

    if total == needed {
        return;
    } else if total < needed {
        panic!("Cannot split block: not enough space {total} {needed}");
    }

    // current block is larger than needed, so we can split it
    // new block will be created after the current block
    // new block linked to the free list head
    let new_block_ptr = (block as *mut u8).add(needed) as *mut Block;
    (*new_block_ptr).size = total - needed;
    (*new_block_ptr).next = (*block).next;
    (*new_block_ptr).free = true;

    (*block).size = needed;

    // set free list head to the new block
    FREE_LIST_HEAD.0.get().write(new_block_ptr);
}

unsafe fn free(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }

    let block_ptr = ptr.sub(std::mem::size_of::<Block>()) as *mut Block;
    (*block_ptr).free = true;

    println!("Freeing block: {:?}", block_ptr);
    coalesing(block_ptr);
    println!("Freed!! {:?}", block_ptr);
}
unsafe fn coalesing(mut block: *mut Block) {
    let mut current = *FREE_LIST_HEAD.0.get();

    let start = current;
    loop {
        if block > current && block < (*current).next {
            println!("Found position to insert block: {:?}", current);
            break;
        }
        if current >= (*current).next && (block > current || block < (*current).next) {
            // we are at the end of the list, and block is not in the list
            println!("Reached end of free list, inserting block: {:?}", current);
            break;
        }
        current = (*current).next;
        if current == start {
            println!(
                "Reached the start of the free list, inserting block: {:?}",
                current
            );
            break;
        }
    }

    // insert
    (*block).next = (*current).next;
    (*current).next = block;

    // free listの先頭がblockの前にある場合、blockを前のブロックと結合する
    if (block as *mut u8).add((*block).size) == (*block).next as *mut u8 {
        println!("Coalescing with next block");
        (*block).size += (*(*block).next).size;
        (*block).next = (*(*block).next).next;
    }

    if (current as *mut u8).add((*current).size) == block as *mut u8 {
        println!("Coalescing with next block");
        // blockと次のブロックが連続している場合、結合する
        (*current).size += (*block).size;
        (*current).next = (*block).next;
        block = current; // blockを更新
    }
}

/// 現在のフリーリストの状態を標準出力に出す（debug用）
pub unsafe fn print_free_list() {
    let mut current = *FREE_LIST_HEAD.0.get();
    let mut i = 0;
    let mut sum_free_size = 0;
    let start = current;

    println!("---- Free List ---- start: {start:p}");
    loop {
        println!(
            "#{:<2}  ptr: {:p}, size(B): {:>8}, free: {}, next: {:p}",
            i,
            current,
            (*current).size,
            (*current).free,
            (*current).next,
        );
        sum_free_size += (*current).size;
        current = (*current).next;
        i += 1;
        if current == FREE_LIST_HEAD.0.get().read() {
            break; // 循環している場合は終了
        }
    }
    if i == 0 {
        println!("(empty)");
    }
    println!(
        "Arena Size: {ARENA_SIZE}\nTotal free size: {sum_free_size}\nUsed Size: {}",
        ARENA_SIZE - sum_free_size
    );
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

    #[test]
    fn test_free() {
        unsafe {
            init_arena();
        }
        for i in 0..5 {
            println!("Test free: iteration {}", i);
            unsafe {
                let p1 = alloc(128);
                let p2 = alloc(256);

                println!("Allocated !! pointers: p1: {:?}, p2: {:?}", p1, p2);
                let before_free = *FREE_LIST_HEAD.0.get();
                assert!(
                    !before_free.is_null(),
                    "Free list should not be empty before freeing"
                );

                free(p1);
                assert!(
                    find_block_in_free_list(p1),
                    "freed block should be in free list"
                );
                print_free_list();
            }
        }
    }

    fn find_block_in_free_list(ptr: *mut u8) -> bool {
        unsafe {
            let head = *FREE_LIST_HEAD.0.get();
            let mut cur = head;
            let mut found = false;
            loop {
                if cur == (ptr as *mut u8).sub(std::mem::size_of::<Block>()) as *mut Block {
                    found = true;
                    break;
                }
                cur = (*cur).next;
                if cur == head {
                    break;
                }
            }
            found
        }
    }
}
