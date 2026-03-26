mod cli;
mod command;
mod pdf;
mod persist;

use anyhow::{Context, Result};

use crate::{cli::CliCommand, command::ProjectWorkspace};

fn main() -> Result<()> {
    match cli::parse()? {
        CliCommand::Init { path } => {
            ProjectWorkspace::init(&path)?;
        }
        CliCommand::Add {
            inputs,
            repo,
            recursive,
        } => {
            let project_root =
                repo.unwrap_or(std::env::current_dir().context("failed to get current directory")?);
            let workspace = ProjectWorkspace::load(&project_root)?;
            workspace.add_inputs(&inputs, recursive)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::command::ProjectWorkspace;
    use std::{
        fs,
        path::{Path, PathBuf},
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

        fn path(&self) -> &Path {
            &self.root
        }
    }

    impl Drop for TestWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn sample_pdf_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test")
            .join("2211.15348v1.pdf")
    }

    #[test]
    fn init_creates_project_layout() {
        let test_workspace = TestWorkspace::new("init-project");
        let workspace = ProjectWorkspace::init(test_workspace.path()).expect("init should succeed");
        let config_path = test_workspace
            .path()
            .join(".paperout")
            .join("poutconfig.toml");
        let config_text = fs::read_to_string(&config_path).expect("config should exist");

        assert!(test_workspace.path().exists());
        assert_eq!(workspace.project_root, test_workspace.path());
        assert!(config_path.exists());
        assert!(test_workspace.path().join("Library").exists());
        assert!(test_workspace.path().join("Assets").exists());
        assert!(config_text.contains("library_path = \"Library\""));
        assert!(config_text.contains("assets_path = \"Assets\""));
    }

    #[test]
    fn init_preserves_existing_config() {
        let test_workspace = TestWorkspace::new("init-existing-project");
        let config_dir = test_workspace.path().join(".paperout");
        let config_path = config_dir.join("poutconfig.toml");

        fs::create_dir_all(&config_dir).expect("config dir should be created");
        fs::write(
            &config_path,
            "library_path = \"Notes\"\nassets_path = \"Files\"\n",
        )
        .expect("config should be written");

        let workspace = ProjectWorkspace::init(test_workspace.path()).expect("init should succeed");

        assert_eq!(workspace.config.library_path, PathBuf::from("Notes"));
        assert_eq!(workspace.config.assets_path, PathBuf::from("Files"));
        assert!(test_workspace.path().join("Notes").exists());
        assert!(test_workspace.path().join("Files").exists());
    }

    #[test]
    fn add_creates_markdown_from_test_pdf() {
        let test_workspace = TestWorkspace::new("add-project");
        let pdf_path = sample_pdf_path();

        let workspace = ProjectWorkspace::init(test_workspace.path()).expect("init should succeed");
        let markdown_path = workspace.add_paper(&pdf_path).expect("add should succeed");
        let markdown = fs::read_to_string(&markdown_path).expect("markdown should exist");
        let asset_id = markdown
            .lines()
            .find_map(|line| line.strip_prefix("asset_id: \""))
            .and_then(|line| line.strip_suffix('"'))
            .expect("markdown should include asset_id");
        let copied_pdf = test_workspace
            .path()
            .join("Assets")
            .join(asset_id)
            .join("2211.15348v1.pdf");

        assert!(markdown.starts_with("---\n"));
        assert!(markdown.contains("asset_id: \""));
        assert!(
            markdown.contains("title: \"Learning Feynman Diagrams using Graph Neural Networks\"")
        );
        assert!(markdown.contains("page_count: 10"));
        assert!(markdown.contains("  - \"Pietro Liò\""));
        assert!(markdown.contains("# Learning Feynman Diagrams using Graph Neural Networks"));
        assert!(markdown.contains("## Abstract"));
        assert!(markdown.contains(&format!(
            "[[Assets/{asset_id}/2211.15348v1.pdf|Learning Feynman Diagrams using Graph Neural Networks]]"
        )));
        assert!(markdown_path.starts_with(test_workspace.path().join("Library")));
        let expected_file_name = format!("{asset_id}.md");
        assert_eq!(
            markdown_path.file_name().and_then(|name| name.to_str()),
            Some(expected_file_name.as_str())
        );
        assert!(copied_pdf.exists());
    }

    #[test]
    fn add_accepts_multiple_pdf_inputs() {
        let test_workspace = TestWorkspace::new("add-multi-project");
        let inbox = test_workspace.path().join("inbox");
        let source_pdf = sample_pdf_path();

        fs::create_dir_all(&inbox).expect("inbox should be created");
        fs::copy(&source_pdf, inbox.join("one.pdf")).expect("first copy should succeed");
        fs::copy(&source_pdf, inbox.join("two.pdf")).expect("second copy should succeed");

        let workspace = ProjectWorkspace::init(test_workspace.path()).expect("init should succeed");
        let markdown_paths = workspace
            .add_inputs(&[inbox.join("one.pdf"), inbox.join("two.pdf")], false)
            .expect("add should succeed");

        assert_eq!(markdown_paths.len(), 2);
        assert_eq!(markdown_paths[0], markdown_paths[1]);
        let markdown_files: Vec<_> = fs::read_dir(test_workspace.path().join("Library"))
            .expect("library dir should exist")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("md"))
            .collect();
        assert_eq!(markdown_files.len(), 1);
        let asset_dirs = fs::read_dir(test_workspace.path().join("Assets"))
            .expect("assets dir should exist")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .count();
        assert!(asset_dirs >= 1);
    }

    #[test]
    fn add_accepts_directory_and_recursive_scan() {
        let test_workspace = TestWorkspace::new("add-dir-project");
        let inbox = test_workspace.path().join("inbox");
        let nested = inbox.join("nested");
        let source_pdf = sample_pdf_path();

        fs::create_dir_all(&nested).expect("nested inbox should be created");
        fs::copy(&source_pdf, inbox.join("top.pdf")).expect("top copy should succeed");
        fs::copy(&source_pdf, nested.join("deep.pdf")).expect("nested copy should succeed");

        let workspace = ProjectWorkspace::init(test_workspace.path()).expect("init should succeed");

        let shallow_paths = workspace
            .add_inputs(std::slice::from_ref(&inbox), false)
            .expect("shallow add should succeed");
        assert_eq!(shallow_paths.len(), 1);

        let recursive_paths = workspace
            .add_inputs(&[inbox], true)
            .expect("recursive add should succeed");
        assert_eq!(recursive_paths.len(), 2);
        let asset_entries: Vec<_> = fs::read_dir(test_workspace.path().join("Assets"))
            .expect("assets dir should exist")
            .filter_map(|entry| entry.ok())
            .collect();
        assert!(!asset_entries.is_empty());
    }
}
