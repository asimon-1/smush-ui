# smush-ui

`smush-ui` is a Rust library and command-line tool for Nintendo Switch UI files. It reads,
edits, and writes BFLYT layouts as JSON; reads, writes, unpacks, repacks, and edits raw
little-endian SARC archives; and provides lossless and PNG-backed workflows for version-4 NX
BNTX texture containers.

The project makes layout data inspectable and editable without working directly with binary offsets and packed fields. JSON output represents pane and group nesting with `children` arrays, while the binary writer recreates the required boundary sections and derived file metadata.

## Future Goals

Future goals include support for other file formats:

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

Run `smush-ui.exe <COMMAND> --help` for the arguments and options accepted by a particular
command. The examples below use `cargo run --` to run an uninstalled development build.
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

If `a.bin` changes from the byte `01` to `02`, `diff-arc` prints:

```text
entries: 1 -> 1; added=0 removed=0 changed=1
~ a.bin size=1->1 sha256=4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a->dbc1b4c900ffe48d575b5da5c638040125f65db0fe3e24494b76ea986457d986
```

Pass `--format json` for stable, pretty-printed, versioned output suitable for CI. `diff-arc`
accepts independent `--expand-bflyt` and `--expand-bntx` flags for semantic field or texture-level
details when both changed payloads parse as that format. Add `--check` to either diff command to
return status `3` when differences are present; parse and I/O failures continue to return status
`1`.

SARC entries are extracted and packed as raw files. The tool does not decode embedded BFLYT,
BNTX, BFLAN, or other entry payloads during mutating archive operations. Only the opt-in
`diff-arc` expansion flags parse embedded BFLYT or BNTX payloads, and failed nested parsing falls
back to the outer opaque payload difference. Compressed wrappers such as zstd-compressed `.zs`
files and Yaz0 are outside the current archive workflow; decompress them before using these
commands.

The deterministic writer aligns the data section and ordinary entry payloads to absolute `0x10`
boundaries. Archives containing `.bntx` or `.bnsh` entries promote the data section and those GPU resource payloads to absolute `0x1000` boundaries.

### BNTX texture containers

Inspect a BNTX container or compare two containers by texture name:

```sh
cargo run -- list-bntx textures.bntx
cargo run -- diff-bntx original.bntx modified.bntx
```

For a container with one 9 by 7 BC4 texture, `list-bntx` prints output like this. The byte count
and digest depend on the texture data:

```text
textures: 1
[0] example_texture 9x7x1 format=0x00001d01 Bc4/Unorm mips=1 image_size=<bytes> sha256=<digest>
```

If one image changes in a four-texture container, `diff-bntx` prints:

```text
textures: 4 -> 4; added=0 removed=0 changed=1 unchanged=3
~ example_texture index=0->0 image=true metadata=false mips=false user_data=false order=false
```

The diff classifies raw image, metadata, mip layout, user-data, and texture-order changes
independently. Both commands accept `--format json`, and `diff-bntx --check` uses the difference
status described above.

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

`bntx.json` preserves the container and texture settings needed to rebuild the file. Each `.bin`
is an exact copy of one texture's original image data, while each `.png` is a convenient view that
can be opened and edited in ordinary image software.

#### Why bundles contain both `.bin` and `.png` files

BNTX textures use compressed, game-ready image data. Converting that data to PNG and back can
change the compressed bytes and may slightly reduce image quality, even when the visible image was
not intentionally changed. Some BNTX texture types also cannot be converted to PNG yet.

Keeping the original `.bin` files solves both problems: unpacking and repacking is lossless by
default, and unsupported texture types can still be preserved. The PNG files are optional editing
copies rather than replacements for the originals.

This affects normal usage as follows:

- If you only want to inspect, archive, or losslessly repack a BNTX, use the default unpack and
  pack commands. PNG edits are ignored during packing, so simply opening or accidentally changing
  a preview cannot alter the game texture.
- If you intentionally edited one or more PNGs, pack with `--from-png`. `smush-ui` remembers which
  previews changed and re-encodes only those textures. Unchanged textures continue using their
  original `.bin` data, avoiding unnecessary quality loss.
- Use `--no-preview` when you only need a lossless bundle and do not need viewable images. This is
  also useful if a file contains a texture profile that cannot be previewed.
- The default previews show the channels as the game normally uses them. Try `--raw-channels` only
  when a preview looks unusual or you know the texture stores separate kinds of data in its color
  channels. The bundle remembers this choice when importing edited PNGs.

Repack a bundle losslessly, or explicitly encode changed PNG previews:

```sh
cargo run -- pack-bntx texture-bundle rebuilt.bntx
cargo run -- pack-bntx texture-bundle rebuilt.bntx --from-png
```

With `--from-png`, edited previews must keep their original dimensions. The tool rebuilds the
texture's additional image levels automatically. It reports missing or invalid PNGs and unsupported
texture types instead of silently discarding an edit. Re-encoding is lossy, so an edited texture
will not normally have the same compressed bytes as its original `.bin` file.

Before a PNG can be added, it needs a BNTX storage profile: how the image is compressed, how its
color channels are used, and how many image levels it contains. Use `--like` to copy that profile
from an existing texture. The new PNG still supplies the name, dimensions, and visible image:

```sh
cargo run -- add-bntx-texture textures.bntx image.png new_texture updated.bntx \
  --like template_texture
```

This is useful when the new texture should behave like another texture already in the container.
If no existing texture is a suitable model, select a compression format explicitly instead:

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

Packing, adding, replacing, and creating serialize completely before atomically replacing their
destination. A failure leaves an existing destination unchanged. Unpacking preflights all paths
and refuses symlinks, traversal, and existing-file conflicts before creating bundle files.

PNG editing currently supports the listed formats for ordinary 2D NX textures. Other image formats
and texture types can still be unpacked and repacked losslessly through their `.bin` files.

## Library usage

Use `Bflyt` with `binrw` to read or write binary layouts, and `serde_jsonc2` to edit their JSON representation:

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

`filesize` and the binary section count are derived when the layout is written. They are intentionally omitted from normal JSON output.

For BNTX data, use `Bntx::from_bytes` and `Bntx::to_bytes` for the semantic model, or the
functions in `smush_ui::bntx::fs` for the safe bundle and PNG workflows. Binary offsets,
dictionaries, runtime scratch regions, and relocation records are derived by the writer rather
than exposed as editable model fields.

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
