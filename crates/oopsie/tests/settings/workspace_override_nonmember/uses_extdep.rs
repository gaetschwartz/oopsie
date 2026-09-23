// A CARGO_WORKSPACE_DIR override must not apply the workspace's max-size cap to extdep, a non-member path dependency.
fn main() {
    let _ = extdep::g();
}
