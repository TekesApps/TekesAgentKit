//! Host system adapter, not a Kit collation algorithm. Matches the current
//! user's case-insensitive localized ordering through CoreFoundation.
use std::{cmp::Ordering, ffi::c_void};
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithBytes(
        allocator: *const c_void,
        bytes: *const u8,
        length: isize,
        encoding: u32,
        external: u8,
    ) -> *const c_void;
    fn CFStringCompare(a: *const c_void, b: *const c_void, options: u64) -> isize;
    fn CFRelease(value: *const c_void);
}
struct Text(*const c_void);
impl Text {
    fn new(value: &str) -> Self {
        // CFString.h: UTF8=0x08000100. The copied buffer remains owned by CF.
        let text = unsafe {
            CFStringCreateWithBytes(
                std::ptr::null(),
                value.as_ptr(),
                isize::try_from(value.len()).expect("string length fits CFIndex"),
                0x08000100,
                0,
            )
        };
        assert!(!text.is_null(), "CoreFoundation string allocation");
        Self(text)
    }
}
impl Drop for Text {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.0);
        }
    }
}
pub fn compare(a: &str, b: &str) -> Ordering {
    let a = Text::new(a);
    let b = Text::new(b);
    // CFString.h: kCFCompareCaseInsensitive=1, kCFCompareLocalized=32.
    unsafe { CFStringCompare(a.0, b.0, 1 | 32) }.cmp(&0)
}
#[test]
fn system_comparison_keeps_case_equivalence_and_unicode_input() {
    assert_eq!(compare("Alpha", "alpha"), Ordering::Equal);
    assert_eq!(compare("a", "b"), Ordering::Less);
    assert_eq!(compare("中文👩🏽‍💻", "中文👩🏽‍💻"), Ordering::Equal);
}
