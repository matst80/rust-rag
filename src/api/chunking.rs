/// Splits a document into overlapping chunks for contextual RAG indexing.
///
/// Each chunk stores the clean segment text, but the embedding is computed on
/// `overlap_tail_of_previous_chunk + chunk_text`. This makes boundary chunks
/// semantically aware of surrounding content without polluting the stored text.
pub struct ChunkSlice {
    /// The text stored in the DB and returned in search results.
    pub text: String,
    /// The text fed to the embedder (context prefix + stored text).
    pub embed_text: String,
}

/// Split `text` into contextual chunks.
///
/// Returns a single-element vec when the text fits within `max_chars`, so
/// callers don't need a separate "should I chunk?" check.
pub fn chunk_document(text: &str, max_chars: usize, overlap_chars: usize) -> Vec<ChunkSlice> {
    if text.len() <= max_chars {
        return vec![ChunkSlice {
            text: text.to_owned(),
            embed_text: text.to_owned(),
        }];
    }

    let segments = collect_segments(text, max_chars);
    let raw_chunks = aggregate_chunks(segments, max_chars);

    raw_chunks
        .iter()
        .enumerate()
        .map(|(i, chunk_text)| {
            let context_prefix = if i > 0 {
                tail_str(&raw_chunks[i - 1], overlap_chars)
            } else {
                String::new()
            };
            let embed_text = if context_prefix.is_empty() {
                chunk_text.clone()
            } else {
                format!("{}\n{}", context_prefix, chunk_text)
            };
            ChunkSlice {
                text: chunk_text.clone(),
                embed_text,
            }
        })
        .collect()
}

/// Split text at paragraph and sentence boundaries into segments each ≤ max_chars.
fn collect_segments(text: &str, max_chars: usize) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split("\n\n") {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        if para.len() <= max_chars {
            out.push(para.to_owned());
        } else {
            split_by_sentences(para, max_chars, &mut out);
        }
    }
    out
}

fn split_by_sentences(text: &str, max_chars: usize, out: &mut Vec<String>) {
    let bytes = text.as_bytes();
    let len = text.len();
    let mut start = 0;

    let mut i = 0;
    while i < len {
        let b = bytes[i];
        if (b == b'.' || b == b'!' || b == b'?')
            && i + 1 < len
            && (bytes[i + 1] == b' ' || bytes[i + 1] == b'\n')
        {
            let boundary = i + 2;
            if boundary - start >= max_chars {
                let seg = text[start..boundary].trim().to_owned();
                if !seg.is_empty() {
                    out.push(seg);
                }
                start = boundary;
            }
        }
        i += 1;
    }

    let rest = text[start..].trim();
    if rest.is_empty() {
        return;
    }
    if rest.len() <= max_chars {
        out.push(rest.to_owned());
    } else {
        force_split(rest, max_chars, out);
    }
}

fn force_split(text: &str, max_chars: usize, out: &mut Vec<String>) {
    let mut pos = 0;
    let len = text.len();
    while pos < len {
        let end = (pos + max_chars).min(len);
        let split_at = if end < len {
            text[pos..end]
                .rfind(|c: char| c.is_whitespace())
                .map(|i| pos + i + 1)
                .unwrap_or(end)
        } else {
            end
        };
        // Guard against zero-progress (no whitespace found in window).
        let split_at = if split_at == pos { end } else { split_at };
        let seg = text[pos..split_at].trim().to_owned();
        if !seg.is_empty() {
            out.push(seg);
        }
        pos = split_at;
    }
}

/// Accumulate segments into chunks, merging adjacent ones that fit within max_chars.
fn aggregate_chunks(segments: Vec<String>, max_chars: usize) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    for seg in segments {
        if current.is_empty() {
            current = seg;
        } else if current.len() + 1 + seg.len() <= max_chars {
            current.push('\n');
            current.push_str(&seg);
        } else {
            chunks.push(std::mem::take(&mut current));
            current = seg;
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// Return the last `max_chars` characters of `s`, starting at a word boundary.
fn tail_str(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        return s.to_owned();
    }
    let raw_start = s.len() - max_chars;
    // Advance to a valid char boundary.
    let byte_start = (raw_start..=s.len())
        .find(|&i| s.is_char_boundary(i))
        .unwrap_or(s.len());
    // Then advance to the next word boundary so we don't start mid-word.
    let word_start = s[byte_start..]
        .find(|c: char| c.is_whitespace())
        .map(|i| byte_start + i + 1)
        .unwrap_or(byte_start);
    s[word_start..].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_returns_single_chunk() {
        let slices = chunk_document("hello world", 1024, 200);
        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].text, "hello world");
        assert_eq!(slices[0].embed_text, "hello world");
    }

    #[test]
    fn long_text_splits_into_multiple_chunks() {
        let para = "word ".repeat(50); // 250 chars
        let text = format!("{para}\n\n{para}\n\n{para}");
        let slices = chunk_document(&text, 300, 50);
        assert!(slices.len() >= 2);
        // Every chunk should be non-empty.
        for s in &slices {
            assert!(!s.text.is_empty());
        }
    }

    #[test]
    fn second_chunk_embed_text_has_context_prefix() {
        let para = "word ".repeat(50);
        let text = format!("{para}\n\n{para}");
        let slices = chunk_document(&text, 300, 50);
        if slices.len() >= 2 {
            assert!(slices[1].embed_text.len() > slices[1].text.len());
        }
    }

    #[test]
    fn first_chunk_embed_text_matches_text() {
        let para = "word ".repeat(50);
        let text = format!("{para}\n\n{para}");
        let slices = chunk_document(&text, 300, 50);
        assert_eq!(slices[0].text, slices[0].embed_text);
    }
}
