fn main() {
    // Compile the same manifests Vite consumes; no JavaScript runs in Rust.
    let directory = std::path::Path::new("../src/plugins");
    println!("cargo:rerun-if-changed={}", directory.display());
    let mut paths: Vec<_> = std::fs::read_dir(directory)
        .expect("plugin directory")
        .map(|entry| entry.expect("plugin entry").path().join("manifest.json"))
        .filter(|path| path.is_file())
        .collect();
    paths.sort();
    let mut generated = String::from("pub const MANIFESTS: &[&str] = &[\n");
    for path in paths {
        println!("cargo:rerun-if-changed={}", path.display());
        generated.push_str(&format!(
            "include_str!({:?}),\n",
            path.canonicalize().expect("manifest path")
        ));
    }
    generated.push_str("];\n");
    std::fs::write(
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("plugin_manifests.rs"),
        generated,
    )
    .expect("manifest registry");
    tauri_build::build()
}
