use std::path::Path;

fn main() {
    // Build the web frontend before compiling the server.
    // This ensures `dist/` exists for rust-embed.
    let web_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("web")
        .join("mapit-web");

    if web_dir.join("dist").join("index.html").exists() {
        println!("cargo:rerun-if-changed={}", web_dir.join("dist").display());
        return;
    }

    // Only run npm build if dist doesn't exist yet (first build).
    // For subsequent builds, the rerun-if-changed directive above handles it.
    let status = std::process::Command::new("npm")
        .args(["run", "build"])
        .current_dir(&web_dir)
        .status()
        .expect("failed to run npm build");

    if !status.success() {
        panic!("npm build failed");
    }
}
