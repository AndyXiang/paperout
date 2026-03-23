mod cli;
mod command;
mod pdf;

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
            let project_root = match repo {
                Some(path) => path,
                None => std::env::current_dir().context("failed to get current directory")?,
            };
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
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn unique_test_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();

        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test")
            .join(format!("{name}-{stamp}"))
    }

    fn sample_pdf_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test")
            .join("2211.15348v1.pdf")
    }

    #[test]
    fn init_creates_project_layout() {
        let root = unique_test_dir("init-project");
        let workspace = ProjectWorkspace::init(&root).expect("init should succeed");

        assert!(root.exists());
        assert!(workspace.config_dir.exists());
        assert!(workspace.config_path.exists());
        assert_eq!(workspace.config_text, "");

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[test]
    fn add_creates_markdown_from_test_pdf() {
        let root = unique_test_dir("add-project");
        let pdf_path = sample_pdf_path();

        let workspace = ProjectWorkspace::init(&root).expect("init should succeed");
        let markdown_path = workspace.add_paper(&pdf_path).expect("add should succeed");
        let markdown = fs::read_to_string(&markdown_path).expect("markdown should exist");

        assert!(markdown.starts_with("---\n"));
        assert!(
            markdown.contains("title: \"Learning Feynman Diagrams using Graph Neural Networks\"")
        );
        assert!(markdown.contains("page_count: 10"));
        assert!(markdown.contains("  - \"Pietro Liò\""));
        assert!(markdown.contains("abstract: |"));
        assert!(markdown.contains("# Learning Feynman Diagrams using Graph Neural Networks"));

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[test]
    fn add_accepts_multiple_pdf_inputs() {
        let root = unique_test_dir("add-multi-project");
        let inbox = root.join("inbox");
        let source_pdf = sample_pdf_path();

        fs::create_dir_all(&inbox).expect("inbox should be created");
        fs::copy(&source_pdf, inbox.join("one.pdf")).expect("first copy should succeed");
        fs::copy(&source_pdf, inbox.join("two.pdf")).expect("second copy should succeed");

        let workspace = ProjectWorkspace::init(&root).expect("init should succeed");
        let markdown_paths = workspace
            .add_inputs(&[inbox.join("one.pdf"), inbox.join("two.pdf")], false)
            .expect("add should succeed");

        assert_eq!(markdown_paths.len(), 2);
        assert!(
            root.join("learning-feynman-diagrams-using-graph-neural-networks.md")
                .exists()
        );

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[test]
    fn add_accepts_directory_and_recursive_scan() {
        let root = unique_test_dir("add-dir-project");
        let inbox = root.join("inbox");
        let nested = inbox.join("nested");
        let source_pdf = sample_pdf_path();

        fs::create_dir_all(&nested).expect("nested inbox should be created");
        fs::copy(&source_pdf, inbox.join("top.pdf")).expect("top copy should succeed");
        fs::copy(&source_pdf, nested.join("deep.pdf")).expect("nested copy should succeed");

        let workspace = ProjectWorkspace::init(&root).expect("init should succeed");

        let shallow_paths = workspace
            .add_inputs(std::slice::from_ref(&inbox), false)
            .expect("shallow add should succeed");
        assert_eq!(shallow_paths.len(), 1);

        let recursive_paths = workspace
            .add_inputs(&[inbox], true)
            .expect("recursive add should succeed");
        assert_eq!(recursive_paths.len(), 2);

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }
}
