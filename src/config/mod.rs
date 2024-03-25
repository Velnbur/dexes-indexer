use std::{io::Read, path::PathBuf};

use color_eyre::eyre::{self, Context};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub ethereum: EthereumNode,

    pub database: Database,

    pub logger: Logger,
}

#[derive(Debug, Deserialize)]
pub struct EthereumNode {
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct Database {
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct Logger {
    pub level: String,
    pub path: Option<String>,
}

impl Config {
    pub fn load(path: impl Into<PathBuf>) -> eyre::Result<Self> {
        let path = path.into();
        let mut file = std::fs::File::open(&path)?;

        let mut buff = String::new();

        file.read_to_string(&mut buff)
            .wrap_err("Failed to read file")?;

        let config = toml::from_str(&buff)?;

        Ok(config)
    }
}
