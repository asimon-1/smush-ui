# smush-ui

`smush-ui` is a Rust library and command-line tool for reading, editing, and writing Nintendo BFLYT layout files as JSON. It targets the little-endian layouts used by Nintendo Switch games and provides a typed model for layout sections, pane hierarchies, materials, textures, text, windows, groups, and related metadata.

The project makes layout data inspectable and editable without working directly with binary offsets and packed fields. JSON output represents pane and group nesting with `children` arrays, while the binary writer recreates the required boundary sections and derived file metadata.

## Future Goals

Future goals include support for other file formats:

- BNTX (Textures)
- BFFNT (Fonts)
- BFLIM (Layout Images)
- BFLAN (Layout Animations)
- SARC (Archives)

## What it supports

- BFLYT parsing and writing.
- JSON conversion through the `smush-ui` CLI.
- Typed models for the BFLYT sections currently used by the included Switch layout fixtures.
- Nested pane and group hierarchies in JSON.

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

The integration tests cover unit-level section serialization as well as binary-to-JSON-to-binary round trips for the checked-in BFLYT fixtures. A variety of extracted BFLYT files have been used for (de)serialization testing.

## References

The following projects were used as references. There might be some overlap in functionalit.

- [Toolbox-Cli](https://github.com/crashbandicode/Toolbox-Cli)
- [nnbfl](https://github.com/crackheadakira/nnbfl)
- [Nintendo-File-Formats](https://github.com/kinnay/Nintendo-File-Formats)
- [Colony](https://github.com/KibbeWater/Colony)
- [3DSkit](https://github.com/Tyulis/3DSkit)
- [bflyt-rs](https://github.com/ultimate-research/bflyt-rs)
- [Nintendo-UI-Studio](https://github.com/Arulo165/Nintendo-UI-Studio)