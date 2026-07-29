use std::process::Command;

fn main() {
    // Compile the macOS Vision bridge used by the CLI.
    #[cfg(target_os = "macos")]
    compile_swift_bridge();
}

#[cfg(target_os = "macos")]
fn compile_swift_bridge() {
    let swift_src = "swift/MattingBridge.swift";
    let out_dir =
        std::env::var("OUT_DIR").expect("OUT_DIR is not set — build.rs must be run by Cargo.");

    let obj_path = format!("{}/MattingBridge.o", out_dir);
    let lib_path = format!("{}/libmatting_bridge.a", out_dir);

    // ── Detect Cargo target architecture ────────────────────────────────────
    let cargo_arch =
        std::env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH is not set by Cargo");
    let arch = match cargo_arch.as_str() {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        value => panic!("Unsupported macOS target architecture: {value}"),
    };
    let target_triple = format!("{}-apple-macosx14.0", arch);

    // ── SDK path ────────────────────────────────────────────────────────────
    let sdk_output = Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()
        .expect("xcrun failed — make sure Xcode or Command Line Tools are installed");
    let sdk_path = String::from_utf8(sdk_output.stdout)
        .unwrap()
        .trim()
        .to_string();

    // ── Compile Swift → object file ─────────────────────────────────────────
    let swiftc_status = Command::new("swiftc")
        .args([
            "-emit-object",
            "-parse-as-library",
            "-module-name",
            "MattingBridge",
            "-target",
            &target_triple,
            "-sdk",
            &sdk_path,
            swift_src,
            "-o",
            &obj_path,
        ])
        .status()
        .expect("Failed to invoke swiftc");

    assert!(
        swiftc_status.success(),
        "Swift compilation failed. Check cli/swift/MattingBridge.swift for errors."
    );

    // ── Archive object → static library ────────────────────────────────────
    let ar_status = Command::new("ar")
        .args(["rcs", &lib_path, &obj_path])
        .status()
        .expect("Failed to invoke ar");

    assert!(ar_status.success(), "ar archiving failed");

    // ── Cargo link directives ───────────────────────────────────────────────
    println!("cargo:rustc-link-search=native={}", out_dir);
    println!("cargo:rustc-link-lib=static=matting_bridge");

    // Locate the Xcode toolchain Swift lib dir.
    // e.g. /Applications/Xcode.app/.../usr/lib/swift/macosx
    // Needed at *link time* for compatibility shims like swiftCompatibility56.
    let swiftc_output = Command::new("xcrun")
        .args(["--find", "swiftc"])
        .output()
        .expect("xcrun --find swiftc failed");
    let swiftc_bin = String::from_utf8(swiftc_output.stdout)
        .unwrap()
        .trim()
        .to_string();

    let toolchain_swift_lib = std::path::Path::new(&swiftc_bin)
        .parent() // bin/
        .and_then(|p| p.parent()) // usr/
        .map(|p| format!("{}/lib/swift/macosx", p.display()))
        .expect("Could not resolve Swift toolchain lib path");

    // Link-time search: toolchain dir first (has swiftCompatibility56 etc.),
    // then /usr/lib/swift as a fallback for the main runtime dylibs.
    println!("cargo:rustc-link-search=native={}", toolchain_swift_lib);
    println!("cargo:rustc-link-search=native=/usr/lib/swift");
    println!("cargo:rustc-link-lib=dylib=swiftCore");
    println!("cargo:rustc-link-lib=dylib=swiftFoundation");

    // Runtime Swift libraries are provided by macOS 14+.
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

    // Apple system frameworks required by MattingBridge.swift
    println!("cargo:rustc-link-lib=framework=Vision");
    println!("cargo:rustc-link-lib=framework=CoreImage");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=CoreGraphics");

    // Rebuild trigger
    println!("cargo:rerun-if-changed={}", swift_src);
}
