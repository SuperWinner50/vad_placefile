use std::path::Path;

mod vad_client;
mod vad_file;
mod vad_params;

use vad_client::VadClient;
pub use vad_file::VadFile;
pub use vad_params::{VadMessage, VadProfile};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug)]
pub enum VadError {
    TabularBlockError,
    SymbologyBlockError,
}

impl std::fmt::Display for VadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for VadError {}

fn main() -> Result<()> {
    if !Path::new("./cache/").exists() {
        std::fs::create_dir("./cache/").expect("Could not create cache directory.");
    }

    loop {
        VadClient.update()?;

        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}
