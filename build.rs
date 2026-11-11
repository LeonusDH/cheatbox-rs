fn main() {
    // re-embed sheets when files are added/changed
    println!("cargo:rerun-if-changed=sheets");
}
