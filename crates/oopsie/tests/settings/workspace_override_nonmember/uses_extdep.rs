// A `.cargo/config.toml [env] CARGO_WORKSPACE_DIR` override in this workspace
// must not leak into `extdep`, a non-member path dependency compiled inside
// this same build; `extdep::DepError` is 8 bytes, well over the workspace's
// max-size = 4, so it only builds if the cap stayed out.
fn main() {
    let _ = extdep::g();
}
