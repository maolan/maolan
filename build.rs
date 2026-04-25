fn main() {
    #[cfg(target_os = "openbsd")]
    {
        println!("cargo:rustc-link-search=native=/usr/X11R6/lib");
    }
}
