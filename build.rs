fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut resource = winres::WindowsResource::new();
        resource.set_icon("assets/daedalus-simulator-icon.ico");
        if let Err(error) = resource.compile() {
            println!("cargo:warning=failed to embed Windows icon: {error}");
        }
    }
}
