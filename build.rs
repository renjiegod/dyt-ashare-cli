fn main() {
    println!(
        "cargo:rustc-env=DYT_TARGET={}",
        std::env::var("TARGET").unwrap_or_default()
    );
}
