//! Token-aware, markdown-structure-aware chunking for parent/child retrieval.
//!
//! Uses `text-splitter`'s `MarkdownSplitter` configured with the embedder's
//! tokenizer (XLM-RoBERTa for bge-m3) so chunk size is measured in real
//! model tokens, not characters. Markdown structure (headings, code fences,
//! tables, lists, paragraphs) is respected — a chunk never breaks inside a
//! code block, and section boundaries are preferred over arbitrary cuts.
//!
//! Each emitted [`Chunk`] also carries a `section_path` — the stack of
//! markdown headers in effect at the chunk's start offset, e.g.
//! `["Phase 1 — Postgres + bge-m3 dense", "Header-aware chunking"]`. The
//! header walk is done once per source via `pulldown-cmark`; chunk offsets
//! are mapped onto it with a binary search. `section_path` lands on the
//! `chunks.section_path TEXT[]` column for parent-section reassembly at
//! retrieval time.

use anyhow::Result;
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use text_splitter::{ChunkConfig, MarkdownSplitter};
use tokenizers::Tokenizer;

#[derive(Debug, Clone)]
pub struct Chunk {
    pub position: i32,
    pub content: String,
    pub section_path: Vec<String>,
}

pub struct MarkdownChunker {
    splitter: MarkdownSplitter<Tokenizer>,
}

impl MarkdownChunker {
    /// `max_tokens` is the upper bound for any single chunk (in tokenizer
    /// tokens). `overlap_tokens` is added to the *end* of one chunk and the
    /// *start* of the next so retrieval doesn't lose context at boundaries.
    /// Per the migration plan: ~500 / ~50 for bge-m3 (8k context window).
    pub fn new(tokenizer: Tokenizer, max_tokens: usize, overlap_tokens: usize) -> Result<Self> {
        let config = ChunkConfig::new(max_tokens)
            .with_sizer(tokenizer)
            .with_overlap(overlap_tokens)?;
        Ok(Self {
            splitter: MarkdownSplitter::new(config),
        })
    }

    pub fn chunks(&self, text: &str) -> Vec<Chunk> {
        let headings = collect_headings(text);
        self.splitter
            .chunk_indices(text)
            .enumerate()
            .map(|(i, (byte_offset, content))| Chunk {
                position: i as i32,
                content: content.to_owned(),
                section_path: section_path_at(&headings, byte_offset),
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
struct Heading {
    byte_offset: usize,
    level: u8,
    title: String,
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn collect_headings(text: &str) -> Vec<Heading> {
    let parser = Parser::new(text).into_offset_iter();
    let mut out = Vec::new();
    let mut current: Option<(usize, u8, String)> = None;
    for (event, range) in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                current = Some((range.start, heading_level(level), String::new()));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((offset, level, title)) = current.take() {
                    let title = title.trim().to_owned();
                    if !title.is_empty() {
                        out.push(Heading { byte_offset: offset, level, title });
                    }
                }
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some((_, _, ref mut buf)) = current {
                    buf.push_str(&t);
                }
            }
            _ => {}
        }
    }
    out
}

fn section_path_at(headings: &[Heading], byte_offset: usize) -> Vec<String> {
    let mut stack: Vec<(u8, String)> = Vec::new();
    for h in headings {
        if h.byte_offset > byte_offset {
            break;
        }
        while stack.last().is_some_and(|(lvl, _)| *lvl >= h.level) {
            stack.pop();
        }
        stack.push((h.level, h.title.clone()));
    }
    stack.into_iter().map(|(_, t)| t).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ahash::AHashMap;
    use tokenizers::{models::wordlevel::WordLevel, pre_tokenizers::whitespace::Whitespace};

    fn whitespace_tokenizer() -> Tokenizer {
        let model = WordLevel::builder()
            .vocab(AHashMap::from([("[UNK]".to_owned(), 0)]))
            .unk_token("[UNK]".to_owned())
            .build()
            .unwrap();
        let mut tokenizer = Tokenizer::new(model);
        tokenizer.with_pre_tokenizer(Some(Whitespace::default()));
        tokenizer
    }

    #[test]
    fn short_text_yields_one_chunk() {
        let chunker = MarkdownChunker::new(whitespace_tokenizer(), 50, 5).unwrap();
        let chunks = chunker.chunks("Hello world. This is a short paragraph.");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].position, 0);
        assert!(chunks[0].section_path.is_empty());
    }

    #[test]
    fn long_text_yields_multiple_chunks() {
        let chunker = MarkdownChunker::new(whitespace_tokenizer(), 10, 2).unwrap();
        let text = "one two three four five six seven eight nine ten \
                    eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty \
                    alpha beta gamma delta epsilon zeta eta theta iota kappa";
        let chunks = chunker.chunks(text);
        assert!(chunks.len() >= 3, "got {} chunks", chunks.len());
        assert!(chunks.iter().enumerate().all(|(i, c)| c.position == i as i32));
    }

    #[test]
    fn section_path_tracks_heading_stack() {
        let chunker = MarkdownChunker::new(whitespace_tokenizer(), 8, 0).unwrap();
        let text = "\
# Top
intro words here for a bit
## Middle
some content under middle section
### Deep
deep content keeps going down
## Other
sibling at level two now wraps
";
        let chunks = chunker.chunks(text);
        assert!(chunks.len() >= 4, "got {} chunks", chunks.len());

        // First chunk is "# Top intro ...": the heading is *at* offset 0,
        // which equals the chunk offset, so it's already in scope.
        assert_eq!(chunks[0].section_path, vec!["Top".to_owned()]);

        // A chunk after `## Middle` should carry [Top, Middle].
        let after_middle = chunks
            .iter()
            .find(|c| c.content.contains("some content"))
            .expect("chunk under Middle");
        assert_eq!(
            after_middle.section_path,
            vec!["Top".to_owned(), "Middle".to_owned()]
        );

        // After `### Deep` we expect three levels.
        let after_deep = chunks
            .iter()
            .find(|c| c.content.contains("deep content"))
            .expect("chunk under Deep");
        assert_eq!(
            after_deep.section_path,
            vec!["Top".to_owned(), "Middle".to_owned(), "Deep".to_owned()]
        );

        // `## Other` pops Middle+Deep; we get [Top, Other], not [Top, Middle, Deep, Other].
        let after_other = chunks
            .iter()
            .find(|c| c.content.contains("sibling"))
            .expect("chunk under Other");
        assert_eq!(
            after_other.section_path,
            vec!["Top".to_owned(), "Other".to_owned()]
        );
    }
}
