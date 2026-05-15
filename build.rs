use clap::CommandFactory;
use clap_mangen::Man;
use std::fs;

#[path = "src/cli.rs"]
mod cli;

fn main() {
    // Create doc directory if it doesn't exist
    let out_dir = std::path::PathBuf::from("doc");
    fs::create_dir_all(&out_dir).expect("Failed to create doc directory");

    // Generate man page from Cli struct
    let cmd = cli::Cli::command();
    let man = Man::new(cmd);
    let mut buf = Vec::new();
    man.render(&mut buf).expect("Failed to render man page");

    fs::write(out_dir.join("omny.1"), buf).expect("Failed to write man page");

    // Expose the build target triple so the updater can pick the matching
    // release asset (e.g. "x86_64-apple-darwin").
    let target = std::env::var("TARGET").expect("TARGET not set by cargo");
    println!("cargo:rustc-env=BUILD_TARGET={target}");

    println!("cargo:rerun-if-changed=src/cli.rs");
}
