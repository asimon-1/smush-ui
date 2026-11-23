use std::io::Cursor;

use binrw::{BinReaderExt, BinWriterExt};
use smush_ui::bflyt::{
    Bflyt, Section,
    sections::{
        Group, GroupEnd, GroupStart, Pane, PaneEnd, PaneFlags, PaneInfo, PaneOrigin, PaneStart,
        PartScalingFlags, Vec2, Vec3,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Token {
    Pane,
    Group,
    PaneStart,
    GroupStart,
    PaneEnd,
    GroupEnd,
}

const ALPHABET: [Token; 6] = [
    Token::Pane,
    Token::Group,
    Token::PaneStart,
    Token::GroupStart,
    Token::PaneEnd,
    Token::GroupEnd,
];
const MAX_SECTIONS: usize = 5;

fn pane() -> PaneInfo {
    PaneInfo {
        flags: PaneFlags::default(),
        origin: PaneOrigin::default(),
        alpha: 255,
        part_scaling: PartScalingFlags::default(),
        name: "pane".into(),
        user_name: String::new(),
        position: Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        rotation: Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        scale: Vec2 { x: 1.0, y: 1.0 },
        size: Vec2 { x: 1.0, y: 1.0 },
    }
}

fn section(token: Token) -> Section {
    match token {
        Token::Pane => Section::Pane(Pane::new(pane())),
        Token::Group => Section::Group(Group::new("group".into(), vec![])),
        Token::PaneStart => Section::PaneStart(PaneStart::new()),
        Token::GroupStart => Section::GroupStart(GroupStart::new()),
        Token::PaneEnd => Section::PaneEnd(PaneEnd::new()),
        Token::GroupEnd => Section::GroupEnd(GroupEnd::new()),
    }
}

fn is_valid(tokens: &[Token]) -> bool {
    let mut open = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        match token {
            Token::PaneStart => {
                if index == 0 || tokens[index - 1] != Token::Pane {
                    return false;
                }
                open.push(Token::PaneStart);
            }
            Token::GroupStart => {
                if index == 0 || tokens[index - 1] != Token::Group {
                    return false;
                }
                open.push(Token::GroupStart);
            }
            Token::PaneEnd => {
                if open.pop() != Some(Token::PaneStart) {
                    return false;
                }
            }
            Token::GroupEnd => {
                if open.pop() != Some(Token::GroupStart) {
                    return false;
                }
            }
            Token::Pane | Token::Group => {}
        }
    }
    open.is_empty()
}

fn check(tokens: &[Token]) {
    let document = Bflyt {
        version: [2, 2, 0, 0],
        filesize: 0,
        num_sections: 0,
        sections: tokens.iter().copied().map(section).collect(),
    };
    let json = serde_jsonc2::to_vec(&document);
    if !is_valid(tokens) {
        assert!(json.is_err(), "invalid hierarchy accepted: {tokens:?}");
        return;
    }

    let json = json.unwrap_or_else(|error| panic!("valid hierarchy rejected: {tokens:?}: {error}"));
    let rebuilt: Bflyt = serde_jsonc2::from_slice(&json)
        .unwrap_or_else(|error| panic!("JSON did not parse: {tokens:?}: {error}"));

    let mut original_bytes = Cursor::new(Vec::new());
    original_bytes.write_le(&document).unwrap();
    let mut rebuilt_bytes = Cursor::new(Vec::new());
    rebuilt_bytes.write_le(&rebuilt).unwrap();
    assert_eq!(
        rebuilt_bytes.get_ref(),
        original_bytes.get_ref(),
        "changed bytes: {tokens:?}"
    );

    let parsed: Bflyt = Cursor::new(rebuilt_bytes.into_inner()).read_le().unwrap();
    assert_eq!(
        serde_jsonc2::to_vec(&parsed).unwrap(),
        json,
        "changed JSON: {tokens:?}"
    );
}

#[test]
fn every_hierarchy_sequence_through_five_sections() {
    fn visit(tokens: &mut Vec<Token>) {
        check(tokens);
        if tokens.len() == MAX_SECTIONS {
            return;
        }
        for token in ALPHABET {
            tokens.push(token);
            visit(tokens);
            tokens.pop();
        }
    }

    visit(&mut Vec::new());
}
