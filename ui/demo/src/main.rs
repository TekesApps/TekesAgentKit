#[cfg(target_os = "macos")]
mod acceptance_script;
#[cfg(target_os = "macos")]
mod components_native;
#[cfg(target_os = "macos")]
mod native;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args
        .iter()
        .any(|arg| arg.starts_with("--child-pane-source"))
    {
        #[cfg(not(feature = "acceptance-source-witness"))]
        {
            eprintln!("child pane source proof requires the acceptance-source-witness feature");
            std::process::exit(2);
        }
        #[cfg(feature = "acceptance-source-witness")]
        assert!(
            args.iter().any(|arg| arg == "--components")
                && args.iter().any(|arg| arg == "--child-pane-source-proof")
                && args.iter().any(|arg| arg == "--offscreen")
                && args.iter().any(|arg| arg == "--endpoint-config")
                && args.iter().any(|arg| arg == "--child-pane-source-receipt"),
            "child pane source proof requires components, explicit offscreen endpoints, and a receipt"
        );
    }
    #[cfg(target_os = "macos")]
    if args.iter().any(|arg| arg == "--components") {
        components_native::run();
    } else {
        native::run();
    }
    #[cfg(not(target_os = "macos"))]
    eprintln!("The native demo currently requires macOS; protocol tests remain headless.");
}
