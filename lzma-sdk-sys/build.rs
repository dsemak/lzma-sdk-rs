use std::env;
use std::fs;
use std::path::PathBuf;

const FEATURE_VERSIONS: &[(&str, &str)] = &[
    ("CARGO_FEATURE_SDK_9_20", "9.20"),
    ("CARGO_FEATURE_SDK_16_04", "16.04"),
    ("CARGO_FEATURE_SDK_19_00", "19.00"),
    ("CARGO_FEATURE_SDK_23_01", "23.01"),
    ("CARGO_FEATURE_SDK_26_00", "26.00"),
];

fn selected_version() -> &'static str {
    let selected: Vec<&str> = FEATURE_VERSIONS
        .iter()
        .filter_map(|(feature, version)| env::var_os(feature).map(|_| *version))
        .collect();

    match selected.as_slice() {
        [version] => version,
        [] => panic!("no SDK version feature selected"),
        _ => panic!("multiple SDK version features selected"),
    }
}

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let version = selected_version();
    let c_root = manifest_dir.join("lzma-sdk").join(version).join("C");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let wrapper = out_dir.join("lzma-sdk-wrapper.h");

    println!("cargo:rerun-if-changed={}", c_root.display());
    println!("cargo:rustc-env=LZMA_SDK_SYS_VERSION={version}");
    fs::write(
        &wrapper,
        [
            "#include \"Alloc.h\"",
            "#include \"7zCrc.h\"",
            "#include \"LzmaLib.h\"",
            "#include \"LzmaEnc.h\"",
            "#include \"LzmaDec.h\"",
            "#include \"Lzma2Enc.h\"",
            "#include \"Lzma2Dec.h\"",
            "#include \"Xz.h\"",
            "#include \"XzCrc64.h\"",
            "#include \"XzEnc.h\"",
            "#include \"Bra.h\"",
        ]
        .join("\n"),
    )
    .expect("failed to write bindgen wrapper header");

    let bindings = bindgen::Builder::default()
        .header(wrapper.display().to_string())
        .clang_arg(format!("-I{}", c_root.display()))
        .allowlist_function("Lzma.*")
        .allowlist_function("Crc.*")
        .allowlist_function("Xz.*")
        .allowlist_function("Xzs.*")
        .allowlist_function("z7_.*")
        .allowlist_var("LZMA.*")
        .allowlist_var("SZ_.*")
        .allowlist_var("XZ_.*")
        .allowlist_var("g_Alloc")
        .allowlist_var("g_AlignedAlloc")
        .allowlist_type("SRes")
        .allowlist_type("SizeT")
        .allowlist_type("ISzAlloc")
        .allowlist_type("ISzAllocPtr")
        .allowlist_type("ISeqInStream")
        .allowlist_type("ISeqInStreamPtr")
        .allowlist_type("ISeqOutStream")
        .allowlist_type("ISeqOutStreamPtr")
        .allowlist_type("ICompressProgress")
        .allowlist_type("ICompressProgressPtr")
        .allowlist_type("CLzma.*")
        .allowlist_type("ELzma.*")
        .allowlist_type("CXz.*")
        .allowlist_type("ECoder.*")
        .use_core()
        .layout_tests(false)
        .generate_comments(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .raw_line("pub const SDK_VERSION: &str = env!(\"LZMA_SDK_SYS_VERSION\");")
        .generate()
        .expect("failed to generate LZMA SDK bindings");
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("failed to write generated bindings");

    let mut sources = vec![
        c_root.join("7zAlloc.c"),
        c_root.join("7zBuf.c"),
        c_root.join("7zBuf2.c"),
        c_root.join("7zCrc.c"),
        c_root.join("7zStream.c"),
        c_root.join("Alloc.c"),
        c_root.join("Bra.c"),
        c_root.join("Bra86.c"),
        c_root.join("BraIA64.c"),
        c_root.join("CpuArch.c"),
        c_root.join("Delta.c"),
        c_root.join("LzFind.c"),
        c_root.join("Lzma2Dec.c"),
        c_root.join("Lzma2Enc.c"),
        c_root.join("LzmaDec.c"),
        c_root.join("LzmaEnc.c"),
        c_root.join("LzmaLib.c"),
        c_root.join("Sha256.c"),
        c_root.join("Xz.c"),
        c_root.join("XzCrc64.c"),
        c_root.join("XzDec.c"),
        c_root.join("XzEnc.c"),
        c_root.join("XzIn.c"),
    ];
    for optional in ["7zCrcOpt.c", "XzCrc64Opt.c", "Sha256Opt.c", "LzFindOpt.c"] {
        let path = c_root.join(optional);
        if path.exists() {
            sources.push(path);
        }
    }

    cc::Build::new()
        .include(&c_root)
        .define("Z7_ST", "1")
        .define("_7ZIP_ST", "1")
        .files(sources)
        .warnings(false)
        .compile("lzma-sdk");
}
