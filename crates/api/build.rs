fn main() {
    for name in ["SN_BUILD_COMMIT", "SN_BUILD_BRANCH", "SN_BUILD_TIME", "SN_BUILD_NUMBER", "SN_BUILD_TARGET"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
}
