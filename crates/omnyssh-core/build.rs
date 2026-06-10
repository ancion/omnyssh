fn main() {
    // Expose the build target triple so the updater can pick the matching
    // release asset (e.g. "x86_64-apple-darwin").
    let target = std::env::var("TARGET").expect("TARGET not set by cargo");
    println!("cargo:rustc-env=BUILD_TARGET={target}");
}
