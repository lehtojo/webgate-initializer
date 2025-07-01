fn main() {
    println!("cargo:rustc-link-lib=static=c");
    println!("cargo:rustc-link-search=/usr/lib/x86_64-linux-gnu");
}
