use binrw::binrw;

pub mod sections;
use self::sections::{
    Bounding, Control, FontList, Group, GroupEnd, GroupStart, Layout, Material, Pane, PaneEnd,
    PaneStart, Parts, Picture, Textbox, TextureList, User, Window,
};

#[binrw]
#[br(magic = b"FLYT\xfe\xff\x14")]
#[bw(magic = b"FLYT\xfe\xff\x14")]
pub struct Bflyt {
    pub version: [u8; 4],
    pub filesize: u32,
    pub num_sections: u16,
    padding: [u8; 2],
    #[br(count = num_sections)]
    pub sections: Vec<Section>,
}

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
