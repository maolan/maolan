fn main() {
    // Add X11 library path for OpenBSD
    #[cfg(target_os = "openbsd")]
    {
        println!("cargo:rustc-link-search=native=/usr/X11R6/lib");
    }

    // Add X11 library path for NetBSD
    #[cfg(target_os = "netbsd")]
    {
        println!("cargo:rustc-link-search=native=/usr/X11R7/lib");
    }
}
