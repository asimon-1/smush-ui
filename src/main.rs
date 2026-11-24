mod diff;

use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use binrw::{BinReaderExt, BinWriterExt};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use smush_ui::{
    bflan::Bflan,
    bflyt::Bflyt,
    bntx::{
        AddTextureOptions, Bntx, BntxInspection, ChannelMode, NewTextureFormat, TextureProfile,
        fs::{
            PackOptions as BntxPackOptions, UnpackOptions as BntxUnpackOptions,
            add_texture_to_file as add_bntx_texture,
            create_from_png_directory as create_bntx_directory,
            pack_from_directory as pack_bntx_directory,
            unpack_to_directory as unpack_bntx_directory,
        },
        inspect_bntx,
        patch::{PatchSourceFormat, create_bntx_patch, patch_bntx_file},
    },
    patch::{PatchArcOptions, patch_archive},
    sarc::{
        SarcArchive,
        fs::{insert_file, pack_from_directory, unpack_to_directory, write_archive_atomic},
        inspect::{SarcDiffOptions, SarcEntryIdentity, SarcInspection},
    },
};

#[derive(Parser)]
#[command(
    about = "Edit Nintendo Switch UI layouts and archives",
    after_help = "Command tree:\n  bflyt\n    to-json\n    from-json\n  bflan\n    to-json\n    from-json\n  arc\n    unpack\n    pack\n    add-file\n    patch\n    list\n    diff\n  bntx\n    unpack\n    pack\n    add-texture\n    patch\n    create-patch\n    create\n    list\n    diff\n\nRun 'smush-ui <FILETYPE> --help' for required arguments, or 'smush-ui <FILETYPE> <COMMAND> --help' for options and examples."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read and write BFLYT layouts.
    #[command(
        after_help = "Required arguments:\n  to-json <INPUT> <OUTPUT>\n  from-json <INPUT> <OUTPUT>\n\nRun 'smush-ui bflyt <COMMAND> --help' for options and examples."
    )]
    Bflyt {
        #[command(subcommand)]
        command: BflytCommand,
    },
    /// Read and write BFLAN layout animations.
    #[command(
        after_help = "Required arguments:\n  to-json <INPUT> <OUTPUT>\n  from-json <INPUT> <OUTPUT>\n\nRun 'smush-ui bflan <COMMAND> --help' for options and examples."
    )]
    Bflan {
        #[command(subcommand)]
        command: BflanCommand,
    },
    /// Read and write SARC archives.
    #[command(
        after_help = "Required arguments:\n  unpack <INPUT> <OUTPUT_DIRECTORY>\n  pack <INPUT_DIRECTORY> <OUTPUT>\n  add-file <INPUT> <SOURCE_FILE> <ARCHIVE_PATH> <OUTPUT>\n  patch <INPUT> <MANIFEST> <OUTPUT>\n  list <INPUT>\n  diff <BEFORE> <AFTER>\n\nRun 'smush-ui arc <COMMAND> --help' for options and examples."
    )]
    Arc {
        #[command(subcommand)]
        command: ArcCommand,
    },
    /// Read and write BNTX texture containers.
    #[command(
        after_help = "Required arguments:\n  unpack <INPUT> <OUTPUT_DIRECTORY>\n  pack <INPUT_DIRECTORY> <OUTPUT>\n  add-texture <INPUT> <SOURCE_PNG> <TEXTURE_NAME> <OUTPUT>\n  patch <INPUT> <PATCH_DIRECTORY> <OUTPUT>\n  create-patch <BASE> <MODIFIED> <PATCH_DIRECTORY>\n  create <INPUT_PNG_DIRECTORY> <OUTPUT>\n  list <INPUT>\n  diff <BEFORE> <AFTER>\n\nRun 'smush-ui bntx <COMMAND> --help' for options and examples."
    )]
    Bntx {
        #[command(subcommand)]
        command: BntxCommand,
    },
}

#[derive(Subcommand)]
enum BflytCommand {
    /// Deserialize a little-endian BFLYT file to pretty-printed JSON.
    #[command(after_help = "Example:\n  smush-ui bflyt to-json layout.bflyt layout.json")]
    ToJson { input: PathBuf, output: PathBuf },
    /// Serialize JSON to a little-endian BFLYT file.
    #[command(after_help = "Example:\n  smush-ui bflyt from-json layout.json rebuilt-layout.bflyt")]
    FromJson { input: PathBuf, output: PathBuf },
}

#[derive(Subcommand)]
enum BflanCommand {
    /// Deserialize a little-endian BFLAN file to formatted JSON.
    #[command(after_help = "Example:\n  smush-ui bflan to-json animation.bflan animation.json")]
    ToJson { input: PathBuf, output: PathBuf },
    /// Serialize JSON to a little-endian BFLAN file.
    #[command(after_help = "Example:\n  smush-ui bflan from-json animation.json rebuilt.bflan")]
    FromJson { input: PathBuf, output: PathBuf },
}

#[derive(Subcommand)]
enum ArcCommand {
    /// Extract the named files in a SARC archive.
    #[command(after_help = "Example:\n  smush-ui arc unpack archive.arc archive-files")]
    Unpack {
        input: PathBuf,
        output_directory: PathBuf,
    },
    /// Pack a directory tree into a SARC archive.
    #[command(after_help = "Example:\n  smush-ui arc pack archive-files rebuilt.arc")]
    Pack {
        input_directory: PathBuf,
        output: PathBuf,
    },
    /// Insert an opaque file into an existing SARC archive.
    #[command(
        after_help = "Example:\n  smush-ui arc add-file archive.arc asset.bin assets/asset.bin updated.arc"
    )]
    AddFile {
        input: PathBuf,
        source_file: PathBuf,
        archive_path: String,
        output: PathBuf,
        /// Replace an existing entry with the same archive path.
        #[arg(long)]
        replace: bool,
    },
    /// Apply a versioned patch manifest to a SARC archive.
    #[command(
        after_help = "Example:\n  smush-ui arc patch archive.arc patch.json updated.arc\n\nThe manifest uses schema_version 1 and upsert, patch, or remove entries. Source paths are relative to the manifest."
    )]
    Patch {
        input: PathBuf,
        manifest: PathBuf,
        output: PathBuf,
    },
    /// List files stored in a SARC archive.
    #[command(after_help = "Example:\n  smush-ui arc list archive.arc --format json")]
    List {
        input: PathBuf,
        #[arg(long, value_enum, default_value = "text")]
        format: OutputFormat,
    },
    /// Compare the files stored in two SARC archives.
    #[command(
        after_help = "Example:\n  smush-ui arc diff original.arc modified.arc --check --expand-bflyt"
    )]
    Diff {
        before: PathBuf,
        after: PathBuf,
        #[arg(long, value_enum, default_value = "text")]
        format: OutputFormat,
        /// Return status 3 when differences are found.
        #[arg(long)]
        check: bool,
        /// Include semantic field changes for embedded BFLYT files.
        #[arg(long)]
        expand_bflyt: bool,
        /// Include semantic field changes for embedded BFLAN files.
        #[arg(long)]
        expand_bflan: bool,
        /// Include texture-level changes for embedded BNTX files.
        #[arg(long)]
        expand_bntx: bool,
    },
}

#[derive(Subcommand)]
enum BntxCommand {
    /// Extract a BNTX container to a lossless editable bundle.
    #[command(after_help = "Example:\n  smush-ui bntx unpack textures.bntx texture-bundle")]
    Unpack {
        input: PathBuf,
        output_directory: PathBuf,
        /// Do not create editable PNG previews.
        #[arg(long)]
        no_preview: bool,
        /// Expose stored channels instead of applying texture channel selectors.
        #[arg(long)]
        raw_channels: bool,
    },
    /// Rebuild a BNTX container from a lossless bundle.
    #[command(
        after_help = "Example:\n  smush-ui bntx pack texture-bundle rebuilt.bntx --from-png\n\nWithout --from-png, packing uses the lossless binary sidecars."
    )]
    Pack {
        input_directory: PathBuf,
        output: PathBuf,
        /// Re-encode changed PNG previews instead of using their lossless raw sidecars.
        #[arg(long)]
        from_png: bool,
    },
    /// Add or replace one PNG texture in a BNTX container.
    #[command(
        after_help = "Example:\n  smush-ui bntx add-texture textures.bntx image.png new_texture updated.bntx --like template_texture"
    )]
    AddTexture {
        input: PathBuf,
        source_png: PathBuf,
        texture_name: String,
        output: PathBuf,
        /// Copy the texture profile from an existing texture.
        #[arg(long, conflicts_with = "format")]
        like: Option<String>,
        /// Use an explicit format; new textures default to BC7 UNORM.
        #[arg(long, value_enum, conflicts_with = "like")]
        format: Option<BntxTextureFormat>,
        /// Replace an existing texture with the requested name.
        #[arg(long)]
        replace: bool,
    },
    /// Apply a texture patch directory to a BNTX container.
    #[command(
        after_help = "Example:\n  smush-ui bntx patch original.bntx delta updated.bntx\n\nThe directory must contain bntx-patch.json or bntx-patch.jsonc."
    )]
    Patch {
        input: PathBuf,
        patch_directory: PathBuf,
        output: PathBuf,
    },
    /// Generate a delta bundle between two BNTX containers.
    #[command(
        after_help = "Example:\n  smush-ui bntx create-patch original.bntx modified.bntx delta --source-format png\n\nUse --source-format bin to store binary texture data. PNG mode falls back to binary when needed."
    )]
    CreatePatch {
        base: PathBuf,
        modified: PathBuf,
        patch_directory: PathBuf,
        #[arg(long, value_enum, default_value = "png")]
        source_format: BntxPatchSourceFormat,
    },
    /// Create a new BNTX container from one directory of PNG files.
    #[command(
        after_help = "Example:\n  smush-ui bntx create images textures.bntx --name texture_container --format bc7-unorm"
    )]
    Create {
        input_png_directory: PathBuf,
        output: PathBuf,
        /// Override the container name derived from the output filename.
        #[arg(long)]
        name: Option<String>,
        /// Texture format used for every input PNG.
        #[arg(long, value_enum, default_value = "bc7-unorm")]
        format: BntxTextureFormat,
    },
    /// List textures stored in a BNTX container.
    #[command(after_help = "Example:\n  smush-ui bntx list textures.bntx --format json")]
    List {
        input: PathBuf,
        #[arg(long, value_enum, default_value = "text")]
        format: OutputFormat,
    },
    /// Compare textures stored in two BNTX containers.
    #[command(
        after_help = "Example:\n  smush-ui bntx diff original.bntx modified.bntx --check --format json"
    )]
    Diff {
        before: PathBuf,
        after: PathBuf,
        #[arg(long, value_enum, default_value = "text")]
        format: OutputFormat,
        /// Return status 3 when differences are found.
        #[arg(long)]
        check: bool,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum BntxTextureFormat {
    Bc1Unorm,
    Bc1Srgb,
    Bc4Unorm,
    Bc5Unorm,
    Bc7Unorm,
    Bc7Srgb,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum BntxPatchSourceFormat {
    Bin,
    Png,
}

impl From<BntxPatchSourceFormat> for PatchSourceFormat {
    fn from(value: BntxPatchSourceFormat) -> Self {
        match value {
            BntxPatchSourceFormat::Bin => Self::Bin,
            BntxPatchSourceFormat::Png => Self::Png,
        }
    }
}

impl From<BntxTextureFormat> for NewTextureFormat {
    fn from(value: BntxTextureFormat) -> Self {
        match value {
            BntxTextureFormat::Bc1Unorm => Self::Bc1Unorm,
            BntxTextureFormat::Bc1Srgb => Self::Bc1Srgb,
            BntxTextureFormat::Bc4Unorm => Self::Bc4Unorm,
            BntxTextureFormat::Bc5Unorm => Self::Bc5Unorm,
            BntxTextureFormat::Bc7Unorm => Self::Bc7Unorm,
            BntxTextureFormat::Bc7Srgb => Self::Bc7Srgb,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RunOutcome {
    Success,
    DifferencesFound,
}

fn main() -> ExitCode {
    match run() {
        Ok(RunOutcome::Success) => ExitCode::SUCCESS,
        Ok(RunOutcome::DifferencesFound) => ExitCode::from(3),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<RunOutcome, Box<dyn std::error::Error>> {
    match Cli::parse().command {
        Command::Bflan { command } => match command {
            BflanCommand::ToJson { input, output } => {
                let bytes = fs::read(&input)
                    .map_err(|error| path_error("read BFLAN input", &input, error))?;
                let animation = Bflan::from_bytes(&bytes)
                    .map_err(|error| path_error("parse BFLAN input", &input, error))?;
                let output_file = File::create(&output)
                    .map_err(|error| path_error("create JSON output", &output, error))?;
                serde_jsonc2::to_writer_pretty(BufWriter::new(output_file), &animation)
                    .map_err(|error| path_error("write JSON output", &output, error))?;
            }
            BflanCommand::FromJson { input, output } => {
                let input_file = File::open(&input)
                    .map_err(|error| path_error("open JSON input", &input, error))?;
                let animation: Bflan = serde_jsonc2::from_reader(BufReader::new(input_file))
                    .map_err(|error| path_error("parse JSON input", &input, error))?;
                let bytes = animation
                    .to_bytes()
                    .map_err(|error| path_error("serialize BFLAN", &output, error))?;
                fs::write(&output, bytes)
                    .map_err(|error| path_error("write BFLAN output", &output, error))?;
            }
        },
        Command::Bflyt { command } => match command {
            BflytCommand::ToJson { input, output } => {
                let input_file = File::open(&input)
                    .map_err(|error| path_error("open BFLYT input", &input, error))?;
                let layout: Bflyt = BufReader::new(input_file)
                    .read_le()
                    .map_err(|error| path_error("parse BFLYT input", &input, error))?;
                let output_file = File::create(&output)
                    .map_err(|error| path_error("create JSON output", &output, error))?;
                serde_jsonc2::to_writer_pretty(BufWriter::new(output_file), &layout)
                    .map_err(|error| path_error("write JSON output", &output, error))?;
            }
            BflytCommand::FromJson { input, output } => {
                let input_file = File::open(&input)
                    .map_err(|error| path_error("open JSON input", &input, error))?;
                let layout: Bflyt = serde_jsonc2::from_reader(BufReader::new(input_file))
                    .map_err(|error| path_error("parse JSON input", &input, error))?;
                let output_file = File::create(&output)
                    .map_err(|error| path_error("create BFLYT output", &output, error))?;
                BufWriter::new(output_file)
                    .write_le(&layout)
                    .map_err(|error| path_error("write BFLYT output", &output, error))?;
            }
        },
        Command::Arc { command } => match command {
            ArcCommand::Unpack {
                input,
                output_directory,
            } => {
                let bytes = fs::read(&input)
                    .map_err(|error| path_error("read SARC input", &input, error))?;
                let archive = SarcArchive::from_bytes(&bytes)
                    .map_err(|error| path_error("parse SARC input", &input, error))?;
                unpack_to_directory(&archive, &output_directory).map_err(|error| {
                    paths_error("unpack SARC archive", &input, &output_directory, error)
                })?;
            }
            ArcCommand::Pack {
                input_directory,
                output,
            } => {
                pack_from_directory(&input_directory, &output).map_err(|error| {
                    paths_error("pack SARC archive", &input_directory, &output, error)
                })?;
            }
            ArcCommand::AddFile {
                input,
                source_file,
                archive_path,
                output,
                replace,
            } => {
                let bytes = fs::read(&input)
                    .map_err(|error| path_error("read SARC input", &input, error))?;
                let mut archive = SarcArchive::from_bytes(&bytes)
                    .map_err(|error| path_error("parse SARC input", &input, error))?;
                let source_data = fs::read(&source_file)
                    .map_err(|error| path_error("read insertion source", &source_file, error))?;
                insert_file(&mut archive, archive_path, source_data, replace)
                    .map_err(|error| path_error("insert archive entry into", &input, error))?;
                write_archive_atomic(&archive, &output)?;
            }
            ArcCommand::Patch {
                input,
                manifest,
                output,
            } => {
                patch_archive(&input, &manifest, &output, PatchArcOptions)
                    .map_err(|error| paths_error("patch SARC archive", &input, &output, error))?;
            }
            ArcCommand::List { input, format } => {
                let bytes = fs::read(&input)
                    .map_err(|error| path_error("read SARC input", &input, error))?;
                let archive = SarcArchive::from_bytes(&bytes)
                    .map_err(|error| path_error("parse SARC input", &input, error))?;
                let inspection = smush_ui::sarc::inspect::inspect(&archive);
                write_sarc_inspection(&inspection, format)?;
            }
            ArcCommand::Diff {
                before,
                after,
                format,
                check,
                expand_bflyt,
                expand_bflan,
                expand_bntx,
            } => {
                return diff::run_arc(
                    &before,
                    &after,
                    format,
                    check,
                    SarcDiffOptions {
                        expand_bflyt,
                        expand_bflan,
                        expand_bntx,
                    },
                );
            }
        },
        Command::Bntx { command } => match command {
            BntxCommand::Unpack {
                input,
                output_directory,
                no_preview,
                raw_channels,
            } => {
                let bytes = fs::read(&input)
                    .map_err(|error| path_error("read BNTX input", &input, error))?;
                let bntx = Bntx::from_bytes(&bytes)
                    .map_err(|error| path_error("parse BNTX input", &input, error))?;
                let options = BntxUnpackOptions {
                    write_previews: !no_preview,
                    channel_mode: if raw_channels {
                        ChannelMode::Raw
                    } else {
                        ChannelMode::Resolved
                    },
                };
                unpack_bntx_directory(&bntx, &output_directory, options).map_err(|error| {
                    paths_error("unpack BNTX bundle", &input, &output_directory, error)
                })?;
            }
            BntxCommand::Pack {
                input_directory,
                output,
                from_png,
            } => {
                pack_bntx_directory(&input_directory, &output, BntxPackOptions { from_png })
                    .map_err(|error| {
                        paths_error("pack BNTX bundle", &input_directory, &output, error)
                    })?;
            }
            BntxCommand::AddTexture {
                input,
                source_png,
                texture_name,
                output,
                like,
                format,
                replace,
            } => {
                let profile = like
                    .map(TextureProfile::Like)
                    .or_else(|| format.map(|format| TextureProfile::Format(format.into())));
                add_bntx_texture(
                    &input,
                    &source_png,
                    texture_name,
                    &output,
                    AddTextureOptions { profile, replace },
                )
                .map_err(|error| paths_error("add BNTX texture", &input, &output, error))?;
            }
            BntxCommand::Patch {
                input,
                patch_directory,
                output,
            } => {
                patch_bntx_file(&input, &patch_directory, &output)
                    .map_err(|error| paths_error("patch BNTX", &input, &output, error))?;
            }
            BntxCommand::CreatePatch {
                base,
                modified,
                patch_directory,
                source_format,
            } => {
                let summary =
                    create_bntx_patch(&base, &modified, &patch_directory, source_format.into())
                        .map_err(|error| {
                            paths_error("create BNTX patch", &base, &patch_directory, error)
                        })?;
                println!("{} upserts, {} removals", summary.upserts, summary.removals);
                for name in summary.png_fallbacks {
                    println!("BIN fallback: {name}");
                }
            }
            BntxCommand::Create {
                input_png_directory,
                output,
                name,
                format,
            } => {
                create_bntx_directory(
                    &input_png_directory,
                    &output,
                    name.as_deref(),
                    format.into(),
                )
                .map_err(|error| {
                    paths_error("create BNTX", &input_png_directory, &output, error)
                })?;
            }
            BntxCommand::List { input, format } => {
                let bytes = fs::read(&input)
                    .map_err(|error| path_error("read BNTX input", &input, error))?;
                let bntx = Bntx::from_bytes(&bytes)
                    .map_err(|error| path_error("parse BNTX input", &input, error))?;
                write_bntx_inspection(&inspect_bntx(&bntx), format)?;
            }
            BntxCommand::Diff {
                before,
                after,
                format,
                check,
            } => return diff::run_bntx(&before, &after, format, check),
        },
    }

    Ok(RunOutcome::Success)
}

#[derive(Serialize)]
struct JsonOutput<'a, T> {
    schema_version: u32,
    result: &'a T,
}

fn write_json<T: Serialize>(value: &T) -> Result<(), Box<dyn std::error::Error>> {
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    serde_jsonc2::to_writer_pretty(
        &mut writer,
        &JsonOutput {
            schema_version: 1,
            result: value,
        },
    )?;
    writeln!(writer)?;
    Ok(())
}

fn write_sarc_inspection(
    inspection: &SarcInspection,
    format: OutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    if matches!(format, OutputFormat::Json) {
        return write_json(inspection);
    }
    println!("entries: {}", inspection.entry_count);
    for entry in &inspection.entries {
        println!(
            "[{}] {} size={} sha256={} kind={} extension={}",
            entry.stored_index,
            display_identity(&entry.identity),
            entry.payload_size,
            entry.payload_sha256,
            entry.detected_kind.as_str(),
            entry.extension.as_deref().unwrap_or("-")
        );
    }
    Ok(())
}

fn write_bntx_inspection(
    inspection: &BntxInspection,
    format: OutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    if matches!(format, OutputFormat::Json) {
        return write_json(inspection);
    }
    println!("textures: {}", inspection.texture_count);
    for texture in &inspection.textures {
        println!(
            "[{}] {} {}x{}x{} format=0x{:08x} {:?}/{:?} mips={} image_size={} sha256={}",
            texture.index,
            texture.name,
            texture.width,
            texture.height,
            texture.depth,
            texture.raw_format,
            texture.channel_format,
            texture.type_format,
            texture.mip_count,
            texture.image_size,
            texture.image_sha256
        );
    }
    Ok(())
}

fn display_identity(identity: &SarcEntryIdentity) -> String {
    match identity {
        SarcEntryIdentity::Path { path } => path.clone(),
        SarcEntryIdentity::Hash { hash } => format!("hash:{hash}"),
    }
}

fn path_error(action: &str, path: &Path, error: impl std::fmt::Display) -> io::Error {
    io::Error::other(format!("failed to {action} {}: {error}", path.display()))
}

fn paths_error(
    action: &str,
    input: &Path,
    output: &Path,
    error: impl std::fmt::Display,
) -> io::Error {
    io::Error::other(format!(
        "failed to {action} from {} to {}: {error}",
        input.display(),
        output.display()
    ))
}
