#[path = "common/bntx.rs"]
mod common_bntx;

use common_bntx::synthetic_bntx;
use smush_ui::{
    bntx::{UserData, UserDataValue, diff_bntx, inspect_bntx},
    hash::sha256_hex,
};

#[test]
fn inspection_is_name_sorted_and_keeps_storage_indices_and_semantics() {
    let bntx = synthetic_bntx();
    let inspection = inspect_bntx(&bntx);
    let records = inspection.textures;

    assert_eq!(inspection.texture_count, bntx.textures.len());
    assert!(records.windows(2).all(|pair| pair[0].name <= pair[1].name));
    for record in &records {
        let texture = &bntx.textures[record.index];
        assert_eq!(record.name, texture.name);
        assert_eq!(record.raw_format, texture.info.format.to_raw());
        assert_eq!(record.mip_count, texture.mip_offsets.len());
        assert_eq!(record.mip_offsets, texture.mip_offsets);
        assert_eq!(record.image_size, texture.image_data.len());
        assert_eq!(record.image_sha256, sha256_hex(&texture.image_data));
        assert_eq!(record.metadata, texture.info);
        assert_eq!(record.user_data, texture.user_data);
    }
    assert_eq!(records[0].name, "color:*?");
    assert_eq!(records[0].index, 2);
}

#[test]
fn diff_classifies_add_remove_and_independent_common_texture_changes() {
    let mut before = synthetic_bntx();
    let mut removed_texture = before.textures[0].clone();
    removed_texture.name = "removed texture".into();
    before.textures.push(removed_texture);
    let mut after = before.clone();

    let image = &mut after.textures[0].image_data;
    image[0] ^= 0xff;

    let metadata_only = &mut after.textures[1];
    metadata_only.info.width += 1;

    let mip_only = &mut after.textures[2];
    mip_only.mip_offsets = vec![0, 1];

    let user_only = &mut after.textures[3];
    user_only.user_data.push(UserData {
        name: "revision".into(),
        value: UserDataValue::Int32(vec![1]),
    });

    let removed_name = after.textures.pop().unwrap().name;
    let added_texture = before.textures[0].clone();
    let added_name = "added texture";
    let mut added_texture = added_texture;
    added_texture.name = added_name.into();
    after.textures.push(added_texture);

    let diff = diff_bntx(&before, &after);
    assert!(diff.has_differences());
    assert_eq!(diff.unchanged_count, 0);
    assert_eq!(
        diff.added
            .iter()
            .map(|record| record.name.as_str())
            .collect::<Vec<_>>(),
        [added_name]
    );
    assert_eq!(
        diff.removed
            .iter()
            .map(|record| record.name.as_str())
            .collect::<Vec<_>>(),
        [removed_name.as_str()]
    );
    assert!(
        diff.changed
            .windows(2)
            .all(|pair| pair[0].name <= pair[1].name)
    );

    let by_name = diff
        .changed
        .iter()
        .map(|change| (change.name.as_str(), change))
        .collect::<std::collections::BTreeMap<_, _>>();
    let image = by_name[before.textures[0].name.as_str()];
    assert!(image.raw_image_changed);
    assert!(!image.metadata_changed && !image.mip_layout_changed && !image.user_data_changed);
    let metadata = by_name[before.textures[1].name.as_str()];
    assert!(metadata.metadata_changed);
    assert!(
        !metadata.raw_image_changed && !metadata.mip_layout_changed && !metadata.user_data_changed
    );
    let mip = by_name[before.textures[2].name.as_str()];
    assert!(mip.mip_layout_changed);
    assert!(!mip.raw_image_changed && !mip.metadata_changed && !mip.user_data_changed);
    let user = by_name[before.textures[3].name.as_str()];
    assert!(user.user_data_changed);
    assert!(!user.raw_image_changed && !user.metadata_changed && !user.mip_layout_changed);
}

#[test]
fn diff_detects_relative_reordering_but_not_index_shifts_from_additions() {
    let before = synthetic_bntx();
    let mut with_addition = before.clone();
    let mut added = before.textures[0].clone();
    added.name = "0-added".into();
    with_addition.textures.insert(0, added);
    let shifted = diff_bntx(&before, &with_addition);
    assert!(shifted.changed.is_empty());
    assert_eq!(shifted.unchanged_count, before.textures.len());

    let mut reordered = before.clone();
    reordered.textures.swap(0, 1);
    let diff = diff_bntx(&before, &reordered);
    assert_eq!(diff.changed.len(), 2);
    assert!(diff.changed.iter().all(|change| {
        change.order_changed
            && !change.raw_image_changed
            && !change.metadata_changed
            && !change.mip_layout_changed
            && !change.user_data_changed
    }));
}

#[test]
fn identical_bntx_diff_has_no_changes() {
    let bntx = synthetic_bntx();
    let diff = diff_bntx(&bntx, &bntx);
    assert!(!diff.has_differences());
    assert_eq!(diff.unchanged_count, bntx.textures.len());
    assert!(diff.added.is_empty() && diff.removed.is_empty() && diff.changed.is_empty());
}
