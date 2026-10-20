fn main() {
    println!("cargo:rerun-if-env-changed=DYT_ASHARE_RELEASE_PUBLIC_KEY");
    println!(
        "cargo:rustc-env=DYT_TARGET={}",
        std::env::var("TARGET").unwrap_or_default()
    );
}
