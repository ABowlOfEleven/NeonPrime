fn main() {
    slint_build::compile("ui/app.slint").unwrap();

    // Embed the app icon into the Windows executable (taskbar / Explorer).
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/neonprime.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=app icon embed skipped: {e}");
        }
    }
}
