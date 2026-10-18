#[cfg(target_os = "macos")]
mod native;
fn main() {
    #[cfg(target_os = "macos")]
    native::run();
    #[cfg(not(target_os = "macos"))]
    eprintln!("The native demo currently requires macOS; protocol tests remain headless.");
}
