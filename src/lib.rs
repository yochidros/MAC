use std::{
    cell::UnsafeCell,
    ptr::null_mut,
    sync::atomic::{AtomicUsize, Ordering},
};

const ARENA_SIZE: usize = 1024 * 1024; // 1 MiB

#[repr(C, align(16))]
#[derive(Debug, Clone)]
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
struct Alloced(UnsafeCell<Vec<*mut Block>>);
impl Alloced {
    fn add(&self, ptr: *mut Block) {
        let v = self.0.get();
        unsafe {
            (*v).push(ptr);
        }
    }
    fn remove(&self, ptr: *mut Block) {
        let v = self.0.get();
        unsafe {
            if let Some(pos) = (*v).iter().position(|&x| x == ptr) {
                (*v).remove(pos);
            }
        }
    }
}
static ALLOCATED: Alloced = Alloced(UnsafeCell::new(vec![]));
unsafe impl Sync for Alloced {}

unsafe fn alloc(size: usize) -> *mut u8 {
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
            println!(
                "Allocated!! block: {:?} with size: {}",
                current,
                (*current).size
            );
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

unsafe fn free(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }
    let mut block = ptr.sub(std::mem::size_of::<Block>()) as *mut Block;
    (*block).free = true;
    ALLOCATED.remove(block);

    let headp = FREE_LIST_HEAD.0.get();
    let head = *headp;
    if head.is_null() {
        (*block).next = block;
        headp.write(block);
        return;
    }

    let mut current = head;
    let mut next = (*current).next;

    loop {
        // current < block < next
        if current < block && block < next {
            break;
        }
        if current >= next && (block > current || block < next) {
            break;
        }
        current = next;
        next = (*current).next;
        if current == *headp {
            break;
        }
    }
    // insert
    (*block).next = next;
    (*current).next = block;

    // coalescing
    {
        let mut next_after = (*block).next;
        // println!(
        //     "block: {:?}, added: {:?} next_after: {:?}",
        //     block,
        //     (block as *mut u8).add((*block).size),
        //     next_after
        // );

        if (block as *mut u8).add((*block).size) == next_after as *mut u8 {
            // !! Merging with next block
            (*block).size += (*next).size;
            (*block).next = (*next).next;
            if next_after == *headp {
                headp.write(block);
            }
            next_after = (*block).next;
        }

        if (current as *mut u8).add((*current).size) == block as *mut u8 {
            // !! Merging with previous block
            (*current).size += (*block).size;
            (*current).next = (*block).next;
            if *headp == block {
                headp.write(current);
            }
            block = current; // blockを更新
        }
    }
    println!("Freed!! {:?}", ptr);
}

unsafe fn coalesing(mut block: *mut Block) {}

/// 現在のフリーリストの状態を標準出力に出す（debug用）
pub unsafe fn print_free_list() {
    let mut current = *FREE_LIST_HEAD.0.get();
    let mut i = 0;
    let mut sum_free_size = 0;

    println!();
    println!("---- Free List ----");
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
        if current == FREE_LIST_HEAD.0.get().read() || i > 10 {
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
    println!("Allocated blocks:");
    (*ALLOCATED.0.get()).iter().enumerate().for_each(|(i, p)| {
        println!("#{i}: {p:?}");
    });
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
                let p3 = alloc(512);
                let p4 = alloc(1024);

                let before_free = *FREE_LIST_HEAD.0.get();
                assert!(
                    !before_free.is_null(),
                    "Free list should not be empty before freeing"
                );

                free(p1);
                free(p2);
                free(p4);
                assert!(
                    find_block_in_free_list(p1),
                    "freed block should be in free list {:?}",
                    p1
                );
                assert!(
                    find_block_in_free_list(p4),
                    "freed block should be in free list {:?}",
                    p4
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
