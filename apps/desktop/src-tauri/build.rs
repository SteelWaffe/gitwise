fn main() {
    // Baked in by CI so the GitHub sign-in works out of the box; empty in local builds.
    println!("cargo:rerun-if-env-changed=GITWISE_GITHUB_CLIENT_ID");
    tauri_build::build()
}
