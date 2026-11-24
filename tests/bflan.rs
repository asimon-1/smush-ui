use std::{fs, io::Cursor, path::Path};

use binrw::{BinReaderExt, BinWriterExt};

use smush_ui::bflan::{
    AnimationEntry, AnimationTag, Bflan, Curve, Pai1, PaiFlags, Section, StepKey, Track,
};

#[test]
fn reads_and_writes_a_minimal_bflan() {
    let bytes = b"FLAN\xff\xfe\x14\x00\0\0\0\x09\x14\0\0\0\0\0\0\0";
    let parsed = Bflan::from_bytes(bytes).expect("minimal BFLAN parses");
    assert_eq!(parsed.version, 0x0900_0000);
    assert!(parsed.sections.is_empty());
    assert_eq!(parsed.to_bytes().unwrap(), bytes);
}

#[test]
fn rejects_invalid_file_and_section_sizes() {
    let mut bytes = Bflan::new(vec![Section::Unknown {
        magic: "test".into(),
        data: vec![1, 2, 3],
    }])
    .to_bytes()
    .unwrap();
    bytes[12] = 0;
    assert!(Bflan::from_bytes(&bytes).is_err());
    bytes[12] = 31;
    bytes[24..28].copy_from_slice(&7u32.to_le_bytes());
    assert!(Bflan::from_bytes(&bytes).is_err());
}

#[test]
fn parses_and_writes_optional_pah1() {
    use smush_ui::bflan::{Pah1, SharedAnimation};
    let animation = Bflan::new(vec![Section::Pah1(Pah1 {
        shares: vec![SharedAnimation {
            source_pane: "source".into(),
            target_group: "group".into(),
        }],
    })]);
    let bytes = animation.to_bytes().unwrap();
    let parsed = Bflan::from_bytes(&bytes).unwrap();
    assert_eq!(parsed, animation);
    assert_eq!(parsed.to_bytes().unwrap(), bytes);
}

#[test]
fn public_sections_use_binrw_from_an_aligned_stream_position() {
    use smush_ui::bflan::{Pah1, Pat1, SharedAnimation};

    let pat = Pat1 {
        order: 1,
        user_data: None,
        start_frame: 0,
        end_frame: 4,
        child_binding: false,
        name: "Open".into(),
        groups: vec!["main".into()],
    };
    let pah = Pah1 {
        shares: vec![SharedAnimation {
            source_pane: "pane".into(),
            target_group: "group".into(),
        }],
    };
    let pai = Pai1 {
        frame_count: 4,
        flags: PaiFlags::default(),
        textures: vec!["texture".into(), "other".into()],
        entries: vec![AnimationEntry {
            name: "pane".into(),
            target: 0,
            tags: Vec::new(),
        }],
    };

    let mut stream = Cursor::new(vec![0; 8]);
    stream.set_position(8);
    stream.write_le(&pat).unwrap();
    stream.write_le(&pah).unwrap();
    stream.write_le(&pai).unwrap();
    stream.set_position(8);
    assert_eq!(stream.read_le::<Pat1>().unwrap(), pat);
    assert_eq!(stream.read_le::<Pah1>().unwrap(), pah);
    assert_eq!(stream.read_le::<Pai1>().unwrap(), pai);
}

#[test]
fn rejects_texture_offset_before_the_name_table() {
    let animation = Bflan::new(vec![Section::Pai1(Pai1 {
        frame_count: 1,
        flags: PaiFlags::default(),
        textures: vec!["texture".into()],
        entries: Vec::new(),
    })]);
    let mut bytes = animation.to_bytes().unwrap();
    bytes[40..44].copy_from_slice(&0u32.to_le_bytes());
    assert!(Bflan::from_bytes(&bytes).is_err());
}

#[test]
fn rejects_malformed_nested_offsets_and_curve_types() {
    let animation = Bflan::new(vec![Section::Pai1(Pai1 {
        frame_count: 4,
        flags: PaiFlags::default(),
        textures: vec![],
        entries: vec![AnimationEntry {
            name: "pane".into(),
            target: 0,
            tags: vec![AnimationTag {
                kind: "FLVI".into(),
                tracks: vec![Track {
                    index: 0,
                    target: 0,
                    curve: Curve::Step(vec![StepKey {
                        frame: 0.0,
                        value: 1,
                        reserved: 0,
                    }]),
                    name: None,
                }],
            }],
        }],
    })]);
    let bytes = animation.to_bytes().unwrap();
    let pai = 20;
    let entry = pai + u32::from_le_bytes(bytes[pai + 20..pai + 24].try_into().unwrap()) as usize;
    let tag =
        entry + u32::from_le_bytes(bytes[entry + 32..entry + 36].try_into().unwrap()) as usize;
    let track = tag + u32::from_le_bytes(bytes[tag + 8..tag + 12].try_into().unwrap()) as usize;

    let mut bad_curve = bytes.clone();
    bad_curve[track + 2] = 9;
    assert!(Bflan::from_bytes(&bad_curve).is_err());

    let mut bad_keys = bytes;
    bad_keys[track + 8..track + 12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(Bflan::from_bytes(&bad_keys).is_err());
}

#[test]
fn edits_step_hermite_and_target_two_names_through_json() {
    use smush_ui::bflan::{HermiteKey, Pat1};
    use smush_ui::bflyt::sections::user::{UserData, UserDataEntry, UserDataValue};

    let animation = Bflan::new(vec![
        Section::Pat1(Pat1 {
            order: 1,
            user_data: Some(UserData {
                entries: vec![UserDataEntry {
                    name: "anim_linkage".into(),
                    value: UserDataValue::String("button;".into()),
                }],
            }),
            start_frame: 0,
            end_frame: 4,
            child_binding: false,
            name: "Open".into(),
            groups: vec!["main".into()],
        }),
        Section::Pai1(Pai1 {
            frame_count: 4,
            flags: PaiFlags::default(),
            textures: vec![],
            entries: vec![
                AnimationEntry {
                    name: "pane".into(),
                    target: 0,
                    tags: vec![AnimationTag {
                        kind: "FLPA".into(),
                        tracks: vec![Track {
                            index: 0,
                            target: 0,
                            curve: Curve::Hermite(vec![HermiteKey {
                                frame: 0.0,
                                value: 5.0,
                                slope: 1.0,
                            }]),
                            name: None,
                        }],
                    }],
                },
                AnimationEntry {
                    name: "user".into(),
                    target: 2,
                    tags: vec![AnimationTag {
                        kind: "FLEU".into(),
                        tracks: vec![Track {
                            index: 0,
                            target: 0,
                            curve: Curve::Step(vec![StepKey {
                                frame: 0.0,
                                value: 3,
                                reserved: 0,
                            }]),
                            name: Some("RubyColorIndex".into()),
                        }],
                    }],
                },
            ],
        }),
    ]);
    let bytes = animation.to_bytes().unwrap();
    assert_eq!(Bflan::from_bytes(&bytes).unwrap(), animation);
    let mut json = serde_jsonc2::to_value(&animation).unwrap();
    json["sections"][0]["Pat1"]["user_data"]["entries"][0]["value"]["String"] = "edited;".into();
    json["sections"][1]["Pai1"]["entries"][0]["tags"][0]["tracks"][0]["curve"]["Hermite"][0]["value"] =
        7.0.into();
    json["sections"][1]["Pai1"]["entries"][1]["tags"][0]["tracks"][0]["curve"]["Step"][0]["value"] =
        9.into();
    json["sections"][1]["Pai1"]["entries"][1]["tags"][0]["tracks"][0]["name"] =
        "ChangedIndex".into();
    let edited: Bflan = serde_jsonc2::from_value(json).unwrap();
    let reparsed = Bflan::from_bytes(&edited.to_bytes().unwrap()).unwrap();
    assert_eq!(reparsed, edited);
}

#[test]
fn rejects_malformed_embedded_user_data() {
    use smush_ui::bflan::Pat1;
    use smush_ui::bflyt::sections::user::{UserData, UserDataEntry, UserDataValue};

    let bytes = Bflan::new(vec![Section::Pat1(Pat1 {
        order: 1,
        user_data: Some(UserData {
            entries: vec![UserDataEntry {
                name: "anim_linkage".into(),
                value: UserDataValue::String("button;".into()),
            }],
        }),
        start_frame: 0,
        end_frame: 4,
        child_binding: false,
        name: "Open".into(),
        groups: vec![],
    })])
    .to_bytes()
    .unwrap();
    let mut bad_offset = bytes.clone();
    bad_offset[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(Bflan::from_bytes(&bad_offset).is_err());

    let user_start = 20 + u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize;
    let mut bad_size = bytes.clone();
    bad_size[user_start + 4..user_start + 8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(Bflan::from_bytes(&bad_size).is_err());

    let mut bad_value = bytes;
    bad_value[user_start + 16..user_start + 20].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(Bflan::from_bytes(&bad_value).is_err());
}

#[test]
fn json_preserves_non_finite_keyframe_bits() {
    use smush_ui::bflan::HermiteKey;
    let animation = Bflan::new(vec![Section::Pai1(Pai1 {
        frame_count: 1,
        flags: PaiFlags::default(),
        textures: vec![],
        entries: vec![AnimationEntry {
            name: "pane".into(),
            target: 0,
            tags: vec![AnimationTag {
                kind: "FLPA".into(),
                tracks: vec![Track {
                    index: 0,
                    target: 0,
                    curve: Curve::Hermite(vec![HermiteKey {
                        frame: 0.0,
                        value: f32::from_bits(0x7fc0_1234),
                        slope: f32::INFINITY,
                    }]),
                    name: None,
                }],
            }],
        }],
    })]);
    let bytes = animation.to_bytes().unwrap();
    let json = serde_jsonc2::to_string(&animation).unwrap();
    assert!(json.contains("2143294004"));
    let restored: Bflan = serde_jsonc2::from_str(&json).unwrap();
    assert_eq!(restored.to_bytes().unwrap(), bytes);
}

#[test]
fn ignored_local_corpus_round_trips_when_available() {
    let directory = Path::new("temp/layout_original_unpacked/anim");
    if !directory.exists() {
        return;
    }
    let files: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(Result::unwrap)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "bflan"))
        .collect();
    assert!(!files.is_empty());
    for path in files {
        let bytes = fs::read(&path).unwrap();
        let parsed =
            Bflan::from_bytes(&bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let rebuilt = parsed
            .to_bytes()
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        if bytes != rebuilt {
            let first = bytes.iter().zip(&rebuilt).position(|(a, b)| a != b);
            let changes: Vec<_> = bytes
                .iter()
                .zip(&rebuilt)
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .take(20)
                .map(|(i, (a, b))| (i, a, b))
                .collect();
            panic!(
                "{}: first difference {first:?}; source {}, rebuilt {}; changes {changes:?}",
                path.display(),
                bytes.len(),
                rebuilt.len()
            );
        }
        let json = serde_jsonc2::to_string(&parsed).unwrap();
        let from_json: Bflan = serde_jsonc2::from_str(&json).unwrap();
        let rebuilt_json = from_json.to_bytes().unwrap();
        assert_eq!(
            Bflan::from_bytes(&rebuilt_json).unwrap(),
            parsed,
            "{}",
            path.display()
        );
    }
}
