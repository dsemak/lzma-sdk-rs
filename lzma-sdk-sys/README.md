# `lzma-sdk-sys`

Low-level FFI crate and mirror workspace for the official `LZMA SDK`.

## Contents

- `src/`: Rust FFI surface for the selected SDK version
- `build.rs`: compiles the vendored C sources and generates Rust bindings from `LzmaLib.h`
- `lzma-sdk/<version>/`: normalized upstream SDK snapshots
- `manifest.toml`: recorded archive hashes and normalized tree hashes
- `releases.toml`: curated list of official archive URLs and dates
- `scripts/`: deterministic import and verification helpers, including `.7z` extraction via `py7zr`

## Imported versions

- `9.20`
- `16.04`
- `19.00`
- `23.01`
- `26.00`

## Update flow

```bash
python3 -m pip install -r lzma-sdk-sys/scripts/requirements.txt
python3 lzma-sdk-sys/scripts/import-lzma-sdk.py --all
python3 lzma-sdk-sys/scripts/verify-upstream.py --all --download-missing
cargo test
```

The canonical upstream pages are:

- [https://www.7-zip.org/sdk.html](https://www.7-zip.org/sdk.html)
- [https://www.7-zip.org/download.html](https://www.7-zip.org/download.html)

`lzma-sdk-sys/src/lib.rs` is only a thin include wrapper. The actual FFI items are generated automatically during build from the selected upstream `LzmaLib.h`.
