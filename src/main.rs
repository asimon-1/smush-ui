use std::{fs::File, io::{BufReader, BufWriter}, path::PathBuf};

use binrw::{BinReaderExt, BinWriterExt};
use clap::{Parser, Subcommand};
use smush_ui::bflyt::Bflyt;

#[derive(Parser)]
#[command(about = "Convert Nintendo BFLYT layouts between binary and JSON")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Deserialize a little-endian BFLYT file to pretty-printed JSON.
    ToJson {
        input: PathBuf,
        output: PathBuf,
    },
    /// Serialize JSON to a little-endian BFLYT file.
    FromJson {
        input: PathBuf,
        output: PathBuf,
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
    }

    Ok(())
}
