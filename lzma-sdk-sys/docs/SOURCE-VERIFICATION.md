# Source Verification

For `LZMA SDK` source archives, the official 7-Zip website is the only source of truth:

- [https://www.7-zip.org/sdk.html](https://www.7-zip.org/sdk.html)
- [https://www.7-zip.org/download.html](https://www.7-zip.org/download.html)

## What we record

Each release in `lzma-sdk-sys/manifest.toml` stores the data we need to prove where it came from and what was imported:

- `version`
- `published`
- `archive_url`
- `archive_file`
- `archive_format`
- `archive_sha256`
- `tree_sha256`
- `source_page`

The static expectations live in `lzma-sdk-sys/releases.toml`, including known release dates and official URLs. The manifest is the audit trail produced by importing those releases.

`.7z` archives are unpacked by the Python import helpers via `py7zr`, while `tar.bz2` archives use the standard library.

## How the tree hash works

The normalized tree hash is a SHA-256 digest of the extracted source tree. It is designed to stay stable across machines and extraction environments, so it ignores metadata that should not matter.

The verifier applies these rules:

- process files in sorted path order
- store paths relative to the version directory
- preserve line endings exactly as extracted
- skip directories themselves
- normalize file modes to either `644` or `755`

For every file, the verifier hashes:

1. the normalized relative path
2. the normalized octal mode
3. the raw file bytes

This keeps the result independent of timestamps and other host-specific extraction details.

## Manual review checklist

When you import a new release:

1. Make sure the archive link comes from an official 7-Zip page.
2. Recompute the archive `sha256`.
3. Recompute the normalized extracted tree hash.
4. Confirm the imported directory is `lzma-sdk-sys/lzma-sdk/<version>/`.
5. Run `cargo test` after the import.
