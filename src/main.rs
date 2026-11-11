use std::{
    fs::{self, File},
    io::{BufReader, BufWriter, Write},
    path::PathBuf,
};

use atomic_write_file::AtomicWriteFile;
use binrw::{BinReaderExt, BinWriterExt};
use clap::{Parser, Subcommand};
use smush_ui::{
    bflyt::Bflyt,
    sarc::{SarcArchive, fs::insert_file},
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse().command {
        Command::ToJson { input, output } => {
            let mut input = BufReader::new(File::open(input)?);
            let layout: Bflyt = input.read_le()?;
            serde_json::to_writer_pretty(BufWriter::new(File::create(output)?), &layout)?;
        }
        Command::FromJson { input, output } => {
            let layout: Bflyt = serde_json::from_reader(BufReader::new(File::open(input)?))?;
            let mut output = BufWriter::new(File::create(output)?);
            output.write_le(&layout)?;
        }
        Command::AddArcFile {
            input,
            source_file,
            archive_path,
            output,
            replace,
        } => {
            let mut archive = SarcArchive::from_bytes(&fs::read(input)?)?;
            insert_file(&mut archive, archive_path, fs::read(source_file)?, replace)?;
            let bytes = archive.to_bytes()?;
            let mut output = AtomicWriteFile::options().open(output)?;
            output.write_all(&bytes)?;
            output.commit()?;
        }
    }

    Ok(())
}
