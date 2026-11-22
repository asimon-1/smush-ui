use super::{Bntx, ChannelFormat, ChannelSelector, Texture, TextureInfo, TypeFormat, UserData};
use crate::hash::sha256_hex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A stable, JSON-friendly view of one texture. `index` always refers to its stored BNTX order.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BntxTextureRecord {
    pub name: String,
    pub index: usize,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub raw_format: u32,
    pub channel_format: ChannelFormat,
    pub type_format: TypeFormat,
    pub channels: [ChannelSelector; 4],
    pub mip_count: usize,
    pub mip_offsets: Vec<u64>,
    pub image_size: usize,
    pub image_sha256: String,
    pub metadata: TextureInfo,
    pub user_data: Vec<UserData>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BntxInspection {
    pub texture_count: usize,
    pub textures: Vec<BntxTextureRecord>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BntxDiff {
    pub before_texture_count: usize,
    pub after_texture_count: usize,
    pub added: Vec<BntxTextureRecord>,
    pub removed: Vec<BntxTextureRecord>,
    pub changed: Vec<BntxTextureChange>,
    pub unchanged_count: usize,
}

impl BntxDiff {
    pub fn has_differences(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty() || !self.changed.is_empty()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BntxTextureChange {
    pub name: String,
    pub before_index: usize,
    pub after_index: usize,
    pub raw_image_changed: bool,
    pub metadata_changed: bool,
    pub mip_layout_changed: bool,
    pub user_data_changed: bool,
    pub order_changed: bool,
}

impl BntxTextureChange {
    pub fn has_differences(&self) -> bool {
        self.raw_image_changed
            || self.metadata_changed
            || self.mip_layout_changed
            || self.user_data_changed
            || self.order_changed
    }
}

/// Build stable texture records sorted by name, retaining each texture's original index.
pub fn inspect_bntx(bntx: &Bntx) -> BntxInspection {
    let mut records = bntx
        .textures
        .iter()
        .enumerate()
        .map(|(index, texture)| texture_record(index, texture))
        .collect::<Vec<_>>();
    records.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.index.cmp(&right.index))
    });
    BntxInspection {
        texture_count: records.len(),
        textures: records,
    }
}

/// Compare textures by their unique names. Each change category is independent, so an order-only
/// move is represented with every content category false and `order_changed` true.
pub fn diff_bntx(before: &Bntx, after: &Bntx) -> BntxDiff {
    let before_by_name = before
        .textures
        .iter()
        .enumerate()
        .map(|(index, texture)| (texture.name.as_str(), (index, texture)))
        .collect::<BTreeMap<_, _>>();
    let after_by_name = after
        .textures
        .iter()
        .enumerate()
        .map(|(index, texture)| (texture.name.as_str(), (index, texture)))
        .collect::<BTreeMap<_, _>>();
    let before_common_rank = before
        .textures
        .iter()
        .filter(|texture| after_by_name.contains_key(texture.name.as_str()))
        .map(|texture| texture.name.as_str())
        .enumerate()
        .map(|(rank, name)| (name, rank))
        .collect::<BTreeMap<_, _>>();
    let after_common_rank = after
        .textures
        .iter()
        .filter(|texture| before_by_name.contains_key(texture.name.as_str()))
        .map(|texture| texture.name.as_str())
        .enumerate()
        .map(|(rank, name)| (name, rank))
        .collect::<BTreeMap<_, _>>();

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    let mut unchanged_count = 0;

    for (name, &(index, texture)) in &after_by_name {
        match before_by_name.get(name) {
            None => added.push(texture_record(index, texture)),
            Some(&(before_index, before_texture)) => {
                let record = BntxTextureChange {
                    name: (*name).to_owned(),
                    before_index,
                    after_index: index,
                    raw_image_changed: before_texture.image_data != texture.image_data,
                    metadata_changed: before_texture.info != texture.info,
                    mip_layout_changed: before_texture.mip_offsets != texture.mip_offsets,
                    user_data_changed: before_texture.user_data != texture.user_data,
                    order_changed: before_common_rank.get(name) != after_common_rank.get(name),
                };
                if record.has_differences() {
                    changed.push(record);
                } else {
                    unchanged_count += 1;
                }
            }
        }
    }
    for (name, &(index, texture)) in &before_by_name {
        if !after_by_name.contains_key(name) {
            removed.push(texture_record(index, texture));
        }
    }

    BntxDiff {
        before_texture_count: before.textures.len(),
        after_texture_count: after.textures.len(),
        added,
        removed,
        changed,
        unchanged_count,
    }
}

fn texture_record(index: usize, texture: &Texture) -> BntxTextureRecord {
    let format = texture.info.format;
    BntxTextureRecord {
        name: texture.name.clone(),
        index,
        width: texture.info.width,
        height: texture.info.height,
        depth: texture.info.depth,
        raw_format: format.to_raw(),
        channel_format: format.channel_format(),
        type_format: format.type_format(),
        channels: texture.info.channels,
        mip_count: texture.mip_offsets.len(),
        mip_offsets: texture.mip_offsets.clone(),
        image_size: texture.image_data.len(),
        image_sha256: sha256_hex(&texture.image_data),
        metadata: texture.info.clone(),
        user_data: texture.user_data.clone(),
    }
}
