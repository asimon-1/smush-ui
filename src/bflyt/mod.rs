use binrw::{
    BinRead, BinResult, BinWrite, Endian, binrw,
    io::{Cursor, Read, Seek, SeekFrom},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

pub mod sections;
use self::sections::{
    Bounding, Control, FontList, Group, GroupEnd, GroupStart, Layout, Material, Pane, PaneEnd,
    PaneStart, Parts, Picture, Textbox, TextureList, User, Window,
};

#[binrw]
#[br(little, magic = b"FLYT\xff\xfe\x14\x00")]
#[bw(little, magic = b"FLYT\xff\xfe\x14\x00")]
pub struct Bflyt {
    pub version: [u8; 4],
    #[bw(try_map = |_: &u32| serialized_filesize(sections))]
    pub filesize: u32,
    #[bw(map = |_: &u32| sections.len() as u32)]
    pub num_sections: u32,
    #[br(parse_with = |reader, endian, _: ()| read_sections(reader, endian, num_sections))]
    pub sections: Vec<Section>,
}

fn serialized_filesize(sections: &[Section]) -> BinResult<u32> {
    let mut bytes = Cursor::new(Vec::new());
    for section in sections {
        section.write_options(&mut bytes, Endian::Little, ())?;
    }
    Ok((0x14 + bytes.into_inner().len()) as u32)
}

fn read_sections<R: Read + Seek>(
    reader: &mut R,
    endian: Endian,
    count: u32,
) -> BinResult<Vec<Section>> {
    let mut sections = Vec::with_capacity(count as usize);

    for _ in 0..count {
        let start = reader.stream_position()?;
        let section = Section::read_options(reader, endian, ())?;

        reader.seek(SeekFrom::Start(start + 4))?;
        let size = u32::read_options(reader, endian, ())?;
        reader.seek(SeekFrom::Start(start + u64::from(size)))?;
        sections.push(section);
    }

    Ok(sections)
}

#[derive(Serialize, Deserialize)]
#[binrw]
pub enum Section {
    Layout(Layout),
    Control(Control),
    User(User),
    TextureList(TextureList),
    FontList(FontList),
    Material(Material),
    Group(Group),
    GroupStart(GroupStart),
    GroupEnd(GroupEnd),
    Pane(Pane),
    PaneStart(PaneStart),
    PaneEnd(PaneEnd),
    Bounding(Bounding),
    Picture(Picture),
    Parts(Parts),
    Textbox(Textbox),
    Window(Window),
}

macro_rules! section_accessors {
    ($($variant:ident => $method:ident : $ty:ty),* $(,)?) => {
        impl Section {
            $(
                pub fn $method(&self) -> Option<&$ty> {
                    match self {
                        Self::$variant(value) => Some(value),
                        _ => None,
                    }
                }
            )*
        }
    };
}

section_accessors! {
    Layout => as_layout: Layout,
    Control => as_control: Control,
    User => as_user: User,
    TextureList => as_texture_list: TextureList,
    FontList => as_font_list: FontList,
    Material => as_material: Material,
    Group => as_group: Group,
    GroupStart => as_group_start: GroupStart,
    GroupEnd => as_group_end: GroupEnd,
    Pane => as_pane: Pane,
    PaneStart => as_pane_start: PaneStart,
    PaneEnd => as_pane_end: PaneEnd,
    Bounding => as_bounding: Bounding,
    Picture => as_picture: Picture,
    Parts => as_parts: Parts,
    Textbox => as_textbox: Textbox,
    Window => as_window: Window,
}

impl Section {
    /// Returns whether this section carries the common pane payload and may own pane children.
    pub fn is_pane_section(&self) -> bool {
        matches!(
            self,
            Self::Pane(_)
                | Self::Bounding(_)
                | Self::Picture(_)
                | Self::Parts(_)
                | Self::Textbox(_)
                | Self::Window(_)
        )
    }
}

#[derive(Serialize)]
struct PaneSectionRef<'a, T: Serialize> {
    #[serde(flatten)]
    section: &'a T,
    #[serde(skip_serializing_if = "Option::is_none")]
    children: Option<Vec<JsonSectionRef<'a>>>,
}

#[derive(Serialize)]
struct GroupSectionRef<'a> {
    #[serde(flatten)]
    section: &'a Group,
    #[serde(skip_serializing_if = "Option::is_none")]
    children: Option<Vec<JsonSectionRef<'a>>>,
}

#[derive(Serialize)]
enum JsonSectionRef<'a> {
    Layout(&'a Layout),
    Control(&'a Control),
    User(&'a User),
    TextureList(&'a TextureList),
    FontList(&'a FontList),
    Material(&'a Material),
    Group(GroupSectionRef<'a>),
    Pane(PaneSectionRef<'a, Pane>),
    Bounding(PaneSectionRef<'a, Bounding>),
    Picture(PaneSectionRef<'a, Picture>),
    Parts(PaneSectionRef<'a, Parts>),
    Textbox(PaneSectionRef<'a, Textbox>),
    Window(PaneSectionRef<'a, Window>),
}

#[derive(Deserialize)]
struct PaneSection<T> {
    #[serde(flatten)]
    section: T,
    #[serde(default)]
    children: Option<Vec<JsonSection>>,
}

#[derive(Deserialize)]
struct GroupSection {
    #[serde(flatten)]
    section: Group,
    #[serde(default)]
    children: Option<Vec<JsonSection>>,
}

#[derive(Deserialize)]
enum JsonSection {
    Layout(Layout),
    Control(Control),
    User(User),
    TextureList(TextureList),
    FontList(FontList),
    Material(Material),
    Group(GroupSection),
    Pane(PaneSection<Pane>),
    Bounding(PaneSection<Bounding>),
    Picture(PaneSection<Picture>),
    Parts(PaneSection<Parts>),
    Textbox(PaneSection<Textbox>),
    Window(PaneSection<Window>),
}

fn json_sections<'a, E: serde::ser::Error>(
    sections: &'a [Section],
) -> Result<Vec<JsonSectionRef<'a>>, E> {
    #[derive(Clone, Copy, PartialEq)]
    enum Boundary {
        Pane,
        Group,
    }

    fn read_level<'a, E: serde::ser::Error>(
        sections: &'a [Section],
        mut index: usize,
        expected_boundary: Option<Boundary>,
    ) -> Result<(Vec<JsonSectionRef<'a>>, usize), E> {
        let mut result = Vec::new();

        while index < sections.len() {
            let boundary = match &sections[index] {
                Section::PaneEnd(_) => Some(Boundary::Pane),
                Section::GroupEnd(_) => Some(Boundary::Group),
                Section::PaneStart(_) | Section::GroupStart(_) => {
                    return Err(E::custom(
                        "hierarchy start marker does not follow its parent section",
                    ));
                }
                _ => None,
            };
            if let Some(boundary) = boundary {
                return if expected_boundary == Some(boundary) {
                    Ok((result, index + 1))
                } else {
                    Err(E::custom(
                        "hierarchy end marker has no matching start marker",
                    ))
                };
            }

            let boundary = match sections.get(index + 1) {
                Some(Section::PaneStart(_)) => Some(Boundary::Pane),
                Some(Section::GroupStart(_)) => Some(Boundary::Group),
                _ => None,
            };
            let (children, next_index) = if let Some(boundary) = boundary {
                let (children, next_index) = read_level::<E>(sections, index + 2, Some(boundary))?;
                (Some(children), next_index)
            } else {
                (None, index + 1)
            };

            let section = match &sections[index] {
                Section::Layout(value) => JsonSectionRef::Layout(value),
                Section::Control(value) => JsonSectionRef::Control(value),
                Section::User(value) => JsonSectionRef::User(value),
                Section::TextureList(value) => JsonSectionRef::TextureList(value),
                Section::FontList(value) => JsonSectionRef::FontList(value),
                Section::Material(value) => JsonSectionRef::Material(value),
                Section::Group(value) => JsonSectionRef::Group(GroupSectionRef {
                    section: value,
                    children,
                }),
                Section::Pane(value) => JsonSectionRef::Pane(PaneSectionRef {
                    section: value,
                    children,
                }),
                Section::Bounding(value) => JsonSectionRef::Bounding(PaneSectionRef {
                    section: value,
                    children,
                }),
                Section::Picture(value) => JsonSectionRef::Picture(PaneSectionRef {
                    section: value,
                    children,
                }),
                Section::Parts(value) => JsonSectionRef::Parts(PaneSectionRef {
                    section: value,
                    children,
                }),
                Section::Textbox(value) => JsonSectionRef::Textbox(PaneSectionRef {
                    section: value,
                    children,
                }),
                Section::Window(value) => JsonSectionRef::Window(PaneSectionRef {
                    section: value,
                    children,
                }),
                Section::PaneStart(_)
                | Section::PaneEnd(_)
                | Section::GroupStart(_)
                | Section::GroupEnd(_) => unreachable!(),
            };

            match boundary {
                Some(Boundary::Pane) if !sections[index].is_pane_section() => {
                    return Err(E::custom("pane start marker follows a non-pane section"));
                }
                Some(Boundary::Group) if !matches!(sections[index], Section::Group(_)) => {
                    return Err(E::custom("group start marker follows a non-group section"));
                }
                _ => {}
            }

            result.push(section);
            index = next_index;
        }

        if expected_boundary.is_some() {
            Err(E::custom(
                "hierarchy start marker has no matching end marker",
            ))
        } else {
            Ok((result, index))
        }
    }

    read_level::<E>(sections, 0, None).map(|(sections, _)| sections)
}

fn binary_sections(sections: Vec<JsonSection>) -> Vec<Section> {
    #[derive(Clone, Copy)]
    enum Boundary {
        Pane,
        Group,
    }

    fn append(output: &mut Vec<Section>, section: JsonSection) {
        let (section, children, boundary) = match section {
            JsonSection::Layout(value) => (Section::Layout(value), None, Boundary::Pane),
            JsonSection::Control(value) => (Section::Control(value), None, Boundary::Pane),
            JsonSection::User(value) => (Section::User(value), None, Boundary::Pane),
            JsonSection::TextureList(value) => (Section::TextureList(value), None, Boundary::Pane),
            JsonSection::FontList(value) => (Section::FontList(value), None, Boundary::Pane),
            JsonSection::Material(value) => (Section::Material(value), None, Boundary::Pane),
            JsonSection::Group(value) => (
                Section::Group(value.section),
                value.children,
                Boundary::Group,
            ),
            JsonSection::Pane(value) => {
                (Section::Pane(value.section), value.children, Boundary::Pane)
            }
            JsonSection::Bounding(value) => (
                Section::Bounding(value.section),
                value.children,
                Boundary::Pane,
            ),
            JsonSection::Picture(value) => (
                Section::Picture(value.section),
                value.children,
                Boundary::Pane,
            ),
            JsonSection::Parts(value) => (
                Section::Parts(value.section),
                value.children,
                Boundary::Pane,
            ),
            JsonSection::Textbox(value) => (
                Section::Textbox(value.section),
                value.children,
                Boundary::Pane,
            ),
            JsonSection::Window(value) => (
                Section::Window(value.section),
                value.children,
                Boundary::Pane,
            ),
        };

        output.push(section);
        if let Some(children) = children {
            output.push(match boundary {
                Boundary::Pane => Section::PaneStart(PaneStart::new()),
                Boundary::Group => Section::GroupStart(GroupStart::new()),
            });
            for child in children {
                append(output, child);
            }
            output.push(match boundary {
                Boundary::Pane => Section::PaneEnd(PaneEnd::new()),
                Boundary::Group => Section::GroupEnd(GroupEnd::new()),
            });
        }
    }

    let mut output = Vec::new();
    for section in sections {
        append(&mut output, section);
    }
    output
}

impl Serialize for Bflyt {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let sections = json_sections::<S::Error>(&self.sections)?;
        let mut state = serializer.serialize_struct("Bflyt", 2)?;
        state.serialize_field("version", &self.version)?;
        state.serialize_field("sections", &sections)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Bflyt {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct JsonBflyt {
            version: [u8; 4],
            #[serde(default, rename = "filesize")]
            _filesize: Option<u32>,
            #[serde(default, rename = "num_sections")]
            _num_sections: Option<u32>,
            sections: Vec<JsonSection>,
        }

        let value = JsonBflyt::deserialize(deserializer)?;
        let sections = binary_sections(value.sections);
        Ok(Self {
            version: value.version,
            filesize: 0,
            num_sections: sections.len() as u32,
            sections,
        })
    }
}
