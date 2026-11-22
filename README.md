# smush-ui

`smush-ui` is a Rust library and command-line tool for Nintendo Switch UI files. It reads and edits
BFLYT layouts as JSON, reads and edits little-endian SARC archives, and provides lossless and PNG
workflows for version-4 NX BNTX texture containers.

JSON makes layouts editable without requiring users to work with binary offsets or packed fields.
It represents pane and group nesting with `children` arrays. The binary writer recreates boundary
sections and derives file metadata.

## Future goals

Planned formats:

- BFFNT (Fonts)
- BFLIM (Layout Images)
- BFLAN (Layout Animations)

## What it supports

- BFLYT
  - Parsing and writing
  - Serialization to and from JSON
- BNTX
  - Parsing and writing
  - Lossless unpacking and repacking
  - Texture insertion or replacement
- SARC
  - Unpacking and repacking
  - Insertion or replacement of individual entries in an existing archive

## Installation

```sh
cargo build --release
```

Run the test suite with:

```sh
cargo test
```

## Command-line usage

```text
Usage: smush-ui.exe <COMMAND>

Commands:
  to-json           Deserialize a little-endian BFLYT file to pretty-printed JSON
  from-json         Serialize JSON to a little-endian BFLYT file
  unpack-arc        Extract the named files in a SARC archive
  pack-arc          Pack a directory tree into a SARC archive
  add-arc-file      Insert an opaque file into an existing SARC archive
  list-arc          List files stored in a SARC archive
  diff-arc          Compare the files stored in two SARC archives
  unpack-bntx       Extract a BNTX container to a lossless editable bundle
  pack-bntx         Rebuild a BNTX container from a lossless bundle
  add-bntx-texture  Add or replace one PNG texture in a BNTX container
  create-bntx       Create a new BNTX container from one directory of PNG files
  list-bntx         List textures stored in a BNTX container
  diff-bntx         Compare textures stored in two BNTX containers

Options:
  -h, --help  Print help
```

Run `smush-ui.exe <COMMAND> --help` to see a command's arguments and options. The examples below
use `cargo run --` to run a development build.
Commands that write files print nothing on success. The list and diff commands print to stdout.

### BFLYT layouts

Convert a BFLYT file to formatted JSON:

```sh
cargo run -- to-json layout.bflyt layout.json
```

Convert edited JSON back to BFLYT:

```sh
cargo run -- from-json layout.json rebuilt-layout.bflyt
```

### SARC archives

Unpack a raw SARC archive into a directory:

```sh
cargo run -- unpack-arc archive.arc archive-files
```

Pack a directory tree into a raw SARC archive:

```sh
cargo run -- pack-arc archive-files rebuilt.arc
```

Add a file under a chosen archive path, leaving the input archive unchanged:

```sh
cargo run -- add-arc-file archive.arc asset.bin assets/asset.bin updated.arc
```

Pass `--replace` to replace an existing entry with the same archive path. The input and output
paths may be identical for this command; replacement is atomic.

Inspect an archive without extracting it, or compare two archives by entry identity and SHA-256:

```sh
cargo run -- list-arc archive.arc
cargo run -- diff-arc original.arc modified.arc
```

For an archive containing `a.bin` with the bytes `abc`, `list-arc` prints:

```text
entries: 1
[0] a.bin size=3 sha256=ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad kind=opaque extension=bin
```

If `a.bin` changes from `01` to `02`, `diff-arc` prints:

```text
entries: 1 -> 1; added=0 removed=0 changed=1
~ a.bin size=1->1 sha256=4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a->dbc1b4c900ffe48d575b5da5c638040125f65db0fe3e24494b76ea986457d986
```

Pass `--format json` for versioned JSON output in a stable format for CI. `diff-arc` accepts
`--expand-bflyt` and `--expand-bntx` to show field or texture changes when both payloads parse as
the selected format. Use `--check` with either diff command to return status `3` when files differ.
Parse and I/O failures return status `1`.

SARC entries are extracted and packed as raw files. The tool does not decode embedded BFLYT,
BNTX, BFLAN, or other entry payloads during mutating archive operations. Only the opt-in
`diff-arc` expansion flags parse embedded BFLYT or BNTX payloads. If parsing fails, the command
reports the outer payload difference. Decompress zstd `.zs` files and Yaz0 files before using these
commands.

The writer aligns the data section and ordinary entry payloads to absolute `0x10` boundaries. If an
archive contains `.bntx` or `.bnsh` entries, it aligns the data section and those GPU payloads to
absolute `0x1000` boundaries.

### BNTX texture containers

Inspect a BNTX container or compare two containers by texture name:

```sh
cargo run -- list-bntx textures.bntx
cargo run -- diff-bntx original.bntx modified.bntx
```

For a container with one 9 by 7 BC4 texture, `list-bntx` prints:

```text
textures: 1
[0] example_texture 9x7x1 format=0x00001d01 Bc4/Unorm mips=1 image_size=<bytes> sha256=<digest>
```

If one image changes in a four-texture container, `diff-bntx` prints:

```text
textures: 4 -> 4; added=0 removed=0 changed=1 unchanged=3
~ example_texture index=0->0 image=true metadata=false mips=false user_data=false order=false
```

The diff reports image, metadata, mip layout, user data, and texture order changes separately. Both
commands accept `--format json`. `diff-bntx --check` uses the status described above.

Unpack a BNTX into a self-contained editable bundle:

```sh
cargo run -- unpack-bntx textures.bntx texture-bundle
```

The bundle has this layout:

```text
texture-bundle/
  bntx.json
  textures/
    0000-texture-a.bin
    0000-texture-a.png
    0001-texture-b.bin
    0001-texture-b.png
```

`bntx.json` stores the container and texture settings needed to rebuild the file. Each `.bin` holds
the original image data. Each `.png` is a view that you can open and edit with image software.

#### Why bundles contain both `.bin` and `.png` files

BNTX textures store compressed image data. Converting a texture to PNG and back changes its
compressed bytes and can reduce image quality, even if you did not edit the visible image. Some
BNTX texture types cannot be converted to PNG.

The `.bin` files preserve the original data during unpacking and repacking. They also retain
textures that the PNG converter does not support. PNG files are optional editing copies.

This affects normal usage as follows:

- To inspect or repack a BNTX without changes, use the default unpack and pack commands. Packing
  ignores PNG edits and uses the original `.bin` data.
- To apply PNG edits, pack with `--from-png`. The tool re-encodes only changed previews. Other
  textures keep their original `.bin` data.
- Use `--no-preview` to create a lossless bundle without PNG files. This also works for textures
  that the PNG converter cannot read.
- Previews show channels as the game uses them. Use `--raw-channels` when you need to view the
  stored channels separately. The bundle records this choice for PNG import.

Repack a bundle losslessly, or explicitly encode changed PNG previews:

```sh
cargo run -- pack-bntx texture-bundle rebuilt.bntx
cargo run -- pack-bntx texture-bundle rebuilt.bntx --from-png
```

With `--from-png`, edited previews must keep their original dimensions. The tool rebuilds additional
image levels. It reports missing or invalid PNGs and unsupported texture types. Re-encoding changes
the compressed bytes, so edited textures will differ from their original `.bin` files.

Before a PNG can be added, it needs a BNTX storage profile: how the image is compressed and how its
color channels are used. Use `--like` to copy that profile from an existing texture. PNG additions
and replacements generate one image level. The new PNG supplies the name, dimensions, and visible image:

```sh
cargo run -- add-bntx-texture textures.bntx image.png new_texture updated.bntx \
  --like template_texture
```

Use this when a new texture needs the same storage profile as an existing texture. If none fits,
choose a compression format:

```sh
cargo run -- add-bntx-texture textures.bntx image.png new_texture updated.bntx \
  --format bc7-srgb
```

New additions default to `bc7-unorm` when neither `--like` nor `--format` is supplied.

Duplicate texture names are rejected unless `--replace` is present. Replacement without
`--like` or `--format` preserves the existing texture's storage profile. The input and output
paths may be identical for replacement.

Create a new container from the regular PNG files directly inside one directory:

```sh
cargo run -- create-bntx images textures.bntx \
  --name texture_container --format bc7-unorm
```

Files are processed in deterministic filename order and their exact UTF-8 stems become texture
names. `--name` defaults to the output filename stem, and `--format` defaults to `bc7-unorm`.
The accepted formats are `bc4-unorm`, `bc5-unorm`, `bc7-unorm`, and `bc7-srgb`.

Packing, adding, replacing, and creating finish serialization before replacing the destination.
They replace it atomically, so a failure leaves an existing destination unchanged. Unpacking checks
all paths first and rejects symlinks, traversal, and file conflicts.

PNG editing currently supports the listed formats for ordinary 2D NX textures. Other image formats
and texture types can still be unpacked and repacked losslessly through their `.bin` files.

## Library usage

Use `Bflyt` with `binrw` to read and write binary layouts. Use `serde_jsonc2` to edit their JSON:

```rust
use std::io::Cursor;

use binrw::{BinReaderExt, BinWriterExt};
use smush_ui::bflyt::Bflyt;

let bytes = std::fs::read("input.bflyt")?;
let layout: Bflyt = Cursor::new(bytes).read_le()?;

let json = serde_jsonc2::to_string_pretty(&layout)?;
let edited: Bflyt = serde_jsonc2::from_str(&json)?;

let mut output = Cursor::new(Vec::new());
output.write_le(&edited)?;
std::fs::write("output.bflyt", output.into_inner())?;
```

The writer derives `filesize` and the binary section count. JSON omits both fields.

For BNTX, use `Bntx::from_bytes` and `Bntx::to_bytes` to work with the semantic model. Use
`smush_ui::bntx::fs` for bundle and PNG operations. The writer derives binary offsets, dictionaries,
runtime scratch regions, and relocation records. The public model does not expose them.

## References

The following projects were used as references:

- [3DSkit](https://github.com/Tyulis/3DSkit)
- [bflyt-rs](https://github.com/ultimate-research/bflyt-rs)
- [Colony](https://github.com/KibbeWater/Colony)
- [Nintendo-File-Formats](https://github.com/kinnay/Nintendo-File-Formats)
- [Nintendo-UI-Studio](https://github.com/Arulo165/Nintendo-UI-Studio)
- [nnbfl](https://github.com/crackheadakira/nnbfl)
- [tomo](https://github.com/alexislours/tomo)
- [Toolbox-Cli](https://github.com/crashbandicode/Toolbox-Cli)
