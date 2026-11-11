use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use atomic_write_file::AtomicWriteFile;
use binrw::{BinReaderExt, BinWriterExt};
use clap::{Parser, Subcommand};
use smush_ui::{
    bflyt::Bflyt,
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
            serde_json::to_writer_pretty(BufWriter::new(output_file), &layout)
                .map_err(|error| path_error("write JSON output", &output, error))?;
        }
        Command::FromJson { input, output } => {
            let input_file =
                File::open(&input).map_err(|error| path_error("open JSON input", &input, error))?;
            let layout: Bflyt = serde_json::from_reader(BufReader::new(input_file))
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
