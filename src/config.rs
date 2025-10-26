use serde::Deserialize;
use colored::*;
use std::fs;
use std::io::ErrorKind;
use std::error::Error;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub account: String,
    pub repository: String,
    pub branch: String,
    pub token: Option<String>,

    #[serde(default = "default_interval")]
    pub interval: u64,

    #[serde(rename = "start-command", default)]
    pub start_command: Option<String>,

    #[serde(rename = "stop-command", default)]
    pub stop_command: Option<String>,
}

fn default_interval() -> u64 {
    60
}

impl Config {
    pub fn load(path: &str) -> Result<Self, Box<dyn Error>> {
        match fs::read_to_string(path) {
            Ok(data) => {
                let config: Config = serde_json::from_str(&data)?;
                println!("{} Loaded config from {}", "SUCCESS:".green().bold(), path);
                Ok(config)
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {
                eprintln!("{} Config file not found.", "FAILURE:".red().bold());
                std::process::exit(1);
            }
            Err(_) => {
                eprintln!("{} Could not read config file.", "FAILURE:".red().bold());
                std::process::exit(1);
            }
        }
    }
}
