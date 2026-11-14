use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use atomic_write_file::AtomicWriteFile;
use binrw::{BinReaderExt, BinWriterExt};
use clap::{Parser, Subcommand, ValueEnum};
use smush_ui::{
    bflyt::Bflyt,
    bntx::{
        AddTextureOptions, Bntx, ChannelMode, NewTextureFormat, TextureProfile,
        fs::{
            PackOptions as BntxPackOptions, UnpackOptions as BntxUnpackOptions,
            add_texture_to_file as add_bntx_texture,
            create_from_png_directory as create_bntx_directory,
            pack_from_directory as pack_bntx_directory,
            unpack_to_directory as unpack_bntx_directory,
        },
    },
    sarc::{
        SarcArchive,
        fs::{insert_file, pack_from_directory, unpack_to_directory},
    },
};

#[derive(Parser)]
#[command(about = "Edit Nintendo Switch UI layouts and archives")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Deserialize a little-endian BFLYT file to pretty-printed JSON.
    ToJson { input: PathBuf, output: PathBuf },
    /// Serialize JSON to a little-endian BFLYT file.
    FromJson { input: PathBuf, output: PathBuf },
    /// Extract the named files in a SARC archive.
    UnpackArc {
        input: PathBuf,
        output_directory: PathBuf,
    },
    /// Pack a directory tree into a SARC archive.
    PackArc {
        input_directory: PathBuf,
        output: PathBuf,
    },
    /// Insert an opaque file into an existing SARC archive.
    AddArcFile {
        input: PathBuf,
        source_file: PathBuf,
        archive_path: String,
        output: PathBuf,
        /// Replace an existing entry with the same archive path.
        #[arg(long)]
        replace: bool,
    },
    /// Extract a BNTX container to a lossless editable bundle.
    UnpackBntx {
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
    PackBntx {
        input_directory: PathBuf,
        output: PathBuf,
        /// Re-encode changed PNG previews instead of using their lossless raw sidecars.
        #[arg(long)]
        from_png: bool,
    },
    /// Add or replace one PNG texture in a BNTX container.
    AddBntxTexture {
        input: PathBuf,
        source_png: PathBuf,
        texture_name: String,
        output: PathBuf,
        /// Copy the texture profile and mip count from an existing texture.
        #[arg(long, conflicts_with = "format")]
        like: Option<String>,
        /// Use an explicit format; new textures default to BC7 UNORM.
        #[arg(long, value_enum, conflicts_with = "like")]
        format: Option<BntxTextureFormat>,
        /// Replace an existing texture with the requested name.
        #[arg(long)]
        replace: bool,
    },
    /// Create a new BNTX container from one directory of PNG files.
    CreateBntx {
        input_png_directory: PathBuf,
        output: PathBuf,
        /// Override the container name derived from the output filename.
        #[arg(long)]
        name: Option<String>,
        /// Texture format used for every input PNG.
        #[arg(long, value_enum, default_value = "bc7-unorm")]
        format: BntxTextureFormat,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum BntxTextureFormat {
    Bc4Unorm,
    Bc5Unorm,
    Bc7Unorm,
    Bc7Srgb,
}

impl From<BntxTextureFormat> for NewTextureFormat {
    fn from(value: BntxTextureFormat) -> Self {
        match value {
            BntxTextureFormat::Bc4Unorm => Self::Bc4Unorm,
            BntxTextureFormat::Bc5Unorm => Self::Bc5Unorm,
            BntxTextureFormat::Bc7Unorm => Self::Bc7Unorm,
            BntxTextureFormat::Bc7Srgb => Self::Bc7Srgb,
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse().command {
        Command::ToJson { input, output } => {
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
        Command::FromJson { input, output } => {
            let input_file =
                File::open(&input).map_err(|error| path_error("open JSON input", &input, error))?;
            let layout: Bflyt = serde_jsonc2::from_reader(BufReader::new(input_file))
                .map_err(|error| path_error("parse JSON input", &input, error))?;
            let output_file = File::create(&output)
                .map_err(|error| path_error("create BFLYT output", &output, error))?;
            BufWriter::new(output_file)
                .write_le(&layout)
                .map_err(|error| path_error("write BFLYT output", &output, error))?;
        }
        Command::UnpackArc {
            input,
            output_directory,
        } => {
            let bytes =
                fs::read(&input).map_err(|error| path_error("read SARC input", &input, error))?;
            let archive = SarcArchive::from_bytes(&bytes)
                .map_err(|error| path_error("parse SARC input", &input, error))?;
            unpack_to_directory(&archive, &output_directory).map_err(|error| {
                paths_error("unpack SARC archive", &input, &output_directory, error)
            })?;
        }
        Command::PackArc {
            input_directory,
            output,
        } => {
            pack_from_directory(&input_directory, &output).map_err(|error| {
                paths_error("pack SARC archive", &input_directory, &output, error)
            })?;
        }
        Command::AddArcFile {
            input,
            source_file,
            archive_path,
            output,
            replace,
        } => {
            let bytes =
                fs::read(&input).map_err(|error| path_error("read SARC input", &input, error))?;
            let mut archive = SarcArchive::from_bytes(&bytes)
                .map_err(|error| path_error("parse SARC input", &input, error))?;
            let source_data = fs::read(&source_file)
                .map_err(|error| path_error("read insertion source", &source_file, error))?;
            insert_file(&mut archive, archive_path, source_data, replace)
                .map_err(|error| path_error("insert archive entry into", &input, error))?;
            let bytes = archive
                .to_bytes()
                .map_err(|error| path_error("serialize SARC output", &output, error))?;
            let mut output_file = AtomicWriteFile::options()
                .open(&output)
                .map_err(|error| path_error("create atomic SARC output", &output, error))?;
            output_file
                .write_all(&bytes)
                .map_err(|error| path_error("write atomic SARC output", &output, error))?;
            output_file
                .commit()
                .map_err(|error| path_error("replace SARC output", &output, error))?;
        }
        Command::UnpackBntx {
            input,
            output_directory,
            no_preview,
            raw_channels,
        } => {
            let bytes =
                fs::read(&input).map_err(|error| path_error("read BNTX input", &input, error))?;
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
        Command::PackBntx {
            input_directory,
            output,
            from_png,
        } => {
            pack_bntx_directory(&input_directory, &output, BntxPackOptions { from_png }).map_err(
                |error| paths_error("pack BNTX bundle", &input_directory, &output, error),
            )?;
        }
        Command::AddBntxTexture {
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
        Command::CreateBntx {
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
            .map_err(|error| paths_error("create BNTX", &input_png_directory, &output, error))?;
        }
    }

    Ok(())
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
