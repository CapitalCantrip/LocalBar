fn stray_after_safety() {
    // SAFETY: this block is fine and ends cleanly here.
    let value = 1;
    // this stray comment is not part of the block above and must fail
    let _ = value;
}
