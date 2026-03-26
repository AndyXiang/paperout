use std::{fs, path::PathBuf};

use anyhow::{Context, Result, bail};

use crate::pdf::metadata::PaperMetadata;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistenceConfig {
    pub project_root: PathBuf,
    pub library_path: PathBuf,
    pub assets_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentNote {
    pub metadata: PaperMetadata,
    pub config: PersistenceConfig,
    pub markdown_path: PathBuf,
    pub pdf_file_name: Option<String>,
}

#[derive(Default)]
pub struct PersistentNoteBuilder {
    metadata: Option<PaperMetadata>,
    config: Option<PersistenceConfig>,
    markdown_path: Option<PathBuf>,
    pdf_file_name: Option<String>,
}

impl PersistentNoteBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_metadata(mut self, metadata: PaperMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn with_config(mut self, config: PersistenceConfig) -> Self {
        self.config = Some(config);
        self
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn with_markdown_path(mut self, markdown_path: PathBuf) -> Self {
        self.markdown_path = Some(markdown_path);
        self
    }

    pub fn with_pdf_file_name(mut self, pdf_file_name: impl Into<String>) -> Self {
        self.pdf_file_name = Some(pdf_file_name.into());
        self
    }

    pub fn build(self) -> Result<PersistentNote> {
        match (self.metadata, self.markdown_path, self.config) {
            (Some(metadata), path, Some(config)) => {
                let markdown_path =
                    path.unwrap_or_else(|| default_markdown_path(&config, &metadata));

                Ok(PersistentNote {
                    metadata,
                    config,
                    markdown_path,
                    pdf_file_name: self.pdf_file_name,
                })
            }
            (None, Some(markdown_path), Some(config)) => {
                PersistentNote::read_markdown(markdown_path, config)
            }
            _ => bail!("persistent note builder requires either metadata+config or path+config"),
        }
    }
}

impl PersistentNote {
    pub fn write_markdown(&self) -> Result<PathBuf> {
        let parent = self
            .markdown_path
            .parent()
            .context("failed to resolve markdown parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create `{}`", parent.display()))?;

        fs::write(&self.markdown_path, self.to_markdown())
            .with_context(|| format!("failed to write `{}`", self.markdown_path.display()))?;

        Ok(self.markdown_path.clone())
    }

    fn read_markdown(markdown_path: PathBuf, config: PersistenceConfig) -> Result<Self> {
        let contents = fs::read_to_string(&markdown_path)
            .with_context(|| format!("failed to read `{}`", markdown_path.display()))?;
        let metadata = parse_markdown_metadata(&contents)?;
        let pdf_file_name = parse_pdf_file_name(&contents, &metadata.asset_id);

        Ok(Self {
            metadata,
            config,
            markdown_path,
            pdf_file_name,
        })
    }

    fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("---\n");
        out.push_str(&format!(
            "asset_id: \"{}\"\n",
            escape_yaml_string(&self.metadata.asset_id)
        ));
        out.push_str(&format!(
            "title: \"{}\"\n",
            escape_yaml_string(&self.metadata.title)
        ));
        out.push_str(&format!("page_count: {}\n", self.metadata.page_count));
        out.push_str("author:\n");
        for author in &self.metadata.author {
            out.push_str(&format!("  - \"{}\"\n", escape_yaml_string(author)));
        }
        out.push_str("---\n\n");
        out.push_str("# ");
        out.push_str(&self.metadata.title);
        out.push_str("\n\n");

        if let Some(pdf_link) = self.pdf_link() {
            out.push_str(&format!(
                "[[{}|{}]]\n\n",
                pdf_link,
                escape_yaml_string(&self.metadata.title)
            ));
        }

        out.push_str("## Abstract\n\n");
        out.push_str(&format_abstract_for_markdown(&self.metadata.abstract_text));
        out.push('\n');
        out
    }

    fn pdf_link(&self) -> Option<String> {
        let pdf_file_name = self.pdf_file_name.as_ref()?;
        Some(format!(
            "{}/{}/{}",
            self.config.assets_path.display(),
            self.metadata.asset_id,
            pdf_file_name
        ))
    }
}

fn default_markdown_path(config: &PersistenceConfig, metadata: &PaperMetadata) -> PathBuf {
    config
        .project_root
        .join(&config.library_path)
        .join(format!("{}.md", metadata.asset_id))
}

fn parse_markdown_metadata(contents: &str) -> Result<PaperMetadata> {
    let (frontmatter, body) = split_frontmatter(contents)?;
    let mut asset_id = None;
    let mut title = None;
    let mut page_count = None;
    let mut authors = Vec::new();
    let mut in_authors = false;

    for line in frontmatter.lines() {
        if let Some(value) = line.strip_prefix("asset_id: ") {
            asset_id = Some(parse_yaml_string(value)?);
            in_authors = false;
        } else if let Some(value) = line.strip_prefix("title: ") {
            title = Some(parse_yaml_string(value)?);
            in_authors = false;
        } else if let Some(value) = line.strip_prefix("page_count: ") {
            page_count = Some(value.trim().parse().context("invalid page_count value")?);
            in_authors = false;
        } else if line.trim() == "author:" {
            in_authors = true;
        } else if in_authors {
            if let Some(value) = line.trim().strip_prefix("- ") {
                authors.push(parse_yaml_string(value)?);
            }
        }
    }

    let abstract_text = parse_abstract_text(body)?;

    Ok(PaperMetadata {
        asset_id: asset_id.context("missing asset_id in markdown front matter")?,
        title: title.context("missing title in markdown front matter")?,
        author: authors,
        abstract_text,
        page_count: page_count.context("missing page_count in markdown front matter")?,
    })
}

fn split_frontmatter(contents: &str) -> Result<(&str, &str)> {
    let rest = contents
        .strip_prefix("---\n")
        .context("markdown is missing front matter start")?;
    let (frontmatter, body) = rest
        .split_once("\n---\n\n")
        .context("markdown is missing front matter end")?;
    Ok((frontmatter, body))
}

fn parse_yaml_string(value: &str) -> Result<String> {
    let trimmed = value.trim();
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .context("expected quoted YAML string")?;
    Ok(unquoted.replace("\\\"", "\"").replace("\\\\", "\\"))
}

fn parse_abstract_text(body: &str) -> Result<String> {
    let (_, abstract_section) = body
        .split_once("\n## Abstract\n\n")
        .context("markdown is missing abstract section")?;
    Ok(abstract_section.trim().to_string())
}

fn parse_pdf_file_name(contents: &str, asset_id: &str) -> Option<String> {
    let marker = format!("[[Assets/{asset_id}/");
    let start = contents.find(&marker)? + marker.len();
    let rest = &contents[start..];
    let end = rest.find('|')?;
    Some(rest[..end].to_string())
}

fn escape_yaml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn format_abstract_for_markdown(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut index = 0;

    while index < chars.len() {
        let ch = chars[index];

        if ch == '-'
            && index > 0
            && chars[index - 1].is_alphabetic()
            && chars.get(index + 1) == Some(&'\n')
            && chars
                .get(index + 2)
                .is_some_and(|next| next.is_alphabetic())
        {
            let prefix_start = out
                .char_indices()
                .rev()
                .find(|(_, current)| !current.is_alphabetic())
                .map(|(pos, current)| pos + current.len_utf8())
                .unwrap_or(0);
            let prefix = out[prefix_start..].to_string();
            index += 2;
            let suffix_start = index;

            while index < chars.len() && chars[index].is_alphabetic() {
                out.push(chars[index]);
                index += 1;
            }

            let suffix: String = chars[suffix_start..index].iter().collect();
            if !should_merge_without_separator(&prefix, &suffix) {
                let insert_at = out.len().saturating_sub(suffix.len());
                out.insert(insert_at, '-');
            }

            while index < chars.len() && matches!(chars[index], '.' | ',' | ';' | ':' | ')' | ']') {
                out.push(chars[index]);
                index += 1;
            }

            out.push('\n');

            while index < chars.len() && matches!(chars[index], ' ' | '\t') {
                index += 1;
            }

            if chars.get(index) == Some(&'\n') {
                index += 1;
            }

            continue;
        }

        out.push(ch);
        index += 1;
    }

    out
}

fn should_merge_without_separator(prefix: &str, suffix: &str) -> bool {
    let continuation_suffixes = [
        "ability",
        "able",
        "ably",
        "acy",
        "al",
        "ally",
        "ance",
        "ances",
        "ant",
        "ants",
        "ary",
        "ation",
        "ations",
        "ative",
        "atively",
        "ed",
        "ence",
        "ences",
        "ent",
        "ents",
        "er",
        "ers",
        "est",
        "fication",
        "fications",
        "ful",
        "fully",
        "ibility",
        "ible",
        "ic",
        "ical",
        "ically",
        "ics",
        "ified",
        "ifies",
        "ify",
        "ing",
        "ion",
        "ions",
        "isation",
        "isations",
        "ise",
        "ised",
        "ising",
        "ism",
        "ist",
        "ists",
        "ity",
        "ities",
        "ive",
        "ively",
        "ization",
        "izations",
        "zation",
        "zations",
        "ize",
        "ized",
        "izing",
        "less",
        "logy",
        "logical",
        "ment",
        "ments",
        "ness",
        "ous",
        "ously",
        "s",
        "ship",
        "ships",
        "tion",
        "tions",
        "ty",
    ];

    prefix.len() <= 1
        || continuation_suffixes
            .iter()
            .any(|candidate| suffix.eq_ignore_ascii_case(candidate))
}

#[cfg(test)]
mod tests {
    use super::{PersistenceConfig, PersistentNoteBuilder, format_abstract_for_markdown};
    use crate::pdf::metadata::PaperMetadata;
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

    fn sample_config(root: &std::path::Path) -> PersistenceConfig {
        PersistenceConfig {
            project_root: root.to_path_buf(),
            library_path: PathBuf::from("Library"),
            assets_path: PathBuf::from("Assets"),
        }
    }

    fn sample_metadata() -> PaperMetadata {
        PaperMetadata {
            asset_id: "abc123".to_string(),
            title: "Sample Paper".to_string(),
            author: vec!["Alice Example".to_string(), "Bob Example".to_string()],
            abstract_text: "Short abstract.".to_string(),
            page_count: 7,
        }
    }

    #[test]
    fn creates_default_markdown_path_from_asset_id() {
        let test_workspace = TestWorkspace::new("persistent-note-create");
        let note = PersistentNoteBuilder::new()
            .with_config(sample_config(&test_workspace.root))
            .with_metadata(sample_metadata())
            .with_pdf_file_name("paper.pdf")
            .build()
            .expect("note should build");

        assert_eq!(
            note.markdown_path,
            test_workspace.root.join("Library").join("abc123.md")
        );
    }

    #[test]
    fn reads_metadata_from_existing_markdown() {
        let test_workspace = TestWorkspace::new("persistent-note-read");
        let markdown_path = test_workspace.root.join("Library").join("abc123.md");
        fs::create_dir_all(markdown_path.parent().unwrap()).expect("library dir should exist");
        fs::write(
            &markdown_path,
            "---\nasset_id: \"abc123\"\ntitle: \"Sample Paper\"\npage_count: 7\nauthor:\n  - \"Alice Example\"\n  - \"Bob Example\"\n---\n\n# Sample Paper\n\n[[Assets/abc123/paper.pdf|Sample Paper]]\n\n## Abstract\n\nShort abstract.\n",
        )
        .expect("markdown should be written");

        let note = PersistentNoteBuilder::new()
            .with_config(sample_config(&test_workspace.root))
            .with_markdown_path(markdown_path.clone())
            .build()
            .expect("note should load");

        assert_eq!(note.metadata, sample_metadata());
        assert_eq!(note.pdf_file_name.as_deref(), Some("paper.pdf"));
        assert_eq!(note.markdown_path, markdown_path);
    }

    #[test]
    fn formats_hyphenated_abstract_line_breaks() {
        let formatted = format_abstract_for_markdown(
            "graph-\nstructured data and generali-\nzation,\nperformance.",
        );

        assert_eq!(
            formatted,
            "graph-structured\ndata and generalization,\nperformance."
        );
    }
}
