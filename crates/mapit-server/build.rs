use std::fs;
use std::path::Path;

fn main() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let web_dir = manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("web")
        .join("mapit-web");
    let embed_dir = manifest_dir.join("embedded_dist");

    // Rebuild the frontend if source files changed
    let status = std::process::Command::new("npm")
        .args(["run", "build"])
        .current_dir(&web_dir)
        .status()
        .expect("failed to run npm build");
    if !status.success() {
        panic!("npm build failed");
    }

    // Copy dist/ into embedded_dist/ so rust-embed can find it locally
    let src = web_dir.join("dist");
    if embed_dir.exists() {
        fs::remove_dir_all(&embed_dir).unwrap();
    }
    cp_dir(&src, &embed_dir);

    // Tell cargo to re-run if web source files change
    println!("cargo:rerun-if-changed={}", web_dir.join("src").display());
    println!("cargo:rerun-if-changed={}", web_dir.display());
    println!("cargo:rerun-if-changed={}", src.display());
}

fn cp_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let ty = entry.file_type().unwrap();
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            cp_dir(&entry.path(), &dst_path);
        } else {
            fs::copy(&entry.path(), &dst_path).unwrap();
        }
    }
}
