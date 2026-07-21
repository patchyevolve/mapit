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

    // If embedded_dist already exists, skip the npm build
    // (committed to git so `cargo install` / `cargo publish` work without npm)
    if embed_dir.exists() && embed_dir.join("index.html").exists() {
        println!("cargo:warning=embedded_dist/index.html found, skipping frontend build");
        return;
    }

    // Try to build the frontend
    let src = web_dir.join("dist");
    match std::process::Command::new("npm")
        .args(["run", "build"])
        .current_dir(&web_dir)
        .status()
    {
        Ok(status) if status.success() && src.exists() => {
            if embed_dir.exists() {
                fs::remove_dir_all(&embed_dir).unwrap();
            }
            cp_dir(&src, &embed_dir);
        }
        Ok(_) => panic!("npm build failed"),
        Err(_) => panic!(
            "npm not found and embedded_dist/index.html missing.\n\
             Build the frontend: cd web/mapit-web && npm install && npm run build\n\
             Or restore embedded_dist from git: git checkout crates/mapit-server/embedded_dist"
        ),
    }

    // Tell cargo to re-run if web source files change
    println!("cargo:rerun-if-changed={}", web_dir.join("src").display());
    println!("cargo:rerun-if-changed={}", web_dir.display());
    if src.exists() {
        println!("cargo:rerun-if-changed={}", src.display());
    }
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
