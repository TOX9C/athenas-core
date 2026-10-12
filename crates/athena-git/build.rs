// Link advapi32 on Windows targets: vendored libgit2 references the
// token/SID APIs (OpenProcessToken, CheckTokenMembership, CopySid,
// EqualSid, GetLengthSid) in its fs_path owner-check code, but the git2
// crate's vendored build does not always emit the import-library link flag
// on every MSVC toolchain — the test binary then fails at link time with
// LNK2019 on those symbols. advapi32 is always present on Windows; the
// directive is a no-op elsewhere.
//
// Build scripts run on the HOST, so `#[cfg(target_env = "msvc")]` would
// test the host toolchain, not the build target — the TARGET env var is
// the supported way to gate target-specific link directives.
fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("windows") {
        println!("cargo:rustc-link-lib=dylib=advapi32");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
