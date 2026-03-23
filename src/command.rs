use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::pdf::{extract::ExtractedPaper, metadata::PaperMetadata, read_pdf_file};

pub struct ProjectWorkspace {
    pub project_root: PathBuf,
    pub config_dir: PathBuf,
    pub config_path: PathBuf,
    pub config_text: String,
}

impl ProjectWorkspace {
    /// Create a new paperout project and return its workspace context.
    pub fn init(path: &Path) -> Result<Self> {
        fs::create_dir_all(path)
            .with_context(|| format!("failed to create project directory `{}`", path.display()))?;

        let config_dir = path.join(".paperout");
        fs::create_dir_all(&config_dir)
            .with_context(|| format!("failed to create `{}`", config_dir.display()))?;

        let config_path = config_dir.join("poutconfig.toml");
        if !config_path.exists() {
            fs::write(&config_path, b"")
                .with_context(|| format!("failed to create `{}`", config_path.display()))?;
        }

        Self::load(path)
    }

    /// Load an existing paperout project from a directory and read its config file.
    pub fn load(path: &Path) -> Result<Self> {
        let project_root = path.to_path_buf();
        let config_dir = project_root.join(".paperout");
        let config_path = config_dir.join("poutconfig.toml");

        if !config_path.exists() {
            bail!(
                "project is not initialized: missing `{}`",
                config_path.display()
            )
        }

        let config_text = fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read `{}`", config_path.display()))?;

        Ok(Self {
            project_root,
            config_dir,
            config_path,
            config_text,
        })
    }

    /// Run the `add` workflow inside this workspace and create a markdown note.
    pub fn add_paper(&self, pdf_path: &Path) -> Result<PathBuf> {
        let file_buf = read_pdf_file(&pdf_path.to_path_buf())?;
        let paper = ExtractedPaper::from_pdf_bytes(file_buf)?;
        let metadata = PaperMetadata::from_extracted(&paper)?;

        let file_name = sanitize_title(&metadata.title);
        let markdown_path = self.project_root.join(format!("{file_name}.md"));

        fs::write(&markdown_path, metadata.to_markdown())
            .with_context(|| format!("failed to write `{}`", markdown_path.display()))?;

        Ok(markdown_path)
    }

    /// Run the `add` workflow for multiple input paths.
    pub fn add_inputs(&self, inputs: &[PathBuf], recursive: bool) -> Result<Vec<PathBuf>> {
        let pdf_paths = collect_pdf_paths(inputs, recursive)?;
        let mut markdown_paths = Vec::with_capacity(pdf_paths.len());

        for pdf_path in pdf_paths {
            markdown_paths.push(self.add_paper(&pdf_path)?);
        }

        Ok(markdown_paths)
    }
}

fn sanitize_title(title: &str) -> String {
    let mut out = String::new();

    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if (ch.is_whitespace() || ch == '-' || ch == '_') && !out.ends_with('-') {
            out.push('-');
        }
    }

    out.trim_matches('-').to_string()
}

fn collect_pdf_paths(inputs: &[PathBuf], recursive: bool) -> Result<Vec<PathBuf>> {
    let mut pdf_paths = Vec::new();

    for input in inputs {
        if input.is_file() {
            if is_pdf_path(input) {
                pdf_paths.push(input.clone());
            }
            continue;
        }

        if input.is_dir() {
            collect_pdf_paths_from_dir(input, recursive, &mut pdf_paths)?;
            continue;
        }

        bail!("input does not exist: `{}`", input.display());
    }

    if pdf_paths.is_empty() {
        bail!("no PDF files found in inputs");
    }

    Ok(pdf_paths)
}

fn collect_pdf_paths_from_dir(
    dir: &Path,
    recursive: bool,
    pdf_paths: &mut Vec<PathBuf>,
) -> Result<()> {
    for entry in fs::read_dir(dir)
        .with_context(|| format!("failed to read directory `{}`", dir.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in `{}`", dir.display()))?;
        let path = entry.path();

        if path.is_file() && is_pdf_path(&path) {
            pdf_paths.push(path);
        } else if recursive && path.is_dir() {
            collect_pdf_paths_from_dir(&path, true, pdf_paths)?;
        }
    }

    Ok(())
}

fn is_pdf_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
}
