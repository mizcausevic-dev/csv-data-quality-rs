//! Validate CSV using a saved registry contract JSON document.
//! Usage: cargo run --example validate_contract_file -- contract.json data.csv

use std::env;
use std::fs::File;
use std::io::{self, BufReader};
use std::path::PathBuf;
use std::process::ExitCode;

use csv_data_quality::{Contract, Validator};

fn run() -> Result<bool, Box<dyn std::error::Error>> {
    let paths: Vec<PathBuf> = env::args_os().skip(1).map(PathBuf::from).collect();
    if paths.len() != 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: validate_contract_file <contract.json> <data.csv>",
        )
        .into());
    }
    let contract = Contract::from_json(&std::fs::read_to_string(&paths[0])?)?;
    let csv = BufReader::new(File::open(&paths[1])?);
    let report = Validator::new(contract).validate_reader(csv)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(report.valid)
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(2),
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}
