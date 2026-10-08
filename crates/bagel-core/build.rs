fn main() {
    // The CurseForge API key can be built in; rebuild when it changes.
    println!("cargo:rerun-if-env-changed=BAGEL_CURSEFORGE_KEY");
}
