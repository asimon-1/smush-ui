# smush-ui

`smush-ui` is a Rust library and command-line tool for Nintendo Switch UI files. It reads,
edits, and writes BFLYT layouts as JSON and reads, writes, unpacks, repacks, and edits raw
little-endian SARC archives stored with the `.arc` extension.

The project makes layout data inspectable and editable without working directly with binary offsets and packed fields. JSON output represents pane and group nesting with `children` arrays, while the binary writer recreates the required boundary sections and derived file metadata.

## Future Goals

Future goals include support for other file formats:

- BNTX (Textures)
- BFFNT (Fonts)
- BFLIM (Layout Images)
- BFLAN (Layout Animations)

## What it supports

- BFLYT parsing and writing.
- JSON conversion through the `smush-ui` CLI.
- Typed models for the BFLYT sections currently used by the included Switch layout fixtures.
- Nested pane and group hierarchies in JSON.
- Little-endian SARC version `0x0100` parsing and deterministic writing.
- SARC directory unpacking and packing, plus insertion or replacement of individual entries.

## Installation

```sh
cargo build --release
```

Run the test suite with:

```sh
cargo test
```

## Command-line usage

Convert a BFLYT file to formatted JSON:

```sh
cargo run -- to-json input.bflyt output.json
```

Convert edited JSON back to BFLYT:

```sh
cargo run -- from-json input.json output.bflyt
```

Unpack a raw SARC archive into a directory:

```sh
cargo run -- unpack-arc input.arc output-directory
```

Pack a directory tree into a raw SARC archive:

```sh
cargo run -- pack-arc input-directory output.arc
```

Add a file under a chosen archive path, leaving the input archive unchanged:

```sh
cargo run -- add-arc-file input.arc source-file timg/new-file.bin output.arc
```

Pass `--replace` to replace an existing entry with the same archive path. The input and output
paths may be identical for this command; replacement is atomic.

SARC entries are extracted and packed as raw files. The tool does not decode embedded BFLYT,
BNTX, BFLAN, or other entry payloads during archive operations. Compressed wrappers such as
zstd-compressed `.zs` files and Yaz0 are outside the current archive workflow; decompress them
before using these commands.

The deterministic writer aligns the data section and ordinary entry payloads to absolute `0x10`
boundaries. Archives containing `.bntx` or `.bnsh` entries promote the data section and those GPU resource payloads to absolute `0x1000` boundaries.

## Library usage

Use `Bflyt` with `binrw` to read or write binary layouts, and `serde_json` to edit their JSON representation:

```rust
use std::io::Cursor;

use binrw::{BinReaderExt, BinWriterExt};
use smush_ui::bflyt::Bflyt;

let bytes = std::fs::read("input.bflyt")?;
let layout: Bflyt = Cursor::new(bytes).read_le()?;

let json = serde_json::to_string_pretty(&layout)?;
let edited: Bflyt = serde_json::from_str(&json)?;

let mut output = Cursor::new(Vec::new());
output.write_le(&edited)?;
std::fs::write("output.bflyt", output.into_inner())?;
```

`filesize` and the binary section count are derived when the layout is written. They are intentionally omitted from normal JSON output.

## Validation

The integration tests cover section serialization, BFLYT binary-to-JSON-to-binary round trips,
and SARC parsing, deterministic writing, filesystem workflows, entry insertion, and CLI behavior.
Extracted proprietary files are used only as ignored local validation inputs.

## References

The following projects were used as references:

- [Toolbox-Cli](https://github.com/crashbandicode/Toolbox-Cli)
- [nnbfl](https://github.com/crackheadakira/nnbfl)
- [Nintendo-File-Formats](https://github.com/kinnay/Nintendo-File-Formats)
- [Colony](https://github.com/KibbeWater/Colony)
- [3DSkit](https://github.com/Tyulis/3DSkit)
- [bflyt-rs](https://github.com/ultimate-research/bflyt-rs)
- [Nintendo-UI-Studio](https://github.com/Arulo165/Nintendo-UI-Studio)
- [ARCropolis](https://github.com/Raytwo/ARCropolis)
