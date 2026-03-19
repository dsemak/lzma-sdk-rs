# `lzma-sdk-rs`

Safe Rust wrappers around the official `LZMA SDK`.

Official upstream:

- SDK page: [https://www.7-zip.org/sdk.html](https://www.7-zip.org/sdk.html)
- Download page: [https://www.7-zip.org/download.html](https://www.7-zip.org/download.html)

This repository contains:

- `src/` with the safe Rust API
- `lzma-sdk-sys/` with vendored upstream snapshots, FFI generation, build glue, and import tooling
- `docs/` with update and source verification notes

## API shape

The crate now has two complementary surfaces.

- The primary safe API is built around owned `Stream`, `Encoder`, and `Decoder` types.
- The format modules `lzma_sdk::lzma`, `lzma_sdk::lzma2`, and `lzma_sdk::xz` remain available as direct compatibility layers with one-shot helpers and format-specific state.

This layout is meant to make future integration with `lzma-safe`-style code much less painful while preserving the current module-oriented helpers.

## High-level example

```rust
use lzma_sdk::{Action, Stream};
use lzma_sdk::encoder::options::XzOptions;

let stream = Stream::new();
let mut encoder = stream.encoder(XzOptions::default())?;

let input = b"hello from lzma-sdk";
let mut compressed = Vec::new();
let mut encoded_chunk = [0_u8; 64];

let (_, written) = encoder.process(input, &mut encoded_chunk, Action::Finish)?;
compressed.extend_from_slice(&encoded_chunk[..written]);

while !encoder.is_finished() {
    let (_, written) = encoder.process(&[], &mut encoded_chunk, Action::Finish)?;
    compressed.extend_from_slice(&encoded_chunk[..written]);
}

let stream = Stream::new();
let mut decoder = stream.decoder()?;
let mut restored = Vec::new();
let mut offset = 0;
let mut decoded_chunk = [0_u8; 64];

loop {
    let action = if offset == compressed.len() {
        Action::Finish
    } else {
        Action::Run
    };
    let (consumed, written) = decoder.process(&compressed[offset..], &mut decoded_chunk, action)?;
    offset += consumed;
    restored.extend_from_slice(&decoded_chunk[..written]);
    if decoder.is_finished() {
        break;
    }
}

assert_eq!(restored, input);
# Ok::<(), lzma_sdk::Error>(())
```

## Format-specific examples

### Raw `LZMA`

```rust
use lzma_sdk::{
    CompressionLevel, CompressionOptions, EndMarkerMode, FastBytes, decompress_data,
    encode_with_options,
};

let options = CompressionOptions::builder()
    .level(CompressionLevel::new(7)?)
    .fast_bytes(FastBytes::new(64)?)
    .end_marker(EndMarkerMode::Enabled)
    .build();

let payload = b"example payload".repeat(128);
let compressed = encode_with_options(&payload, &options)?;
let restored = decompress_data(&compressed)?;

assert_eq!(restored, payload);
# Ok::<(), lzma_sdk::Error>(())
```

### `LZMA2`

```rust
use lzma_sdk::lzma2;

let payload = b"lzma2 payload".repeat(256);
let compressed = lzma2::encode(&payload)?;
let restored = compressed.decompress()?;

assert_eq!(restored, payload);
# Ok::<(), lzma_sdk::Error>(())
```

### `XZ`

```rust
use lzma_sdk::xz;

let payload = b"xz payload".repeat(256);
let compressed = xz::encode(&payload)?;
let restored = xz::decode(&compressed.compressed)?;

assert_eq!(restored.output, payload);
# Ok::<(), lzma_sdk::Error>(())
```

## Notes

- Raw `LZMA` exposes typed props and advanced encoder flags such as algorithm, match finder mode, hash bytes, and end-marker handling.
- Unknown-size raw `LZMA` decoding is reliable only for streams with an end marker.
- The new owned encoders currently buffer input until `Action::Finish`, because the safe layer still builds on one-shot SDK encode primitives. The decoder side is genuinely incremental.
- `decode_to_writer()` and `write_decompressed_to()` now stream decoded output to the destination writer instead of buffering the whole decoded payload first.
- The vendored SDK is currently built in single-thread mode. Thread-count fields are exposed for forward compatibility, but multi-thread execution should not be relied on yet.

## Feature flags

Select exactly one mirrored SDK version feature:

- `sdk-9-20`
- `sdk-16-04`
- `sdk-19-00`
- `sdk-23-01`
- `sdk-26-00`

The default feature is `sdk-26-00`. These features are mutually exclusive.

## Update

See `lzma-sdk-sys/README.md` for the mirror layout and reproducible import flow.
See `lzma-sdk-sys/docs/UPDATE.md` and `lzma-sdk-sys/docs/SOURCE-VERIFICATION.md` for the operational details.

## License

The upstream `LZMA SDK` is public domain according to the official SDK page.

This repository follows the same licensing model. See `LICENSE`.
