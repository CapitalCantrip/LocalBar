fn safety_multiline() {
    // SAFETY: the pointer is valid for the duration of this call because
    // it was just obtained from a live reference on the line above, and
    // no other code can invalidate it before the unsafe block returns.
    unsafe {
        std::ptr::null_mut::<i32>();
    }
}
