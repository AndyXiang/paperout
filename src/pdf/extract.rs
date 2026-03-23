use anyhow::{Context, Result};
use pdf_oxide::PdfDocument;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    TitleCandidate,
    AuthorCandidate,
    AbstractCandidate,
    Reference,
    Body,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextBlock {
    pub index: usize,
    pub text: String,
    pub kind: BlockKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedPaper {
    pub page_count: usize,
    pub raw_text: String,
    pub blocks: Vec<TextBlock>,
    pub body: Vec<TextBlock>,
    pub references: Vec<TextBlock>,
}

impl ExtractedPaper {
    /// Build an extracted paper from raw PDF bytes.
    pub fn from_pdf_bytes(file_buf: Vec<u8>) -> Result<Self> {
        let (page_count, raw_text) = Self::extract_text_from_pdf(file_buf)?;
        let raw_text = Self::normalize_text(raw_text);
        let mut paper = Self {
            page_count,
            raw_text,
            blocks: Vec::new(),
            body: Vec::new(),
            references: Vec::new(),
        };

        paper.split_into_blocks();
        paper.classify_blocks();
        paper.partition_sections();

        Ok(paper)
    }

    /// Extract plain text from a PDF byte buffer.
    fn extract_text_from_pdf(file_buf: Vec<u8>) -> Result<(usize, String)> {
        let mut document =
            PdfDocument::from_bytes(file_buf).context("failed to open PDF bytes with pdf_oxide")?;
        let page_count = document.page_count()?;
        let mut output = String::new();

        for page_index in 0..page_count {
            let page_text = document
                .extract_text(page_index)
                .with_context(|| format!("failed to extract text from page {}", page_index + 1))?;
            output.push_str(&page_text);
            output.push('\n');
        }

        Ok((page_count, output))
    }

    /// Normalize line endings before further parsing.
    fn normalize_text(text: String) -> String {
        text.replace("\r\n", "\n").replace('\r', "\n")
    }

    /// Split the extracted text into paragraph-like blocks.
    fn split_into_blocks(&mut self) {
        let block_regex = Regex::new(r"\n\s*\n").expect("block regex should compile");

        self.blocks = block_regex
            .split(&self.raw_text)
            .map(str::trim)
            .filter(|block| !block.is_empty())
            .enumerate()
            .map(|(index, text)| TextBlock {
                index,
                text: text.to_string(),
                kind: BlockKind::Unknown,
            })
            .collect();
    }

    /// Assign a coarse semantic kind to each text block.
    fn classify_blocks(&mut self) {
        let abstract_index = self
            .blocks
            .iter()
            .position(|block| is_abstract_block(&block.text));
        let reference_index = self
            .blocks
            .iter()
            .position(|block| is_reference_heading(&block.text));

        for block in &mut self.blocks {
            block.kind = classify_block(&block.text, block.index, abstract_index, reference_index);
        }
    }

    /// Partition classified blocks into body and reference sections.
    fn partition_sections(&mut self) {
        self.body = self
            .blocks
            .iter()
            .filter(|block| block.kind != BlockKind::Reference)
            .cloned()
            .collect();
        self.references = self
            .blocks
            .iter()
            .filter(|block| block.kind == BlockKind::Reference)
            .cloned()
            .collect();
    }

    pub fn title_block(&self) -> Option<&TextBlock> {
        self.blocks
            .iter()
            .find(|block| block.kind == BlockKind::TitleCandidate)
    }

    pub fn author_block(&self) -> Option<&TextBlock> {
        self.blocks
            .iter()
            .find(|block| block.kind == BlockKind::AuthorCandidate)
    }

    pub fn abstract_block(&self) -> Option<&TextBlock> {
        self.blocks
            .iter()
            .find(|block| block.kind == BlockKind::AbstractCandidate)
    }
}

fn classify_block(
    text: &str,
    index: usize,
    abstract_index: Option<usize>,
    reference_index: Option<usize>,
) -> BlockKind {
    if reference_index.is_some_and(|start| index >= start) {
        return BlockKind::Reference;
    }

    if is_abstract_block(text) {
        return BlockKind::AbstractCandidate;
    }

    if abstract_index.is_some_and(|abstract_at| index < abstract_at) {
        if looks_like_author_block(text) {
            return BlockKind::AuthorCandidate;
        }

        if looks_like_title_block(text) {
            return BlockKind::TitleCandidate;
        }
    }

    if abstract_index.is_some_and(|abstract_at| index > abstract_at) {
        return BlockKind::Body;
    }

    BlockKind::Unknown
}

pub fn parse_authors(block: &str) -> Vec<String> {
    let email_regex =
        Regex::new(r"\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b").expect("email regex");
    let affiliation_keywords = [
        "university",
        "laboratory",
        "department",
        "school",
        "institute",
        "college",
        "faculty",
        "centre",
        "center",
    ];

    block
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !email_regex.is_match(&line.to_ascii_uppercase()))
        .map(|line| line.trim_start_matches('†').trim())
        .filter(|line| !line.is_empty())
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            !affiliation_keywords
                .iter()
                .any(|keyword| lower.contains(keyword))
        })
        .flat_map(split_author_line)
        .map(clean_author)
        .filter(|candidate| looks_like_author(candidate))
        .collect()
}

pub fn extract_abstract_text(blocks: &[TextBlock], index: usize) -> Option<String> {
    let trimmed = blocks.get(index)?.text.trim();
    let lower = trimmed.to_ascii_lowercase();

    if lower == "abstract" {
        return blocks
            .get(index + 1)
            .map(|block| block.text.trim().to_string())
            .filter(|value| !value.is_empty());
    }

    if lower.starts_with("abstract.") {
        return Some(trimmed["Abstract.".len()..].trim().to_string());
    }

    if lower.starts_with("abstract ") {
        return Some(trimmed["Abstract".len()..].trim().to_string());
    }

    None
}

fn split_author_line(line: &str) -> Vec<String> {
    line.split(" and ")
        .flat_map(split_author_candidates)
        .collect()
}

fn split_author_candidates(line: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut current = String::new();

    for ch in line.chars() {
        if !current.is_empty()
            && ch.is_uppercase()
            && current
                .chars()
                .last()
                .is_some_and(|last| last.is_lowercase())
        {
            candidates.push(current.trim().to_string());
            current.clear();
        }

        current.push(ch);
    }

    if !current.trim().is_empty() {
        candidates.push(current.trim().to_string());
    }

    candidates
}

fn clean_author(value: String) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn looks_like_author(value: &str) -> bool {
    let words: Vec<_> = value.split_whitespace().collect();
    if words.len() < 2 || words.len() > 4 {
        return false;
    }

    words.iter().all(|word| {
        word.chars()
            .next()
            .map(|ch| ch.is_uppercase())
            .unwrap_or(false)
    })
}

fn looks_like_title_block(block: &str) -> bool {
    let trimmed = block.trim();
    let lower = trimmed.to_ascii_lowercase();

    !trimmed.is_empty()
        && !trimmed.contains('@')
        && !lower.contains("department")
        && !lower.contains("university")
        && !lower.contains("abstract")
        && trimmed.split_whitespace().count() >= 4
}

fn looks_like_author_block(block: &str) -> bool {
    !parse_authors(block).is_empty()
}

fn is_abstract_block(block: &str) -> bool {
    let trimmed = block.trim();
    trimmed.eq_ignore_ascii_case("abstract")
        || trimmed.to_ascii_lowercase().starts_with("abstract.")
        || trimmed.to_ascii_lowercase().starts_with("abstract ")
}

fn is_reference_heading(block: &str) -> bool {
    let trimmed = block.trim().to_ascii_lowercase();
    trimmed == "references" || trimmed == "bibliography"
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::{BlockKind, ExtractedPaper, extract_abstract_text, parse_authors};

    fn sample_pdf_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test")
            .join("2211.15348v1.pdf")
    }

    fn second_sample_pdf_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test")
            .join("2403.04482v2.pdf")
    }

    #[test]
    fn extracts_text_from_test_pdf() {
        let path = sample_pdf_path();
        let file_buf = fs::read(&path).expect("should read test PDF");
        let paper =
            ExtractedPaper::from_pdf_bytes(file_buf).expect("should extract paper from test PDF");

        assert!(paper.page_count > 0, "page count should be detected");
        assert!(!paper.raw_text.is_empty(), "raw text should not be empty");

        let preview: String = paper.raw_text.chars().take(1200).collect();
        println!("{preview}");
    }

    #[test]
    fn classifies_title_author_and_abstract_for_first_pdf() {
        let path = sample_pdf_path();
        let file_buf = fs::read(&path).expect("should read test PDF");
        let paper =
            ExtractedPaper::from_pdf_bytes(file_buf).expect("should extract paper from test PDF");

        assert_eq!(
            paper.title_block().map(|block| block.kind.clone()),
            Some(BlockKind::TitleCandidate)
        );
        assert_eq!(
            paper.author_block().map(|block| block.kind.clone()),
            Some(BlockKind::AuthorCandidate)
        );
        assert_eq!(
            paper.abstract_block().map(|block| block.kind.clone()),
            Some(BlockKind::AbstractCandidate)
        );
        assert!(
            extract_abstract_text(&paper.blocks, paper.abstract_block().unwrap().index).is_some()
        );
    }

    #[test]
    fn classifies_title_author_and_abstract_for_second_pdf() {
        let path = second_sample_pdf_path();
        let file_buf = fs::read(&path).expect("should read second test PDF");
        let paper = ExtractedPaper::from_pdf_bytes(file_buf)
            .expect("should extract paper from second test PDF");

        assert_eq!(
            paper.title_block().map(|block| block.text.as_str()),
            Some(
                "On the Topology Awareness and Generalization\nPerformance of Graph Neural Networks"
            )
        );
        assert_eq!(
            paper
                .author_block()
                .map(|block| parse_authors(&block.text))
                .unwrap(),
            vec!["Junwei Su", "Chuan Wu"]
        );
        assert!(
            extract_abstract_text(&paper.blocks, paper.abstract_block().unwrap().index).is_some()
        );
    }
}
