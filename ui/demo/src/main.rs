#[cfg(target_os = "macos")]
mod components_native;
#[cfg(target_os = "macos")]
mod native;
fn main() {
    #[cfg(target_os = "macos")]
    if std::env::args().any(|arg| arg == "--components") {
        components_native::run();
    } else {
        native::run();
    }
    #[cfg(not(target_os = "macos"))]
    eprintln!("The native demo currently requires macOS; protocol tests remain headless.");
}
