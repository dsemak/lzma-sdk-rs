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

fn supports_lzma2_multithread(version: &str) -> bool {
    matches!(version, "19.00" | "23.01" | "26.00")
}

fn collect_existing_sources(c_root: &PathBuf, files: &[&str]) -> Vec<PathBuf> {
    files
        .iter()
        .map(|file| c_root.join(file))
        .filter(|path| path.exists())
        .collect()
}

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let version = selected_version();
    let c_root = manifest_dir.join("lzma-sdk").join(version).join("C");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let wrapper = out_dir.join("lzma-sdk-wrapper.h");
    let supports_lzma2_mt = supports_lzma2_multithread(version);

    println!("cargo:rerun-if-changed={}", c_root.display());
    println!("cargo:rustc-env=LZMA_SDK_SYS_VERSION={version}");

    let mut wrapper_headers = vec![
        "#include \"Alloc.h\"",
        "#include \"7z.h\"",
        "#include \"7zCrc.h\"",
        "#include \"Delta.h\"",
        "#include \"LzmaLib.h\"",
        "#include \"LzmaEnc.h\"",
        "#include \"LzmaDec.h\"",
        "#include \"Lzma2Enc.h\"",
        "#include \"Lzma2Dec.h\"",
        "#include \"Sha256.h\"",
        "#include \"Xz.h\"",
        "#include \"XzCrc64.h\"",
        "#include \"XzEnc.h\"",
        "#include \"Bra.h\"",
    ];

    if supports_lzma2_mt {
        wrapper_headers.push("#include \"Lzma2DecMt.h\"");
    }

    fs::write(&wrapper, wrapper_headers.join("\n"))
        .expect("failed to write bindgen wrapper header");

    let mut bindings = bindgen::Builder::default()
        .header(wrapper.display().to_string())
        .clang_arg(format!("-I{}", c_root.display()))
        .allowlist_function("Lzma.*")
        .allowlist_function("SeqInStream.*")
        .allowlist_function("Look.*")
        .allowlist_function("Sz.*")
        .allowlist_function("Crc.*")
        .allowlist_function("Delta.*")
        .allowlist_function("Sha256.*")
        .allowlist_function("Xz.*")
        .allowlist_function("Xzs.*")
        .allowlist_function("z7_.*")
        .allowlist_var("k7zSignature")
        .allowlist_var("LZMA.*")
        .allowlist_var("SHA256.*")
        .allowlist_var("SZ_.*")
        .allowlist_var("XZ_.*")
        .allowlist_var("g_Alloc")
        .allowlist_var("g_AlignedAlloc")
        .allowlist_type("SRes")
        .allowlist_type("SizeT")
        .allowlist_type("UInt16")
        .allowlist_type("UInt64")
        .allowlist_type("ISzAlloc")
        .allowlist_type("ISzAllocPtr")
        .allowlist_type("ISeqInStream")
        .allowlist_type("ISeqInStreamPtr")
        .allowlist_type("ISeqOutStream")
        .allowlist_type("ISeqOutStreamPtr")
        .allowlist_type("ISeekInStream")
        .allowlist_type("ISeekInStreamPtr")
        .allowlist_type("ILookInStream")
        .allowlist_type("ILookInStreamPtr")
        .allowlist_type("ICompressProgress")
        .allowlist_type("ICompressProgressPtr")
        .allowlist_type("CLookToRead2")
        .allowlist_type("CSz.*")
        .allowlist_type("CSha256")
        .allowlist_type("CLzma.*")
        .allowlist_type("ELzma.*")
        .allowlist_type("CXzs")
        .allowlist_type("CXzStream")
        .allowlist_type("CXzBlock")
        .allowlist_type("CXzBlockSizes")
        .allowlist_type("CXzCheck")
        .allowlist_type("CXz.*")
        .allowlist_type("ECoder.*")
        .use_core()
        .layout_tests(false)
        .generate_comments(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .raw_line("pub const SDK_VERSION: &str = env!(\"LZMA_SDK_SYS_VERSION\");");

    if supports_lzma2_mt {
        bindings = bindings.allowlist_type("CLzma2DecMt.*");
    }

    let bindings = bindings
        .generate()
        .expect("failed to generate LZMA SDK bindings");

    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("failed to write generated bindings");

    let mut sources = collect_existing_sources(
        &c_root,
        &[
            "7zAlloc.c",
            "7zArcIn.c",
            "7zBuf.c",
            "7zBuf2.c",
            "7zCrc.c",
            "7zDec.c",
            "7zStream.c",
            "Alloc.c",
            "Bcj2.c",
            "Bra.c",
            "Bra86.c",
            "BraIA64.c",
            "CpuArch.c",
            "Delta.c",
            "LzFind.c",
            "Lzma2Dec.c",
            "Lzma2Enc.c",
            "LzmaDec.c",
            "LzmaEnc.c",
            "LzmaLib.c",
            "Sha256.c",
            "Xz.c",
            "XzCrc64.c",
            "XzDec.c",
            "XzEnc.c",
            "XzIn.c",
        ],
    );

    if supports_lzma2_mt {
        sources.extend(collect_existing_sources(
            &c_root,
            &["MtDec.c", "Lzma2DecMt.c"],
        ));
    }

    for optional in ["7zCrcOpt.c", "XzCrc64Opt.c", "Sha256Opt.c", "LzFindOpt.c"] {
        let path = c_root.join(optional);
        if path.exists() {
            sources.push(path);
        }
    }

    let mut build = cc::Build::new();
    build.include(&c_root).files(sources).warnings(false);
    build.define("Z7_ST", "1").define("_7ZIP_ST", "1");
    build.compile("lzma-sdk");
}
