//! Copies the built viewer (viewer/dist) into OUT_DIR, where `include_dir!`
//! embeds it into the acu binary. Without a built viewer, it writes a page
//! that says how to build one, so that `cargo build` still succeeds.

use std::{env, fs, path::Path, path::PathBuf};

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let src = manifest.join("../viewer/dist");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("viewer");
    println!("cargo:rerun-if-changed=../viewer/dist");
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out).unwrap();
    if src.join("index.html").exists() {
        copy_dir(&src, &out);
    } else {
        println!("cargo:warning=viewer/dist is missing; acu will embed a placeholder page. Run `npm run build` in viewer/ first.");
        fs::write(
            out.join("index.html"),
            "<!doctype html><meta charset=utf-8><title>Atlas Carries Us</title>\
             <p>This acu binary holds no viewer. Build it with <code>npm ci &amp;&amp; npm run build</code> \
             in <code>viewer/</code>, then rebuild acu.</p>",
        )
        .unwrap();
    }
}
