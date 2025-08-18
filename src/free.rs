use crate::{Block, ALLOCATED, ARENA_SIZE, FREE_LIST_HEAD};

pub unsafe fn free(ptr: *mut u8) {
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
/// 現在のフリーリストの状態を標準出力に出す（debug用）
pub unsafe fn print_free_list() {
    #[cfg(not(debug_assertions))]
    {
        return; // debugモードでのみ有効
    }
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
    use crate::alloc::alloc;
    use crate::init_arena;

    use super::*;

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
