use std::{env, error::Error, fs, io};

use ssbu_ui_sandbox::SarcArchive;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<_> = env::args().skip(1).collect();
    if arguments.len() != 3 {
        return Err(usage_error().into());
    }

    match arguments[0].as_str() {
        "to-yaml" => {
            let bytes = fs::read(&arguments[1])?;
            let archive = SarcArchive::from_bytes(&bytes)?;
            fs::write(&arguments[2], archive.to_yaml_string()?)?;
        }
        "from-yaml" => {
            let yaml = fs::read_to_string(&arguments[1])?;
            let archive = SarcArchive::from_yaml_str(&yaml)?;
            fs::write(&arguments[2], archive.to_bytes()?)?;
        }
        _ => return Err(usage_error().into()),
    }

    Ok(())
}

fn usage_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "usage: ssbu-ui-sandbox <to-yaml|from-yaml> <input> <output>",
    )
}
