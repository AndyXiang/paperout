use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use toml::Value;

use crate::{
    pdf::{extract::ExtractedPaper, metadata::PaperMetadata, read_pdf_file},
    persist::{PersistenceConfig, PersistentNoteBuilder},
};

const DEFAULT_LIBRARY_DIR: &str = "Library";
const DEFAULT_ASSETS_DIR: &str = "Assets";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectConfig {
    pub library_path: PathBuf,
    pub assets_path: PathBuf,
}

pub struct ProjectWorkspace {
    pub project_root: PathBuf,
    pub config: ProjectConfig,
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

        if config_path.exists() {
            return Self::load(path);
        }

        let config = ProjectConfig::default();
        config.ensure_layout(path)?;
        fs::write(&config_path, config.to_toml())
            .with_context(|| format!("failed to write `{}`", config_path.display()))?;

        Self::load(path)
    }

    /// Load an existing paperout project from a directory and read its config file.
    pub fn load(path: &Path) -> Result<Self> {
        let project_root = find_project_root(path)?;
        let config_path = project_root.join(".paperout").join("poutconfig.toml");

        let config_text = fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read `{}`", config_path.display()))?;
        let config = ProjectConfig::from_toml(&config_text)?;
        config.ensure_layout(&project_root)?;

        Ok(Self {
            project_root,
            config,
        })
    }

    /// Run the `add` workflow inside this workspace and create a markdown note.
    pub fn add_paper(&self, pdf_path: &Path) -> Result<PathBuf> {
        let file_buf = read_pdf_file(pdf_path)?;
        let paper = ExtractedPaper::from_pdf_bytes(file_buf)?;
        let mut metadata = PaperMetadata::from_extracted(&paper)?;
        metadata.asset_id = build_asset_id(&metadata);

        let assets_dir = self.project_root.join(&self.config.assets_path);
        let asset_dir = assets_dir.join(&metadata.asset_id);
        let pdf_file_name = pdf_path
            .file_name()
            .context("failed to resolve PDF file name")?;
        let asset_path = asset_dir.join(pdf_file_name);

        fs::create_dir_all(&asset_dir)
            .with_context(|| format!("failed to create `{}`", asset_dir.display()))?;

        fs::copy(pdf_path, &asset_path).with_context(|| {
            format!(
                "failed to copy `{}` to `{}`",
                pdf_path.display(),
                asset_path.display()
            )
        })?;
        PersistentNoteBuilder::new()
            .with_config(self.persistence_config())
            .with_metadata(metadata)
            .with_pdf_file_name(pdf_file_name.to_string_lossy())
            .build()?
            .write_markdown()
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

    pub fn persistence_config(&self) -> PersistenceConfig {
        PersistenceConfig {
            project_root: self.project_root.clone(),
            library_path: self.config.library_path.clone(),
            assets_path: self.config.assets_path.clone(),
        }
    }
}

fn build_asset_id(metadata: &PaperMetadata) -> String {
    let mut hasher = Sha256::new();
    hasher.update(metadata.title.as_bytes());
    hasher.update(b"\n");

    for author in &metadata.author {
        hasher.update(author.as_bytes());
        hasher.update(b"\n");
    }

    format!("{:x}", hasher.finalize())
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

fn find_project_root(start: &Path) -> Result<PathBuf> {
    let mut current = if start.is_file() {
        start
            .parent()
            .context("failed to resolve parent directory for file path")?
            .to_path_buf()
    } else {
        start.to_path_buf()
    };

    loop {
        let config_path = current.join(".paperout").join("poutconfig.toml");
        if config_path.exists() {
            return Ok(current);
        }

        if !current.pop() {
            bail!(
                "project is not initialized: no `.paperout/poutconfig.toml` found from `{}` upward",
                start.display()
            );
        }
    }
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            library_path: PathBuf::from(DEFAULT_LIBRARY_DIR),
            assets_path: PathBuf::from(DEFAULT_ASSETS_DIR),
        }
    }
}

impl ProjectConfig {
    fn from_toml(config_text: &str) -> Result<Self> {
        let value: Value =
            toml::from_str(config_text).context("failed to parse `.paperout/poutconfig.toml`")?;
        let table = value
            .as_table()
            .context("project config must be a TOML table")?;

        Ok(Self {
            library_path: table_path(table, "library_path")?,
            assets_path: table_path(table, "assets_path")?,
        })
    }

    fn to_toml(&self) -> String {
        format!(
            "library_path = \"{}\"\nassets_path = \"{}\"\n",
            self.library_path.display(),
            self.assets_path.display()
        )
    }

    fn ensure_layout(&self, project_root: &Path) -> Result<()> {
        let library_dir = project_root.join(&self.library_path);
        let assets_dir = project_root.join(&self.assets_path);

        fs::create_dir_all(&library_dir)
            .with_context(|| format!("failed to create `{}`", library_dir.display()))?;
        fs::create_dir_all(&assets_dir)
            .with_context(|| format!("failed to create `{}`", assets_dir.display()))?;

        Ok(())
    }
}

fn table_path(table: &toml::Table, key: &str) -> Result<PathBuf> {
    let value = table
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("missing or invalid `{key}` in project config"))?;
    Ok(PathBuf::from(value))
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

#[cfg(test)]
mod tests {
    use super::find_project_root;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct TestWorkspace {
        root: PathBuf,
    }

    impl TestWorkspace {
        fn new(name: &str) -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time should move forward")
                .as_nanos();

            Self {
                root: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("test")
                    .join(format!("{name}-{stamp}")),
            }
        }
    }

    impl Drop for TestWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn finds_project_root_in_parent_directories() {
        let test_workspace = TestWorkspace::new("find-root");
        let nested_dir = test_workspace.root.join("a").join("b").join("c");
        let config_dir = test_workspace.root.join(".paperout");

        fs::create_dir_all(&nested_dir).expect("nested dir should be created");
        fs::create_dir_all(&config_dir).expect("config dir should be created");
        fs::write(
            config_dir.join("poutconfig.toml"),
            "library_path = \"Library\"\nassets_path = \"Assets\"\n",
        )
        .expect("config should be written");

        let root = find_project_root(&nested_dir).expect("should find parent project root");
        assert_eq!(root, test_workspace.root);
    }
}
