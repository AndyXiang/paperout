use anyhow::{Result, bail};

use crate::pdf::extract::{ExtractedPaper, extract_abstract_text, parse_authors};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaperMetadata {
    pub asset_id: String,
    pub title: String,
    pub author: Vec<String>,
    pub abstract_text: String,
    pub page_count: usize,
}

impl PaperMetadata {
    pub fn from_extracted(paper: &ExtractedPaper) -> Result<Self> {
        let title = paper
            .title_block()
            .map(|block| collapse_inline_whitespace(&block.text))
            .ok_or_else(|| anyhow::anyhow!("failed to extract title"))?;
        let authors = paper
            .author_block()
            .map(|block| parse_authors(&block.text))
            .unwrap_or_default();
        let abstract_text = paper
            .abstract_block()
            .and_then(|block| extract_abstract_text(&paper.blocks, block.index))
            .ok_or_else(|| anyhow::anyhow!("failed to extract abstract"))?;

        if authors.is_empty() {
            bail!("failed to extract author");
        }

        Ok(Self {
            asset_id: String::new(),
            title,
            author: authors,
            abstract_text,
            page_count: paper.page_count,
        })
    }

    pub fn to_markdown(&self, pdf_link: &str) -> String {
        let mut out = String::new();
        out.push_str("---\n");
        out.push_str(&format!(
            "asset_id: \"{}\"\n",
            escape_yaml_string(&self.asset_id)
        ));
        out.push_str(&format!("title: \"{}\"\n", escape_yaml_string(&self.title)));
        out.push_str(&format!("page_count: {}\n", self.page_count));
        out.push_str("author:\n");
        for author in &self.author {
            out.push_str(&format!("  - \"{}\"\n", escape_yaml_string(author)));
        }
        out.push_str("---\n\n");
        out.push_str(&format!(
            "[[{}|{}]]\n",
            pdf_link,
            escape_yaml_string(&self.title)
        ));
        out.push_str("\n## Abstract\n\n");
        out.push_str(&self.abstract_text);
        out.push('\n');
        out
    }
}

fn escape_yaml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn collapse_inline_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::PaperMetadata;
    use crate::pdf::extract::{BlockKind, ExtractedPaper, TextBlock};

    #[test]
    fn normalizes_title_to_single_line() {
        let paper = ExtractedPaper {
            page_count: 3,
            raw_text: String::new(),
            blocks: vec![
                TextBlock {
                    index: 0,
                    text: "Paper Title Line 1\nLine 2".to_string(),
                    kind: BlockKind::TitleCandidate,
                },
                TextBlock {
                    index: 1,
                    text: "Alice Bob".to_string(),
                    kind: BlockKind::AuthorCandidate,
                },
                TextBlock {
                    index: 2,
                    text: "Abstract".to_string(),
                    kind: BlockKind::AbstractCandidate,
                },
                TextBlock {
                    index: 3,
                    text: "Short abstract.".to_string(),
                    kind: BlockKind::Body,
                },
            ],
            body: Vec::new(),
            references: Vec::new(),
        };
        let metadata = PaperMetadata::from_extracted(&paper).expect("metadata should parse");

        assert!(metadata.asset_id.is_empty());
        assert_eq!(metadata.title, "Paper Title Line 1 Line 2");
        assert_eq!(metadata.page_count, 3);
    }
}
