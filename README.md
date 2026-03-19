# `lzma-sdk-rs`

Rust wrappers around the official `LZMA SDK`.

The goal of this crate is pretty simple: expose the SDK in a way that feels like normal Rust,
without hiding the low-level pieces you still need when working with raw `LZMA`, `LZMA2`, or `XZ`
streams.

Upstream references:

- SDK page: [https://www.7-zip.org/sdk.html](https://www.7-zip.org/sdk.html)
- Download page: [https://www.7-zip.org/download.html](https://www.7-zip.org/download.html)

This repository is split into a few straightforward parts:

- `src/` with the safe Rust API
- `lzma-sdk-sys/` with vendored upstream snapshots, FFI generation, build glue, and import tooling
- `docs/` with update and source verification notes

## API Overview

There are two ways to use the crate.

- If you want a consistent entry point, start with `Stream` and build owned encoders or decoders from it.
- If you are working directly with format details, use `lzma_sdk::lzma`, `lzma_sdk::lzma2`, or `lzma_sdk::xz`.

That split is intentional. The high-level API keeps the common path simple, while the format
modules still let you get at typed options, raw properties, and per-format state when you need it.

## High-Level Example

```rust
use lzma_sdk::{LzmaAction, Stream, XzOptions};

let stream = Stream::new();
let mut encoder = stream.encoder(XzOptions::default());

let input = b"hello from lzma-sdk";
let mut compressed = Vec::new();
let mut encoded_chunk = [0_u8; 64];

let (_, written) = encoder.process(input, &mut encoded_chunk, LzmaAction::Finish)?;
compressed.extend_from_slice(&encoded_chunk[..written]);

while !encoder.is_finished() {
    let (_, written) = encoder.process(&[], &mut encoded_chunk, LzmaAction::Finish)?;
    compressed.extend_from_slice(&encoded_chunk[..written]);
}

let stream = Stream::new();
let mut decoder = stream.decoder()?;
let mut restored = Vec::new();
let mut offset = 0;
let mut decoded_chunk = [0_u8; 64];

loop {
    let action = if offset == compressed.len() {
        LzmaAction::Finish
    } else {
        LzmaAction::Run
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

## Format-Specific Examples

### Raw `LZMA`

```rust
use lzma_sdk::{
    CompressionLevel, EndMarkerMode, FastBytes, LzmaAction, LzmaOptions, Stream,
};

let options = LzmaOptions::builder()
    .level(CompressionLevel::new(7)?)
    .fast_bytes(FastBytes::new(64)?)
    .end_marker(EndMarkerMode::Enabled)
    .build();

let payload = b"example payload".repeat(128);
let stream = Stream::new();
let mut encoder = stream.raw_encoder(options);
let mut encoded_chunk = vec![0_u8; payload.len() + 1024];
let mut compressed = Vec::new();

let (_, written) = encoder.process(&payload, &mut encoded_chunk, LzmaAction::Finish)?;
compressed.extend_from_slice(&encoded_chunk[..written]);

while !encoder.is_finished() {
    let (_, written) = encoder.process(&[], &mut encoded_chunk, LzmaAction::Finish)?;
    compressed.extend_from_slice(&encoded_chunk[..written]);
}

let encoded = encoder.encoded().ok_or(lzma_sdk::Error::Param)?.clone();
let raw_props = encoded.props;
let typed_props = encoded.properties()?;
let raw_payload = encoded.compressed;

assert_eq!(raw_props, typed_props.encode());
assert_eq!(compressed, raw_payload);

let stream = Stream::new();
let mut decoder = stream.raw_decoder(&typed_props)?;
let mut restored = Vec::new();
let mut decoded_chunk = [0_u8; 64];
let mut offset = 0;

loop {
    let action = if offset == raw_payload.len() {
        LzmaAction::Finish
    } else {
        LzmaAction::Run
    };
    let (consumed, written) = decoder.process(&raw_payload[offset..], &mut decoded_chunk, action)?;
    offset += consumed;
    restored.extend_from_slice(&decoded_chunk[..written]);
    if decoder.is_finished() {
        break;
    }
}

assert_eq!(restored, payload);
# Ok::<(), lzma_sdk::Error>(())
```

### `LZMA2`

```rust
use lzma_sdk::{Lzma2Options, LzmaAction, Stream};

let payload = b"lzma2 payload".repeat(256);
let stream = Stream::new();
let mut encoder = stream.lzma2_encoder(Lzma2Options::default());
let mut encoded_chunk = vec![0_u8; payload.len() + 1024];
let mut compressed = Vec::new();

let (_, written) = encoder.process(&payload, &mut encoded_chunk, LzmaAction::Finish)?;
compressed.extend_from_slice(&encoded_chunk[..written]);

while !encoder.is_finished() {
    let (_, written) = encoder.process(&[], &mut encoded_chunk, LzmaAction::Finish)?;
    compressed.extend_from_slice(&encoded_chunk[..written]);
}

let encoded = encoder.encoded().ok_or(lzma_sdk::Error::Param)?.clone();
let property = encoded.property;
let raw_payload = encoded.compressed;

assert_eq!(compressed, raw_payload);

let stream = Stream::new();
let mut decoder = stream.lzma2_decoder(property)?;
let mut restored = Vec::new();
let mut decoded_chunk = [0_u8; 64];
let mut offset = 0;

loop {
    let action = if offset == raw_payload.len() {
        LzmaAction::Finish
    } else {
        LzmaAction::Run
    };
    let (consumed, written) = decoder.process(&raw_payload[offset..], &mut decoded_chunk, action)?;
    offset += consumed;
    restored.extend_from_slice(&decoded_chunk[..written]);
    if decoder.is_finished() {
        break;
    }
}

assert_eq!(restored, payload);
# Ok::<(), lzma_sdk::Error>(())
```

### `XZ`

```rust
use lzma_sdk::{LzmaAction, Stream, XzOptions};

let payload = b"xz payload".repeat(256);
let stream = Stream::new();
let mut encoder = stream.encoder(XzOptions::default());
let mut encoded_chunk = vec![0_u8; payload.len() + 1024];
let mut compressed = Vec::new();

let (_, written) = encoder.process(&payload, &mut encoded_chunk, LzmaAction::Finish)?;
compressed.extend_from_slice(&encoded_chunk[..written]);

while !encoder.is_finished() {
    let (_, written) = encoder.process(&[], &mut encoded_chunk, LzmaAction::Finish)?;
    compressed.extend_from_slice(&encoded_chunk[..written]);
}

let encoded = encoder.encoded().ok_or(lzma_sdk::Error::Param)?.clone();
let raw_payload = encoded.compressed;

assert_eq!(compressed, raw_payload);

let stream = Stream::new();
let mut decoder = stream.decoder()?;
let mut restored = Vec::new();
let mut decoded_chunk = [0_u8; 64];
let mut offset = 0;

loop {
    let action = if offset == raw_payload.len() {
        LzmaAction::Finish
    } else {
        LzmaAction::Run
    };
    let (consumed, written) = decoder.process(&raw_payload[offset..], &mut decoded_chunk, action)?;
    offset += consumed;
    restored.extend_from_slice(&decoded_chunk[..written]);
    if decoder.is_finished() {
        break;
    }
}

assert_eq!(restored, payload);
# Ok::<(), lzma_sdk::Error>(())
```

## Notes

- Raw `LZMA` gives you typed props plus the usual tuning knobs like algorithm, match finder mode,
  hash bytes, and end-marker handling.
- Once an encoder finishes, you can keep the raw pieces and reuse them later:
  `LzmaCompressedData::props`, `LzmaCompressedData::compressed`,
  `Lzma2CompressedData::property`, `Lzma2CompressedData::compressed`, and
  `XzCompressedData::compressed`.
- Raw `LZMA` streams without an end marker are awkward to decode when the size is unknown, so the
  reliable path is to use an end marker.
- Encoders created through the owned API still buffer input until `LzmaAction::Finish`. That is a
  limitation of the current safe wrapper over the one-shot SDK encode primitives. Decoding is
  actually incremental.
- The vendored SDK is built in single-thread mode right now. Thread-count options are exposed for
  compatibility and future work, but they should not be treated as real multi-threaded execution
  today.

## Feature Flags

Pick exactly one mirrored SDK version feature:

- `sdk-9-20`
- `sdk-16-04`
- `sdk-19-00`
- `sdk-23-01`
- `sdk-26-00`

The default is `sdk-26-00`. These features are mutually exclusive.

## Updating The Vendored SDK

If you need to refresh the vendored SDK or verify where a snapshot came from:

- see `lzma-sdk-sys/README.md` for the mirror layout and import flow
- see `lzma-sdk-sys/docs/UPDATE.md` and `lzma-sdk-sys/docs/SOURCE-VERIFICATION.md` for the
  operational details

## License

According to the official SDK page, the upstream `LZMA SDK` is in the public domain.

This repository follows the same model. See `LICENSE`.
