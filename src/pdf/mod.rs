pub mod extract;
pub mod metadata;

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};

pub fn read_pdf_file(path: &PathBuf) -> Result<Vec<u8>> {
    fs::read(path).with_context(|| format!("failed to read PDF file `{}`", path.display()))
}
