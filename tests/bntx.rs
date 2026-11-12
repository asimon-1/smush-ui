use smush_ui::bntx::{
    Bntx, ChannelFormat, ChannelSelector, FloatValue, Platform, SurfaceDimension, TextureDimension,
    TileMode, TypeFormat, UserDataValue,
};
use std::{collections::BTreeMap, fs, path::Path};

fn fixture_paths() -> Vec<std::path::PathBuf> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_assets/bntx");
    if !directory.exists() {
        return Vec::new();
    }
    let mut paths = fs::read_dir(&directory)
        .expect("tests/test_assets/bntx should exist")
        .map(|entry| entry.expect("fixture entry should be readable").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "bntx")
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

#[test]
fn parses_every_local_bntx_fixture() {
    let paths = fixture_paths();
    if paths.is_empty() {
        return;
    }
    let mut texture_count = 0usize;
    let mut formats = BTreeMap::<u32, usize>::new();
    let mut block_heights = BTreeMap::<u8, usize>::new();

    for path in &paths {
        let bytes = fs::read(path)
            .unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()));
        let bntx = Bntx::from_bytes(&bytes)
            .unwrap_or_else(|error| panic!("{} should parse: {error}", path.display()));

        assert_eq!(
            (bntx.version.major, bntx.version.minor, bntx.version.micro),
            (4, 0, 0)
        );
        assert_eq!(bntx.platform, Platform::Nx);
        assert_eq!(bntx.alignment_exponent, 12);
        assert_eq!(bntx.alignment(), Some(0x1000));
        assert_eq!(bntx.target_address_size, 64);
        assert_eq!(bntx.flag, 0);

        for texture in &bntx.textures {
            assert_eq!(texture.info.image_dimension, TextureDimension::TwoD);
            assert_eq!(texture.info.tile_mode, TileMode::BlockLinear);
            assert_eq!(texture.info.swizzle, 0);
            assert_eq!(texture.info.sample_count, 1);
            assert_eq!(texture.info.depth, 1);
            assert_eq!(texture.info.array_count, 1);
            assert_eq!(texture.info.surface_dimension, SurfaceDimension::TwoD);
            assert_eq!(texture.info.image_alignment, 0x200);
            assert_eq!(texture.mip_offsets, [0]);
            assert!(texture.user_data.is_empty());
            assert!(!texture.image_data.is_empty());
            *formats.entry(texture.info.format.to_raw()).or_default() += 1;
            *block_heights
                .entry(texture.info.texture_layout.block_height_exponent())
                .or_default() += 1;
        }
        texture_count += bntx.textures.len();
    }

    assert_eq!(paths.len(), 1_238);
    assert_eq!(texture_count, 1_280);
    assert_eq!(
        formats,
        BTreeMap::from([(0x1d01, 10), (0x1e01, 1), (0x2001, 1_237), (0x2006, 32)])
    );
    assert_eq!(
        block_heights,
        BTreeMap::from([(0, 40), (1, 32), (2, 621), (3, 3), (4, 584)])
    );
}

#[test]
fn writes_every_local_bntx_fixture_byte_for_byte() {
    for path in fixture_paths() {
        let expected = fs::read(&path)
            .unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()));
        let bntx = Bntx::from_bytes(&expected)
            .unwrap_or_else(|error| panic!("{} should parse: {error}", path.display()));
        let actual = bntx
            .to_bytes()
            .unwrap_or_else(|error| panic!("{} should write: {error}", path.display()));
        if actual != expected {
            let mismatch = actual
                .iter()
                .zip(&expected)
                .position(|(actual, expected)| actual != expected)
                .unwrap_or(actual.len().min(expected.len()));
            panic!(
                "{} differs at {mismatch:#x}: wrote {} bytes, expected {} bytes",
                path.display(),
                actual.len(),
                expected.len()
            );
        }
    }
}

#[test]
fn parses_combined_texture_order_and_typed_formats() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_assets/bntx/__Combined.bntx");
    if !path.exists() {
        return;
    }
    let bytes = fs::read(&path).expect("combined fixture should be readable");
    let bntx = Bntx::from_bytes(&bytes).expect("combined fixture should parse");

    assert_eq!(bntx.name, "__Combined");
    assert_eq!(bntx.textures.len(), 43);
    assert_eq!(bntx.textures[0].name, "com_black16^u");
    assert_eq!(
        bntx.textures[0].info.format.channel_format(),
        ChannelFormat::Bc7U
    );
    assert_eq!(bntx.textures[0].info.format.type_format(), TypeFormat::Srgb);
    assert_eq!(
        bntx.textures[0].info.channels,
        [
            ChannelSelector::Red,
            ChannelSelector::Green,
            ChannelSelector::Blue,
            ChannelSelector::Alpha,
        ]
    );

    let formats = bntx
        .textures
        .iter()
        .map(|texture| texture.info.format.to_raw())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(formats, [0x1d01, 0x1e01, 0x2006].into());
}

#[test]
fn rejects_corrupted_header_and_top_level_ranges() {
    let original = synthetic_bntx(1, false).bytes;

    let cases: &[(&str, usize, &[u8])] = &[
        ("magic", 0, b"NOPE"),
        ("BOM", 0x0c, &[0xfe, 0xff]),
        ("version", 0x0a, &[5, 0]),
        ("alignment", 0x0e, &[11]),
        ("address size", 0x0f, &[32]),
        ("platform", 0x20, b"PC  "),
    ];
    for &(label, offset, replacement) in cases {
        let mut bytes = original.clone();
        bytes[offset..offset + replacement.len()].copy_from_slice(replacement);
        assert!(
            Bntx::from_bytes(&bytes).is_err(),
            "corrupt {label} should fail"
        );
    }

    let mut truncated = original.clone();
    truncated.truncate(31);
    assert!(Bntx::from_bytes(&truncated).is_err());

    let mut wrong_size = original;
    wrong_size[0x1c..0x20].copy_from_slice(&1u32.to_le_bytes());
    assert!(Bntx::from_bytes(&wrong_size).is_err());
}

#[derive(Clone, Copy)]
struct SyntheticOffsets {
    string_block: usize,
    dictionary: usize,
    texture_blocks: [usize; 2],
    brtd: usize,
    image_start: usize,
    relocation: usize,
    container_name: usize,
    wide_name: usize,
    first_user_record: usize,
    user_dictionary: usize,
}

struct SyntheticFixture {
    bytes: Vec<u8>,
    offsets: SyntheticOffsets,
}

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn align(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

fn append_utf8(bytes: &mut Vec<u8>, value: &str) -> u64 {
    let offset = bytes.len();
    bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
    bytes.push(0);
    if !bytes.len().is_multiple_of(2) {
        bytes.push(0);
    }
    offset as u64
}

fn key_bit(key: &[u8], bit: u32) -> u8 {
    let byte_index = (bit / 8) as usize;
    if byte_index >= key.len() {
        0
    } else {
        (key[key.len() - 1 - byte_index] >> (bit % 8)) & 1
    }
}

fn patricia_nodes(keys: &[(&str, u64)], empty_offset: u64) -> Vec<(u32, u16, u16, u64)> {
    let mut nodes = vec![(u32::MAX, 0, 0, empty_offset)];
    if keys.is_empty() {
        return nodes;
    }

    // The first key is a terminal leaf. Each later key adds one decision node
    // at its highest differing bit, with one self edge as the terminal leaf.
    nodes.push((0, 1, 1, keys[0].1));
    nodes[0].1 = 1;
    for new_index in 2..=keys.len() {
        let key = keys[new_index - 1].0.as_bytes();
        let mut previous = -1i64;
        let mut cursor = nodes[0].1 as usize;
        while i64::from(nodes[cursor].0) > previous {
            previous = i64::from(nodes[cursor].0);
            let bit = key_bit(key, nodes[cursor].0);
            cursor = if bit == 0 {
                nodes[cursor].1 as usize
            } else {
                nodes[cursor].2 as usize
            };
        }
        let old_key = keys[cursor - 1].0.as_bytes();
        let highest_bit = (0..(key.len().max(old_key.len()) as u32 * 8))
            .rev()
            .find(|&bit| key_bit(key, bit) != key_bit(old_key, bit))
            .expect("distinct dictionary keys have a differing bit");

        let mut parent = 0usize;
        let mut previous = -1i64;
        let mut child = nodes[parent].1 as usize;
        while i64::from(nodes[child].0) > previous && nodes[child].0 < highest_bit {
            previous = i64::from(nodes[child].0);
            parent = child;
            child = if key_bit(key, nodes[child].0) == 0 {
                nodes[child].1 as usize
            } else {
                nodes[child].2 as usize
            };
        }

        let new_node = new_index;
        let key_bit = key_bit(key, highest_bit);
        nodes.push((
            highest_bit,
            if key_bit == 0 {
                new_node as u16
            } else {
                child as u16
            },
            if key_bit == 1 {
                new_node as u16
            } else {
                child as u16
            },
            keys[new_index - 1].1,
        ));
        if nodes[parent].1 as usize == child {
            nodes[parent].1 = new_node as u16;
        } else {
            nodes[parent].2 = new_node as u16;
        }
    }
    nodes
}

fn write_dictionary(
    bytes: &mut [u8],
    offset: usize,
    keys: &[(&str, u64)],
    empty_offset: u64,
) -> usize {
    bytes[offset..offset + 4].copy_from_slice(b"_DIC");
    put_u32(bytes, offset + 4, keys.len() as u32);
    let nodes = patricia_nodes(keys, empty_offset);
    for (index, (reference, left, right, key_offset)) in nodes.into_iter().enumerate() {
        let position = offset + 8 + index * 16;
        put_u32(bytes, position, reference);
        put_u16(bytes, position + 4, left);
        put_u16(bytes, position + 6, right);
        put_u64(bytes, position + 8, key_offset);
    }
    offset + 8 + (keys.len() + 1) * 16
}

fn synthetic_bntx(texture_count: usize, with_user_data: bool) -> SyntheticFixture {
    assert!((1..=2).contains(&texture_count));
    let all_names = ["a", "b"];
    let names = &all_names[..texture_count];
    let pointer_table = 0x198;
    let mut bytes = vec![0; pointer_table + texture_count * 8];
    let string_block = align(bytes.len(), 4);
    bytes.resize(string_block + 0x18, 0);
    bytes[string_block..string_block + 4].copy_from_slice(b"_STR");

    let empty_offset = (string_block + 0x14) as u64;
    let container_offset = append_utf8(&mut bytes, "synthetic");
    let mut string_offsets = Vec::new();
    for name in names {
        string_offsets.push(append_utf8(&mut bytes, name));
    }
    let user_names = ["bytes", "float", "signed", "text", "wide"];
    let user_name_offsets = if with_user_data {
        user_names
            .iter()
            .map(|name| append_utf8(&mut bytes, name))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let dictionary = align(bytes.len(), 2);
    bytes.resize(dictionary, 0);
    let texture_keys = names
        .iter()
        .zip(&string_offsets)
        .map(|(name, offset)| (*name, *offset))
        .collect::<Vec<_>>();
    let string_block_end = dictionary + 8 + (texture_count + 1) * 16;
    bytes.resize(string_block_end, 0);
    write_dictionary(&mut bytes, dictionary, &texture_keys, empty_offset);
    let string_count = 1 + texture_count + user_name_offsets.len();
    put_u32(
        &mut bytes,
        string_block + 4,
        (string_block_end - string_block) as u32,
    );
    put_u32(
        &mut bytes,
        string_block + 8,
        (string_block_end - string_block) as u32,
    );
    put_u32(&mut bytes, string_block + 16, string_count as u32);

    let mut texture_blocks = [0usize; 2];
    let mut per_texture_sizes = Vec::new();
    for (texture_index, texture_block) in texture_blocks.iter_mut().enumerate().take(texture_count)
    {
        *texture_block = bytes.len();
        let user_count = usize::from(with_user_data && texture_index == 0) * 5;
        let mip_table = 0xa0;
        let user_records = align(mip_table + 16, 8);
        let user_dictionary = user_records + user_count * 0x18;
        let user_values_base = user_dictionary + 8 + (user_count + 1) * 16;
        let mut user_value_offsets = [0usize; 5];
        let mut block_size = if user_count == 0 {
            0xb0
        } else {
            user_values_base
        };
        if user_count != 0 {
            user_value_offsets = [
                block_size,
                block_size + 8,
                block_size + 16,
                block_size + 24,
                block_size + 32,
            ];
            block_size += 48;
        }
        per_texture_sizes.push((
            block_size,
            mip_table,
            user_records,
            user_dictionary,
            user_value_offsets,
        ));
        bytes.resize(bytes.len() + block_size, 0);
    }
    let brtd = align(bytes.len() + 0x10, 0x1000) - 0x10;
    if let Some((last_size, _, _, _, _)) = per_texture_sizes.last_mut() {
        *last_size = brtd - texture_blocks[texture_count - 1];
    }
    bytes.resize(brtd + 0x10, 0);
    let image_start = brtd + 0x10;
    let payload_size = 16;
    let payload_stride = 0x200;
    bytes.resize(
        image_start + (texture_count - 1) * payload_stride + payload_size,
        0,
    );
    for index in 0..texture_count {
        for byte in 0..payload_size {
            bytes[image_start + index * payload_stride + byte] = (index * 16 + byte) as u8;
        }
    }
    let relocation = bytes.len();
    bytes.resize(relocation + 0x10 + 2 * 0x18 + 0x08, 0);

    // Common file and NX headers.
    bytes[0..8].copy_from_slice(b"BNTX\0\0\0\0");
    bytes[8] = 0; // micro
    bytes[9] = 0; // minor
    put_u16(&mut bytes, 10, 4);
    put_u16(&mut bytes, 12, 0xfeff);
    bytes[14] = 12;
    bytes[15] = 64;
    put_u32(&mut bytes, 16, (container_offset + 2) as u32);
    put_u16(&mut bytes, 20, 0);
    put_u16(&mut bytes, 22, string_block as u16);
    put_u32(&mut bytes, 24, relocation as u32);
    let file_size = bytes.len() as u32;
    put_u32(&mut bytes, 28, file_size);
    bytes[0x20..0x24].copy_from_slice(b"NX  ");
    put_u32(&mut bytes, 0x24, texture_count as u32);
    put_u64(&mut bytes, 0x28, pointer_table as u64);
    put_u64(&mut bytes, 0x30, brtd as u64);
    put_u64(&mut bytes, 0x38, dictionary as u64);
    put_u64(&mut bytes, 0x40, 0x58);
    put_u64(&mut bytes, 0x58, brtd as u64);

    // Pointer table and BRTI records.
    for index in 0..texture_count {
        let offset = texture_blocks[index];
        put_u64(&mut bytes, pointer_table + index * 8, offset as u64);
        let (block_size, mip_table, user_records, user_dictionary, user_values) =
            per_texture_sizes[index];
        bytes[offset..offset + 4].copy_from_slice(b"BRTI");
        put_u32(&mut bytes, offset + 4, block_size as u32);
        put_u32(&mut bytes, offset + 8, block_size as u32);
        bytes[offset + 0x10] = 7 + index as u8;
        bytes[offset + 0x11] = 2;
        put_u16(&mut bytes, offset + 0x12, 0);
        put_u16(&mut bytes, offset + 0x14, 0);
        put_u16(&mut bytes, offset + 0x16, 2);
        put_u32(&mut bytes, offset + 0x18, 1);
        put_u32(
            &mut bytes,
            offset + 0x1c,
            if index == 0 { 0x2001 } else { 0x1d01 },
        );
        put_u32(&mut bytes, offset + 0x20, 0x20);
        put_u32(&mut bytes, offset + 0x24, 4 + index as u32);
        put_u32(&mut bytes, offset + 0x28, 8);
        put_u32(&mut bytes, offset + 0x2c, 1);
        put_u32(&mut bytes, offset + 0x30, 1);
        put_u32(&mut bytes, offset + 0x34, 0xa5b6c7d2 + index as u32);
        put_u32(&mut bytes, offset + 0x38, 0x12340000 + index as u32);
        for byte in 0..20 {
            bytes[offset + 0x3c + byte] = (index * 20 + byte) as u8;
        }
        put_u32(&mut bytes, offset + 0x50, payload_size as u32);
        put_u32(&mut bytes, offset + 0x54, 0x200);
        bytes[offset + 0x58..offset + 0x5c].copy_from_slice(&[2, 3, 4, 5]);
        put_u32(&mut bytes, offset + 0x5c, 1);
        put_u64(&mut bytes, offset + 0x60, string_offsets[index]);
        put_u64(&mut bytes, offset + 0x68, 0x20);
        put_u64(&mut bytes, offset + 0x70, (offset + mip_table) as u64);
        put_u64(&mut bytes, offset + 0x80, (offset + mip_table) as u64);
        put_u64(&mut bytes, offset + 0x88, (offset + mip_table + 8) as u64);
        put_u64(
            &mut bytes,
            offset + mip_table,
            image_start as u64 + (index * payload_stride) as u64,
        );
        put_u64(
            &mut bytes,
            offset + mip_table + 8,
            image_start as u64 + (index * payload_stride + 8) as u64,
        );

        let user_count = usize::from(with_user_data && index == 0) * 5;
        if user_count != 0 {
            put_u64(&mut bytes, offset + 0x78, (offset + user_records) as u64);
            put_u64(&mut bytes, offset + 0x98, (offset + user_dictionary) as u64);
            for user_index in 0..5 {
                let record = offset + user_records + user_index * 0x18;
                put_u64(&mut bytes, record, user_name_offsets[user_index]);
                put_u64(
                    &mut bytes,
                    record + 8,
                    (offset + user_values[user_index]) as u64,
                );
                put_u32(
                    &mut bytes,
                    record + 0x10,
                    if user_index == 0 { 2 } else { 1 },
                );
                bytes[record + 0x14] = [3, 1, 0, 2, 4][user_index];
            }
            let user_keys = user_names
                .iter()
                .zip(&user_name_offsets)
                .map(|(name, key)| (*name, *key))
                .collect::<Vec<_>>();
            write_dictionary(
                &mut bytes,
                offset + user_dictionary,
                &user_keys,
                empty_offset,
            );
            put_u32(&mut bytes, offset + user_values[2], (-42i32) as u32);
            put_u32(&mut bytes, offset + user_values[1], 0x7fc01234);
            let utf8_offset = offset + user_values[3];
            put_u16(&mut bytes, utf8_offset, 3);
            bytes[utf8_offset + 2..utf8_offset + 5].copy_from_slice(b"one");
            bytes[utf8_offset + 5] = 0;
            let utf16_offset = offset + user_values[4];
            put_u16(&mut bytes, utf16_offset, 2);
            put_u16(&mut bytes, utf16_offset + 2, b'h' as u16);
            put_u16(&mut bytes, utf16_offset + 4, 0x00e9);
            put_u16(&mut bytes, utf16_offset + 6, 0);
            bytes[offset + user_values[0]..offset + user_values[0] + 2]
                .copy_from_slice(&[0x12, 0xfe]);
        }
    }

    // Associate the memory-pool pointer at 0x58 with the BRTD target section.
    // This exercises the RLT's target-section semantics across file regions.
    bytes[brtd..brtd + 4].copy_from_slice(b"BRTD");
    put_u64(&mut bytes, brtd + 8, (relocation - brtd) as u64);
    bytes[relocation..relocation + 4].copy_from_slice(b"_RLT");
    put_u32(&mut bytes, relocation + 4, relocation as u32);
    put_u32(&mut bytes, relocation + 8, 2);
    put_u64(&mut bytes, relocation + 0x10, 0);
    put_u32(&mut bytes, relocation + 0x18, 0);
    put_u32(&mut bytes, relocation + 0x1c, brtd as u32);
    put_u32(&mut bytes, relocation + 0x20, 0);
    put_u32(&mut bytes, relocation + 0x24, 0);
    put_u64(&mut bytes, relocation + 0x28, 0);
    put_u32(&mut bytes, relocation + 0x30, brtd as u32);
    put_u32(&mut bytes, relocation + 0x34, (relocation - brtd) as u32);
    put_u32(&mut bytes, relocation + 0x38, 0);
    put_u32(&mut bytes, relocation + 0x3c, 1);
    put_u32(&mut bytes, relocation + 0x40, 0x58);
    put_u16(&mut bytes, relocation + 0x44, 1);
    bytes[relocation + 0x46] = 1;

    SyntheticFixture {
        bytes,
        offsets: SyntheticOffsets {
            string_block,
            dictionary,
            texture_blocks,
            brtd,
            image_start,
            relocation,
            container_name: container_offset as usize,
            wide_name: user_name_offsets.get(4).copied().unwrap_or_default() as usize,
            first_user_record: texture_blocks[0] + per_texture_sizes[0].2,
            user_dictionary: texture_blocks[0] + per_texture_sizes[0].3,
        },
    }
}

#[test]
fn parses_compact_single_and_two_texture_fixtures() {
    let single = synthetic_bntx(1, true);
    let parsed = Bntx::from_bytes(&single.bytes).expect("single synthetic fixture should parse");
    assert_eq!(parsed.name, "synthetic");
    assert_eq!(
        parsed.version,
        smush_ui::bntx::Version {
            major: 4,
            minor: 0,
            micro: 0
        }
    );
    assert_eq!(parsed.platform, Platform::Nx);
    assert_eq!(parsed.textures.len(), 1);
    let texture = &parsed.textures[0];
    assert_eq!(texture.name, "a");
    assert_eq!(texture.info.flags, 7);
    assert_eq!(texture.info.image_dimension, TextureDimension::TwoD);
    assert_eq!(texture.info.tile_mode, TileMode::BlockLinear);
    assert_eq!(texture.info.swizzle, 0);
    assert_eq!(texture.info.sample_count, 1);
    assert_eq!(texture.info.format.to_raw(), 0x2001);
    assert_eq!(texture.info.gpu_access, 0x20);
    assert_eq!(
        (texture.info.width, texture.info.height, texture.info.depth),
        (4, 8, 1)
    );
    assert_eq!(texture.info.array_count, 1);
    assert_eq!(texture.info.texture_layout.to_raw(), 0xa5b6c7d2);
    assert_eq!(texture.info.texture_layout2, 0x12340000);
    assert_eq!(
        texture.info.reserved,
        std::array::from_fn(|index| index as u8)
    );
    assert_eq!(texture.info.image_alignment, 0x200);
    assert_eq!(
        texture.info.channels,
        [
            ChannelSelector::Red,
            ChannelSelector::Green,
            ChannelSelector::Blue,
            ChannelSelector::Alpha
        ]
    );
    assert_eq!(texture.info.surface_dimension, SurfaceDimension::TwoD);
    assert_eq!(texture.mip_offsets, [0, 8]);
    assert_eq!(texture.image_data, (0..16).collect::<Vec<_>>());
    assert_eq!(texture.user_data.len(), 5);
    assert_eq!(texture.user_data[0].name, "bytes");
    assert_eq!(
        texture.user_data[0].value,
        UserDataValue::Byte(vec![0x12, 0xfe])
    );
    assert_eq!(
        texture.user_data[1].value,
        UserDataValue::Single(vec![FloatValue::from_bits(0x7fc01234)])
    );
    assert_eq!(texture.user_data[2].value, UserDataValue::Int32(vec![-42]));
    assert_eq!(texture.user_data[2].name, "signed");
    assert_eq!(
        texture.user_data[3].value,
        UserDataValue::String(vec!["one".to_owned()])
    );
    assert_eq!(
        texture.user_data[4].value,
        UserDataValue::WString(vec!["hé".to_owned()])
    );

    let pair = synthetic_bntx(2, false);
    let parsed = Bntx::from_bytes(&pair.bytes).expect("two-texture synthetic fixture should parse");
    assert_eq!(
        parsed
            .textures
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(parsed.textures[0].mip_offsets, [0, 8]);
    assert_eq!(parsed.textures[1].mip_offsets, [0, 8]);
    assert_eq!(parsed.textures[1].info.format.to_raw(), 0x1d01);
    assert_eq!(parsed.textures[1].image_data, (16..32).collect::<Vec<_>>());
}

#[test]
fn json_preserves_raw_and_non_finite_values() {
    let mut fixture = synthetic_bntx(1, true);
    let texture = fixture.offsets.texture_blocks[0];
    put_u32(&mut fixture.bytes, texture + 0x1c, 0xdead_beef);
    fixture.bytes[texture + 0x58..texture + 0x5c].copy_from_slice(&[0xff, 3, 4, 5]);

    let parsed = Bntx::from_bytes(&fixture.bytes).expect("synthetic fixture should parse");
    assert_eq!(parsed.textures[0].info.format.to_raw(), 0xdead_beef);
    assert_eq!(
        parsed.textures[0].info.format.channel_format(),
        ChannelFormat::Unknown(0xadbe)
    );
    assert_eq!(
        parsed.textures[0].info.format.type_format(),
        TypeFormat::Unknown(0xef)
    );
    assert_eq!(
        parsed.textures[0].info.channels[0],
        ChannelSelector::Unknown(0xff)
    );

    let json = serde_json::to_string(&parsed).expect("BNTX model should serialize");
    let reparsed: Bntx = serde_json::from_str(&json).expect("BNTX model should deserialize");
    assert_eq!(reparsed, parsed);
    assert_eq!(
        reparsed.textures[0].user_data[1].value,
        UserDataValue::Single(vec![FloatValue::from_bits(0x7fc0_1234)])
    );
}

#[test]
fn rejects_malformed_synthetic_fixture_ranges_and_records() {
    let fixture = synthetic_bntx(1, true);
    let o = fixture.offsets;
    let cases: &[(&str, usize, &[u8])] = &[
        ("relocated flag", 0x14, &1u16.to_le_bytes()),
        ("first block offset", 0x16, &0u16.to_le_bytes()),
        ("filename pointer underflow", 0x10, &1u32.to_le_bytes()),
        ("texture pointer table", 0x28, &u64::MAX.to_le_bytes()),
        ("texture data block pointer", 0x30, &u64::MAX.to_le_bytes()),
        ("texture dictionary pointer", 0x38, &u64::MAX.to_le_bytes()),
        ("memory pool pointer", 0x40, &0u64.to_le_bytes()),
        ("STR magic", o.string_block, b"NOPE"),
        ("STR next block", o.string_block + 4, &0u32.to_le_bytes()),
        ("STR block size", o.string_block + 8, &1u32.to_le_bytes()),
        (
            "string terminator",
            o.container_name + 2 + "synthetic".len(),
            &[1],
        ),
        ("string alignment", o.wide_name + 7, &[1]),
        ("texture block magic", o.texture_blocks[0], b"NOPE"),
        (
            "texture next block",
            o.texture_blocks[0] + 4,
            &0u32.to_le_bytes(),
        ),
        (
            "texture block size",
            o.texture_blocks[0] + 8,
            &0u32.to_le_bytes(),
        ),
        (
            "texture block padding",
            o.texture_blocks[0] + 0x0c,
            &1u32.to_le_bytes(),
        ),
        (
            "zero texture width",
            o.texture_blocks[0] + 0x24,
            &0u32.to_le_bytes(),
        ),
        ("block height exponent", o.texture_blocks[0] + 0x34, &[0xd6]),
        (
            "texture name pointer",
            o.texture_blocks[0] + 0x60,
            &u64::MAX.to_le_bytes(),
        ),
        (
            "texture runtime pointer",
            o.texture_blocks[0] + 0x80,
            &0u64.to_le_bytes(),
        ),
        (
            "descriptor slot pointer",
            o.texture_blocks[0] + 0x90,
            &u64::MAX.to_le_bytes(),
        ),
        ("BRTD magic", o.brtd, b"NOPE"),
        ("BRTD block size", o.brtd + 8, &u64::MAX.to_le_bytes()),
        ("texture count", 0x24, &u32::MAX.to_le_bytes()),
        (
            "string count",
            o.string_block + 0x10,
            &u32::MAX.to_le_bytes(),
        ),
        (
            "mip pointer order",
            o.texture_blocks[0] + 0xa0,
            &(o.image_start as u64 + 8).to_le_bytes(),
        ),
        (
            "mip pointer range",
            o.texture_blocks[0] + 0xa8,
            &u64::MAX.to_le_bytes(),
        ),
        (
            "image payload alignment",
            o.texture_blocks[0] + 0xa0,
            &(o.image_start as u64 + 1).to_le_bytes(),
        ),
        (
            "user data pointer pairing",
            o.texture_blocks[0] + 0x98,
            &0u64.to_le_bytes(),
        ),
        (
            "user records containment",
            o.texture_blocks[0] + 0x78,
            &(o.texture_blocks[0] as u64).to_le_bytes(),
        ),
        ("user dictionary magic", o.user_dictionary, b"NOPE"),
        ("user data type", o.first_user_record + 0x14, &[9]),
        (
            "user data count",
            o.first_user_record + 0x10,
            &u32::MAX.to_le_bytes(),
        ),
        (
            "user data range",
            o.first_user_record + 8,
            &u64::MAX.to_le_bytes(),
        ),
        (
            "image range overlap",
            o.texture_blocks[0] + 0xa8,
            &(o.image_start as u64).to_le_bytes(),
        ),
    ];
    for &(label, offset, replacement) in cases {
        let mut bytes = fixture.bytes.clone();
        bytes[offset..offset + replacement.len()].copy_from_slice(replacement);
        assert!(
            Bntx::from_bytes(&bytes).is_err(),
            "malformed {label} should fail"
        );
    }

    let mut unaligned_brtd = fixture.bytes.clone();
    let shifted_brtd = o.brtd - 1;
    let shifted_texture_size = shifted_brtd - o.texture_blocks[0];
    unaligned_brtd[shifted_brtd..shifted_brtd + 4].copy_from_slice(b"BRTD");
    put_u32(&mut unaligned_brtd, shifted_brtd + 4, 0);
    put_u64(
        &mut unaligned_brtd,
        shifted_brtd + 8,
        (o.relocation - shifted_brtd) as u64,
    );
    put_u64(&mut unaligned_brtd, 0x30, shifted_brtd as u64);
    put_u32(
        &mut unaligned_brtd,
        o.texture_blocks[0] + 4,
        shifted_texture_size as u32,
    );
    put_u32(
        &mut unaligned_brtd,
        o.texture_blocks[0] + 8,
        shifted_texture_size as u32,
    );
    assert!(
        Bntx::from_bytes(&unaligned_brtd).is_err(),
        "unaligned BRTD should fail"
    );

    let mut bad_string = fixture.bytes.clone();
    let texture_name_pointer = u64::from_le_bytes(
        bad_string[o.texture_blocks[0] + 0x60..o.texture_blocks[0] + 0x68]
            .try_into()
            .unwrap(),
    ) as usize;
    bad_string[texture_name_pointer + 2] = 0xff;
    assert!(
        Bntx::from_bytes(&bad_string).is_err(),
        "invalid UTF-8 should fail"
    );

    let wide_record = o.first_user_record + 4 * 0x18;
    let wide_value = u64::from_le_bytes(
        fixture.bytes[wide_record + 8..wide_record + 16]
            .try_into()
            .unwrap(),
    ) as usize;
    let mut bad_wide_string = fixture.bytes.clone();
    put_u16(&mut bad_wide_string, wide_value + 2, 0xd800);
    assert!(
        Bntx::from_bytes(&bad_wide_string).is_err(),
        "invalid UTF-16 should fail"
    );

    let mut unterminated_wide_string = fixture.bytes.clone();
    put_u16(&mut unterminated_wide_string, wide_value + 6, 1);
    assert!(
        Bntx::from_bytes(&unterminated_wide_string).is_err(),
        "unterminated UTF-16 should fail"
    );

    let mut overlapping = synthetic_bntx(2, false);
    let second_table = overlapping.offsets.texture_blocks[1] + 0xa0;
    put_u64(
        &mut overlapping.bytes,
        second_table,
        overlapping.offsets.image_start as u64,
    );
    assert!(
        Bntx::from_bytes(&overlapping.bytes).is_err(),
        "overlapping texture payloads should fail"
    );

    let mut unordered_textures = synthetic_bntx(2, false);
    put_u64(
        &mut unordered_textures.bytes,
        0x198 + 8,
        unordered_textures.offsets.texture_blocks[0] as u64,
    );
    assert!(
        Bntx::from_bytes(&unordered_textures.bytes).is_err(),
        "unordered texture pointers should fail"
    );

    let mut duplicate_names = synthetic_bntx(2, false);
    let first_name = u64::from_le_bytes(
        duplicate_names.bytes[duplicate_names.offsets.texture_blocks[0] + 0x60
            ..duplicate_names.offsets.texture_blocks[0] + 0x68]
            .try_into()
            .unwrap(),
    );
    put_u64(
        &mut duplicate_names.bytes,
        duplicate_names.offsets.texture_blocks[1] + 0x60,
        first_name,
    );
    assert!(
        Bntx::from_bytes(&duplicate_names.bytes).is_err(),
        "duplicate texture names should fail"
    );
}

#[test]
fn ignores_unmodeled_dictionary_and_relocation_contents() {
    let mut fixture = synthetic_bntx(1, true);
    let expected = Bntx::from_bytes(&fixture.bytes).expect("synthetic fixture should parse");
    let offsets = fixture.offsets;

    fixture.bytes[offsets.dictionary..offsets.dictionary + 4].copy_from_slice(b"NOPE");
    fixture.bytes[offsets.user_dictionary + 12] = 0xff;
    fixture.bytes[offsets.relocation..].fill(0xa5);

    let actual = Bntx::from_bytes(&fixture.bytes)
        .expect("unmodeled metadata should not affect decoded textures");
    assert_eq!(actual, expected);
}

fn assert_writer_fixed_point(label: &str, expected: &Bntx) {
    let first_bytes = expected
        .to_bytes()
        .unwrap_or_else(|error| panic!("{label} should write: {error}"));
    let actual = Bntx::from_bytes(&first_bytes)
        .unwrap_or_else(|error| panic!("written {label} should parse: {error}"));
    assert_eq!(&actual, expected, "{label} semantic round trip");
    assert_eq!(
        actual
            .to_bytes()
            .unwrap_or_else(|error| panic!("reparsed {label} should write: {error}")),
        first_bytes,
        "{label} writer fixed point"
    );
}

#[test]
fn writes_empty_single_multi_unknown_and_user_data_models_deterministically() {
    let single = Bntx::from_bytes(&synthetic_bntx(1, true).bytes)
        .expect("single synthetic fixture should parse");
    let multi = Bntx::from_bytes(&synthetic_bntx(2, false).bytes)
        .expect("multi synthetic fixture should parse");

    let mut empty = single.clone();
    empty.textures.clear();

    let mut unknown_fixture = synthetic_bntx(1, true);
    let texture = unknown_fixture.offsets.texture_blocks[0];
    put_u32(&mut unknown_fixture.bytes, texture + 0x1c, 0xdead_beef);
    unknown_fixture.bytes[texture + 0x58] = 0xff;
    let unknown = Bntx::from_bytes(&unknown_fixture.bytes)
        .expect("unknown-format synthetic fixture should parse");

    assert_writer_fixed_point("empty container", &empty);
    assert_writer_fixed_point("single texture with user data", &single);
    assert_writer_fixed_point("multiple textures", &multi);
    assert_writer_fixed_point("unknown format and channel", &unknown);

    assert_eq!(single.to_bytes().unwrap(), single.to_bytes().unwrap());
    assert_eq!(multi.to_bytes().unwrap(), multi.to_bytes().unwrap());
}

#[test]
fn writes_compact_fixture_with_exact_layout_and_relocations() {
    let model = Bntx::from_bytes(&synthetic_bntx(1, false).bytes)
        .expect("compact synthetic fixture should parse");
    let actual = model
        .to_bytes()
        .expect("compact synthetic model should write");
    let mut expected = vec![0; 0x2088];

    expected[0..8].copy_from_slice(b"BNTX\0\0\0\0");
    put_u16(&mut expected, 0x0a, 4);
    put_u16(&mut expected, 0x0c, 0xfeff);
    expected[0x0e] = 12;
    expected[0x0f] = 64;
    put_u32(&mut expected, 0x10, 0x1ba);
    put_u16(&mut expected, 0x16, 0x1a0);
    put_u32(&mut expected, 0x18, 0x2000);
    put_u32(&mut expected, 0x1c, 0x2088);

    expected[0x20..0x24].copy_from_slice(b"NX  ");
    put_u32(&mut expected, 0x24, 1);
    put_u64(&mut expected, 0x28, 0x198);
    put_u64(&mut expected, 0x30, 0xff0);
    put_u64(&mut expected, 0x38, 0x1c8);
    put_u64(&mut expected, 0x40, 0x58);
    put_u64(&mut expected, 0x198, 0x1f0);

    expected[0x1a0..0x1a4].copy_from_slice(b"_STR");
    put_u32(&mut expected, 0x1a4, 0x50);
    put_u32(&mut expected, 0x1a8, 0x50);
    put_u32(&mut expected, 0x1b0, 2);
    put_u16(&mut expected, 0x1b8, 9);
    expected[0x1ba..0x1c3].copy_from_slice(b"synthetic");
    put_u16(&mut expected, 0x1c4, 1);
    expected[0x1c6] = b'a';

    expected[0x1c8..0x1cc].copy_from_slice(b"_DIC");
    put_u32(&mut expected, 0x1cc, 1);
    put_u32(&mut expected, 0x1d0, u32::MAX);
    put_u16(&mut expected, 0x1d4, 1);
    put_u64(&mut expected, 0x1d8, 0x1b4);
    put_u16(&mut expected, 0x1e6, 1);
    put_u64(&mut expected, 0x1e8, 0x1c4);

    expected[0x1f0..0x1f4].copy_from_slice(b"BRTI");
    put_u32(&mut expected, 0x1f4, 0xe00);
    put_u32(&mut expected, 0x1f8, 0xe00);
    expected[0x200] = 7;
    expected[0x201] = 2;
    put_u16(&mut expected, 0x206, 2);
    put_u32(&mut expected, 0x208, 1);
    put_u32(&mut expected, 0x20c, 0x2001);
    put_u32(&mut expected, 0x210, 0x20);
    put_u32(&mut expected, 0x214, 4);
    put_u32(&mut expected, 0x218, 8);
    put_u32(&mut expected, 0x21c, 1);
    put_u32(&mut expected, 0x220, 1);
    put_u32(&mut expected, 0x224, 0xa5b6_c7d2);
    put_u32(&mut expected, 0x228, 0x1234_0000);
    expected[0x22c..0x240].copy_from_slice(&std::array::from_fn::<_, 20, _>(|i| i as u8));
    put_u32(&mut expected, 0x240, 16);
    put_u32(&mut expected, 0x244, 0x200);
    expected[0x248..0x24c].copy_from_slice(&[2, 3, 4, 5]);
    put_u32(&mut expected, 0x24c, 1);
    put_u64(&mut expected, 0x250, 0x1c4);
    put_u64(&mut expected, 0x258, 0x20);
    put_u64(&mut expected, 0x260, 0x490);
    put_u64(&mut expected, 0x270, 0x290);
    put_u64(&mut expected, 0x278, 0x390);
    put_u64(&mut expected, 0x490, 0x1000);
    put_u64(&mut expected, 0x498, 0x1008);

    expected[0xff0..0xff8].copy_from_slice(b"BRTD\0\0\0\0");
    put_u64(&mut expected, 0xff8, 0x1010);
    expected[0x1000..0x1010].copy_from_slice(&(0..16).collect::<Vec<_>>());

    expected[0x2000..0x2004].copy_from_slice(b"_RLT");
    put_u32(&mut expected, 0x2004, 0x2000);
    put_u32(&mut expected, 0x2008, 2);
    put_u32(&mut expected, 0x201c, 0x4a0);
    put_u32(&mut expected, 0x2024, 7);
    put_u32(&mut expected, 0x2030, 0xff0);
    put_u32(&mut expected, 0x2034, 0x1010);
    put_u32(&mut expected, 0x2038, 7);
    put_u32(&mut expected, 0x203c, 2);
    for (index, (offset, pointers)) in [
        (0x28, 1),
        (0x38, 2),
        (0x198, 1),
        (0x1d8, 1),
        (0x1e8, 1),
        (0x250, 3),
        (0x270, 2),
        (0x30, 1),
        (0x490, 2),
    ]
    .into_iter()
    .enumerate()
    {
        let entry = 0x2040 + index * 8;
        put_u32(&mut expected, entry, offset);
        put_u16(&mut expected, entry + 4, 1);
        expected[entry + 6] = pointers;
    }

    if actual != expected {
        let mismatch = actual
            .iter()
            .zip(&expected)
            .position(|(actual, expected)| actual != expected)
            .unwrap_or(actual.len().min(expected.len()));
        panic!(
            "compact writer mismatch at {mismatch:#x}: actual={:?}, expected={:?}",
            actual.get(mismatch),
            expected.get(mismatch)
        );
    }
}

#[test]
fn rejects_malformed_semantic_models_before_writing() {
    let single = Bntx::from_bytes(&synthetic_bntx(1, true).bytes)
        .expect("single synthetic fixture should parse");
    let multi = Bntx::from_bytes(&synthetic_bntx(2, false).bytes)
        .expect("multi synthetic fixture should parse");
    let mut cases = Vec::new();

    let mut unsupported_version = single.clone();
    unsupported_version.version.major = 5;
    cases.push(("unsupported version", unsupported_version));
    let mut duplicate_names = multi;
    duplicate_names.textures[1].name = duplicate_names.textures[0].name.clone();
    cases.push(("duplicate texture names", duplicate_names));
    let mut zero_width = single.clone();
    zero_width.textures[0].info.width = 0;
    cases.push(("zero width", zero_width));
    let mut empty_mips = single.clone();
    empty_mips.textures[0].mip_offsets.clear();
    cases.push(("empty mip table", empty_mips));
    let mut bad_mip = single.clone();
    bad_mip.textures[0].mip_offsets[0] = 1;
    cases.push(("nonzero first mip", bad_mip));
    let mut empty_image = single.clone();
    empty_image.textures[0].image_data.clear();
    cases.push(("empty image", empty_image));
    let mut bad_alignment = single.clone();
    bad_alignment.textures[0].info.image_alignment = 3;
    cases.push(("non-power-of-two alignment", bad_alignment));
    let mut duplicate_user_data = single.clone();
    duplicate_user_data.textures[0].user_data[1].name =
        duplicate_user_data.textures[0].user_data[0].name.clone();
    cases.push(("duplicate user-data names", duplicate_user_data));
    let mut nul_name = single;
    nul_name.textures[0].name.push('\0');
    cases.push(("NUL in texture name", nul_name));

    for (label, model) in cases {
        assert!(model.to_bytes().is_err(), "{label} should fail to write");
    }
}
