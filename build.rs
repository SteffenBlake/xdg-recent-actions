use std::process::Command;
fn main() {
    for path in [
        ".git/HEAD",
        ".git/refs",
        ".git/index",
        "src",
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    let revision = Command::new("git")
        .args(["describe", "--always", "--dirty"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|| "source-archive".into());
    println!("cargo:rustc-env=GIT_REVISION={revision}");
}
