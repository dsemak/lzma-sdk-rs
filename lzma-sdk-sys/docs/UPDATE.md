# Updating the Mirrored SDK

## Requirements

You only need a few things:

- Python 3.12+
- `python3 -m pip install -r lzma-sdk-sys/scripts/requirements.txt`
- `cargo`

## Procedure

1. Import the official SDK archives.

   To import every release we track:

   ```bash
   python3 lzma-sdk-sys/scripts/import-lzma-sdk.py --all
   ```

   To import specific versions instead:

   ```bash
   python3 lzma-sdk-sys/scripts/import-lzma-sdk.py --version 26.00 --version 23.01
   ```

2. Verify the imported snapshots against the recorded manifest.

   ```bash
   python3 lzma-sdk-sys/scripts/verify-upstream.py --all --download-missing
   ```

3. Run the Rust tests.

   ```bash
   cargo test
   ```

4. Commit the imported sources together with any wrapper updates.

   ```bash
   git add lzma-sdk-sys src Cargo.toml README.md docs .github
   git commit -m "Import LZMA SDK 26.00"
   ```

## Policy

- Only import archives linked from the official 7-Zip site.
- Record the final archive URL, published date, archive format, `sha256`, and normalized tree hash in `lzma-sdk-sys/manifest.toml`.
- Leave imported source trees untouched apart from deterministic extraction and filesystem normalization.
- When a new upstream release appears, add it to `lzma-sdk-sys/releases.toml`, import it, verify it, and update the feature matrix if the Rust API should expose it.
