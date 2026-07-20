fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        println!("cargo:rerun-if-changed=images/maolan-icon.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("images/maolan-icon.ico");
        if let Err(e) = res.compile() {
            eprintln!("Failed to compile Windows icon resource: {e}");
        }
    }
}
