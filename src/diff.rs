use std::{fs, path::Path};

use smush_ui::{
    bntx::{Bntx, BntxDiff, diff_bntx},
    sarc::{
        SarcArchive,
        inspect::{SarcDiff, SarcDiffOptions},
    },
};

use super::{OutputFormat, RunOutcome, display_identity, path_error, write_json};

pub(super) fn run_arc(
    before: &Path,
    after: &Path,
    format: OutputFormat,
    check: bool,
    options: SarcDiffOptions,
) -> Result<RunOutcome, Box<dyn std::error::Error>> {
    let before_bytes =
        fs::read(before).map_err(|error| path_error("read before SARC input", before, error))?;
    let after_bytes =
        fs::read(after).map_err(|error| path_error("read after SARC input", after, error))?;
    let before_archive = SarcArchive::from_bytes(&before_bytes)
        .map_err(|error| path_error("parse before SARC input", before, error))?;
    let after_archive = SarcArchive::from_bytes(&after_bytes)
        .map_err(|error| path_error("parse after SARC input", after, error))?;
    let difference =
        smush_ui::sarc::inspect::diff_with_options(&before_archive, &after_archive, options);
    write_sarc_diff(&difference, format)?;
    Ok(if check && difference.has_differences() {
        RunOutcome::DifferencesFound
    } else {
        RunOutcome::Success
    })
}

pub(super) fn run_bntx(
    before: &Path,
    after: &Path,
    format: OutputFormat,
    check: bool,
) -> Result<RunOutcome, Box<dyn std::error::Error>> {
    let before_bytes =
        fs::read(before).map_err(|error| path_error("read before BNTX input", before, error))?;
    let after_bytes =
        fs::read(after).map_err(|error| path_error("read after BNTX input", after, error))?;
    let before_bntx = Bntx::from_bytes(&before_bytes)
        .map_err(|error| path_error("parse before BNTX input", before, error))?;
    let after_bntx = Bntx::from_bytes(&after_bytes)
        .map_err(|error| path_error("parse after BNTX input", after, error))?;
    let difference = diff_bntx(&before_bntx, &after_bntx);
    write_bntx_diff(&difference, format)?;
    Ok(if check && difference.has_differences() {
        RunOutcome::DifferencesFound
    } else {
        RunOutcome::Success
    })
}

fn write_sarc_diff(
    difference: &SarcDiff,
    format: OutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    if matches!(format, OutputFormat::Json) {
        return write_json(difference);
    }
    println!(
        "entries: {} -> {}; added={} removed={} changed={}",
        difference.before_entry_count,
        difference.after_entry_count,
        difference.added.len(),
        difference.removed.len(),
        difference.changed.len()
    );
    for entry in &difference.added {
        println!(
            "+ {} size={} sha256={}",
            display_identity(&entry.identity),
            entry.payload_size,
            entry.payload_sha256
        );
    }
    for entry in &difference.removed {
        println!(
            "- {} size={} sha256={}",
            display_identity(&entry.identity),
            entry.payload_size,
            entry.payload_sha256
        );
    }
    for entry in &difference.changed {
        println!(
            "~ {} size={}->{} sha256={}->{}",
            display_identity(&entry.identity),
            entry.before_payload_size,
            entry.after_payload_size,
            entry.before_payload_sha256,
            entry.after_payload_sha256
        );
        if let Some(bflyt) = &entry.bflyt {
            for change in &bflyt.changes {
                println!(
                    "    bflyt {:?} {} before={} after={}",
                    change.kind,
                    change.path,
                    display_json_value(change.before.as_ref()),
                    display_json_value(change.after.as_ref())
                );
            }
        }
        if let Some(bntx) = &entry.bntx {
            write_nested_bntx_diff(bntx);
        }
    }
    Ok(())
}

fn write_bntx_diff(
    difference: &BntxDiff,
    format: OutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    if matches!(format, OutputFormat::Json) {
        return write_json(difference);
    }
    println!(
        "textures: {} -> {}; added={} removed={} changed={} unchanged={}",
        difference.before_texture_count,
        difference.after_texture_count,
        difference.added.len(),
        difference.removed.len(),
        difference.changed.len(),
        difference.unchanged_count
    );
    for texture in &difference.added {
        println!("+ {} index={}", texture.name, texture.index);
    }
    for texture in &difference.removed {
        println!("- {} index={}", texture.name, texture.index);
    }
    for texture in &difference.changed {
        println!(
            "~ {} index={}->{} image={} metadata={} mips={} user_data={} order={}",
            texture.name,
            texture.before_index,
            texture.after_index,
            texture.raw_image_changed,
            texture.metadata_changed,
            texture.mip_layout_changed,
            texture.user_data_changed,
            texture.order_changed
        );
    }
    Ok(())
}

fn write_nested_bntx_diff(difference: &BntxDiff) {
    for texture in &difference.added {
        println!("    bntx + {} index={}", texture.name, texture.index);
    }
    for texture in &difference.removed {
        println!("    bntx - {} index={}", texture.name, texture.index);
    }
    for texture in &difference.changed {
        println!(
            "    bntx ~ {} image={} metadata={} mips={} user_data={} order={}",
            texture.name,
            texture.raw_image_changed,
            texture.metadata_changed,
            texture.mip_layout_changed,
            texture.user_data_changed,
            texture.order_changed
        );
    }
}

fn display_json_value(value: Option<&serde_jsonc2::Value>) -> String {
    let value = value
        .map(|value| serde_jsonc2::to_string(value).expect("JSON value serialization cannot fail"))
        .unwrap_or_else(|| "-".into());
    const MAX_CHARACTERS: usize = 160;
    let mut characters = value.chars();
    let prefix = characters.by_ref().take(MAX_CHARACTERS).collect::<String>();
    if characters.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}
