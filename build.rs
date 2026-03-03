fn main() {
    // Add X11 library path for OpenBSD
    #[cfg(target_os = "openbsd")]
    {
        println!("cargo:rustc-link-search=native=/usr/X11R6/lib");
    }
}
