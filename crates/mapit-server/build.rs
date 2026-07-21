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
    // (CI pre-builds the frontend before cargo build)
    if embed_dir.exists() && embed_dir.read_dir().map(|mut i| i.next().is_some()).unwrap_or(false) {
        println!("cargo:warning=embedded_dist exists, skipping frontend build");
        return;
    }

    // Try to build the frontend - don't hard-fail if npm is unavailable
    let src = web_dir.join("dist");
    if let Ok(status) = std::process::Command::new("npm")
        .args(["run", "build"])
        .current_dir(&web_dir)
        .status()
    {
        if status.success() && src.exists() {
            if embed_dir.exists() {
                fs::remove_dir_all(&embed_dir).unwrap();
            }
            cp_dir(&src, &embed_dir);
        } else {
            panic!("npm build failed");
        }
    } else {
        panic!("npm not found — build frontend manually: cd web/mapit-web && npm install && npm run build");
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
