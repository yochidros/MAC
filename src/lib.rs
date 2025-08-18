use std::{cell::UnsafeCell, ptr::null_mut};

mod align_up;
mod alloc;
mod free;
mod realloc;

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
}
