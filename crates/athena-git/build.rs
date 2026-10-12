// Link advapi32 on Windows: vendored libgit2 references the token/SID APIs
// (OpenProcessToken, CheckTokenMembership, CopySid, EqualSid, GetLengthSid)
// in its fs_path owner-check code, but the git2 crate's vendored build does
// not always emit the import-library link flag on every MSVC toolchain —
// the test binary then fails at link time with LNK2019 on those symbols.
// advapi32 is always present on Windows; the directive is a no-op elsewhere.
fn main() {
    println!("cargo:rustc-link-lib=dylib=advapi32");
    println!("cargo:rerun-if-changed=build.rs");
}
