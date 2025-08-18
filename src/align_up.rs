pub fn align_up(x: usize, align: usize) -> usize {
    (x + align - 1) & !(align - 1)
}
