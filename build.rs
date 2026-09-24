fn main() {
    cc::Build::new()
        .file("src/backend/x86/codegen.c")
        .file("src/backend/x86/abi.c")
        .include("src/backend")
        .include("src/backend/x86")
        .warnings(true)
        .extra_warnings(true)
        .compile("wand2c_backend_x86");

    println!("cargo:rerun-if-changed=src/backend/x86/codegen.c");
    println!("cargo:rerun-if-changed=src/backend/x86/codegen.h");
    println!("cargo:rerun-if-changed=src/backend/x86/abi.c");
    println!("cargo:rerun-if-changed=src/backend/x86/abi.h");
}
