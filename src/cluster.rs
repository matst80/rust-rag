//! Deterministic corpus clustering for dreaming.
//!
//! This is stage 0 and stage 1 of the corpus-scale dreaming pipeline, and it
//! deliberately contains no model calls. Everything here is a pure function of
//! the corpus text plus a fixed set of knobs, so a round produces the same
//! clusters on every run and can be tested without an embedder or an LLM.
//!
//! Why this exists: the per-item dreaming loop asks a model "what are the five
//! nearest entries to this one?" and lets it decide. Measured against a real
//! 2489-item corpus that signal is noise — every pairwise distance lands in a
//! 0.44-0.54 band with no separation between related and unrelated entries, and
//! the five nearest neighbours of `Fyll vattenloopen` ("fill the watering
//! circuit") were `Byt overliggare mellan tomterna` ("swap the mattresses
//! between the plots"). Meanwhile the corpus is only ~0.57 MB in total, i.e.
//! small enough to reason about as a whole.
//!
//! So the corpus is first partitioned by cheap, high-signal evidence that does
//! not need a model, and only the leftovers are ever handed to one:
//!
//! 1. [`near_duplicates`] — word-shingle containment finds entries that are
//!    literally copies of each other.
//! 2. [`Corpus::cluster`] — sparse TF-IDF similarity restricted to *identifying*
//!    tokens (paths, snake_case, dotted names, digits), mutual-kNN filtered,
//!    then connected components.
//! 3. [`Corpus::partition`] — splits the result into clusters, singletons and a
//!    junk bucket, so a round never wastes model budget on entries that carry
//!    no recoverable meaning.
//!
//! Clusters are labelled deterministically from their highest-IDF members, which
//! gives a usable name (`web/src · internal/adapters · circuit`) without a model.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// A document reduced to what the clustering actually looks at.
#[derive(Debug, Clone)]
pub struct Doc {
    pub id: String,
    pub text: String,
}

/// Why an entry landed where it did. Surfaced in the round report so an
/// operator can tell "no neighbours" apart from "too short to mean anything".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    Cluster,
    Singleton,
    Junk,
}

/// An entry paired with the cluster it was assigned to.
#[derive(Debug, Clone)]
pub struct Assignment {
    pub id: String,
    pub cluster_id: String,
    pub bucket: Bucket,
}

/// A group of entries that share identifying vocabulary.
#[derive(Debug, Clone)]
pub struct Cluster {
    /// Stable across runs: a hash over the sorted member ids.
    pub id: String,
    /// Deterministic human label built from the highest-IDF member tokens.
    pub label: String,
    pub members: Vec<String>,
}

/// Two entries that are near-copies of each other.
#[derive(Debug, Clone)]
pub struct DuplicatePair {
    pub a: String,
    pub b: String,
    /// Fraction of the shorter entry's shingles present in the longer one.
    pub containment: f32,
}

/// Everything the deterministic pass produced for one corpus.
#[derive(Debug, Clone, Default)]
pub struct Partition {
    pub clusters: Vec<Cluster>,
    pub assignments: Vec<Assignment>,
    pub duplicates: Vec<DuplicatePair>,
    /// Entries too short / too generic to carry recoverable meaning.
    pub junk: Vec<String>,
    pub singletons: Vec<String>,
}

impl Partition {
    pub fn members_of(&self, cluster_id: &str) -> Vec<&str> {
        self.clusters
            .iter()
            .find(|c| c.id == cluster_id)
            .map(|c| c.members.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }

    /// Largest clusters first — the round consumes the big ones first because
    /// they hold the most redundancy.
    pub fn clusters_by_size(&self) -> Vec<&Cluster> {
        let mut out: Vec<&Cluster> = self.clusters.iter().collect();
        out.sort_by(|a, b| b.members.len().cmp(&a.members.len()).then(a.id.cmp(&b.id)));
        out
    }
}

/// Tunables. Defaults are calibrated for short (100-500 char) notes, which is
/// what agent session memory actually contains.
#[derive(Debug, Clone)]
pub struct ClusterParams {
    /// Minimum entries in a group before it is called a cluster rather than a
    /// pile of singletons.
    pub min_cluster_size: usize,
    /// Cosine similarity above which two entries are candidate neighbours.
    pub min_similarity: f32,
    /// Mutual-kNN width. An edge survives only if each side is in the other's
    /// top `knn` — this is what stops one generic entry from chaining the whole
    /// corpus into a single component.
    pub knn: usize,
    /// Entries with fewer identifying tokens than this are treated as junk.
    pub min_identifying_tokens: usize,
    /// Entries shorter than this (in chars) are junk regardless.
    pub min_chars: usize,
    /// Distinct identifying tokens required before two entries may share one.
    pub min_shared_tokens: usize,
    /// Minimum fraction of the smaller entry's identifying-token mass that the
    /// two entries share. A plain token count is not enough (two notes can
    /// share one moderately common identifier), and an absolute IDF sum is not
    /// either (IDF is corpus-size dependent, so a threshold that works on 2489
    /// entries rejects everything on 20). This ratio is scale-invariant.
    pub min_shared_ratio: f32,
    /// Drop any identifying token present in more than this fraction of the
    /// corpus *and* in at least `max_df_min_count` entries. These are hubs —
    /// without this a handful of generic identifiers chain unrelated notes into
    /// one component. The absolute floor matters as much as the ratio: on a
    /// 3-entry corpus every token exceeds a 10% ratio, which would leave the
    /// index empty.
    pub max_df_ratio: f32,
    pub max_df_min_count: usize,
    /// Components larger than this are re-partitioned with a stricter
    /// threshold; anything still oversized is reported as diffuse rather than
    /// being presented as a coherent cluster.
    pub max_cluster_size: usize,
    /// Number of times an oversized or incoherent component is re-split before
    /// giving up.
    pub max_split_rounds: usize,
    /// Minimum mean "best similarity to another member" for a group to count
    /// as a cluster. Connected components can chain: note A links to B and B
    /// links to C while A and C have nothing in common, and the union looks
    /// like one topic in a report but is meaningless as a unit of work. Every
    /// member must have at least one good partner.
    pub min_cohesion: f32,
    /// Shingle containment above which two entries are near-duplicates.
    pub dup_containment: f32,
    /// Shingle length in words.
    pub shingle_len: usize,
    /// Cap on entries considered in one pass, so a runaway source cannot pin
    /// the worker. Oldest-first, matching the dreaming cursor.
    pub max_docs: usize,
}

impl Default for ClusterParams {
    fn default() -> Self {
        Self {
            min_cluster_size: 3,
            min_similarity: 0.28,
            knn: 8,
            min_identifying_tokens: 2,
            min_chars: 40,
            min_shared_tokens: 2,
            min_shared_ratio: 0.35,
            max_df_ratio: 0.10,
            max_df_min_count: 8,
            max_cluster_size: 250,
            max_split_rounds: 3,
            min_cohesion: 0.45,
            dup_containment: 0.8,
            shingle_len: 5,
            max_docs: 5000,
        }
    }
}

/// Words that appear across unrelated notes and therefore identify nothing.
/// Deliberately small: an aggressive stop list is what turns a working
/// identifier scheme into mush.
const STOPWORDS: &[&str] = &[
    "add", "added", "after", "all", "also", "and", "any", "are", "back", "because", "been", "before",
    "being", "but", "can", "code", "could", "did", "does", "each", "file", "files", "for", "from",
    "get", "got", "had", "has", "have", "here", "how", "into", "its", "just", "like", "make", "many",
    "may", "more", "most", "need", "needs", "new", "not", "now", "one", "only", "or", "other",
    "over", "own", "same", "see", "set", "should", "since", "some", "still", "such", "than", "that",
    "the", "their", "them", "then", "there", "these", "they", "this", "those", "through", "to",
    "under", "use", "used", "uses", "using", "very", "was", "way", "we", "were", "what", "when",
    "where", "which", "while", "who", "why", "will", "with", "within", "work", "would", "you",
    "your",
];

fn is_stopword(tok: &str) -> bool {
    STOPWORDS.binary_search(&tok).is_ok() || STOPWORDS.contains(&tok)
}

/// Path prefixes that appear in half the corpus and therefore identify nothing.
/// `internal/...` and `web/src/...` coexist across unrelated repositories, so
/// keeping the bare prefix lets one note bridge every other note into a single
/// component — which is exactly what the first run against the real 2489-entry
/// corpus did (2172 of 2489 entries collapsed into one cluster).
const GENERIC_SEGMENTS: &[&str] = &[
    "app", "apps", "api", "bin", "cmd", "common", "core", "dist", "docs", "internal", "lib",
    "libs", "pkg", "pkgs", "public", "script", "scripts", "server", "service", "services", "src",
    "static", "store", "test", "tests", "tools", "types", "util", "utils", "web",
];

fn is_generic_segment(tok: &str) -> bool {
    GENERIC_SEGMENTS.binary_search(&tok).is_ok() || GENERIC_SEGMENTS.contains(&tok)
}

/// Split a token on the separators that show up inside identifiers so
/// `web/src/lib` yields `web`, `src`, `lib` *and* the whole path (which is the
/// part that actually identifies a repository).
fn split_identifier(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    if raw.len() >= 6 && (raw.contains('/') || raw.contains('.')) {
        // The full path is the strongest signal; keep it whole.
        out.push(raw.to_owned());
    }
    for part in raw.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
        if part.len() >= 4 && !is_generic_segment(part) {
            out.push(part.to_owned());
        }
    }
    out
}

/// Tokenize into plain words (weight-carrying) and identifying tokens
/// (presence-only). Identifying tokens are what the similarity is built on:
/// they are the rare, repo-specific strings that actually separate one
/// cluster from another.
fn tokenize(text: &str) -> (Vec<String>, HashSet<String>) {
    let mut words = Vec::new();
    let mut identifying = HashSet::new();
    for raw in text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '/' || c == '.' || c == '-')) {
        // Sentence punctuation rides along with the last word of a sentence
        // ("updates.", "needed.") and would otherwise become a distinct token.
        let lower = raw.trim_matches(|c| c == '.' || c == '-').to_ascii_lowercase();
        if lower.is_empty() {
            continue;
        }
        let shaped = lower.contains('_') || lower.contains('/') || lower.contains('.') || lower.contains('-');
        for tok in split_identifier(&lower) {
            if tok.len() < 3 || is_stopword(&tok) {
                continue;
            }
            // A digit or an inner separator marks it as an identifier rather
            // than prose ("pprof", "v2", "internal/circuit").
            let identifying_token = shaped
                || tok.chars().any(|c| c.is_ascii_digit())
                || (tok.len() >= 8 && tok.matches('_').count() == 0 && tok.contains('_'))
                || (tok.len() >= 10 && tok.chars().any(|c| c.is_uppercase()));
            if identifying_token {
                identifying.insert(tok);
            } else {
                words.push(tok);
            }
        }
    }
    (words, identifying)
}

/// Word tokens with the same normalization as [`shingles`], for the
/// sub-shingle fallback in [`near_duplicates`].
fn word_tokens(text: &str) -> BTreeSet<String> {
    dup_words(text).into_iter().collect()
}

fn dup_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '/' || *c == '.')
                .collect::<String>()
                .to_ascii_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// Word-shingles used for near-duplicate detection. Two entries written at
/// different times about the same thing share long runs of words even when
/// every heading differs — which is exactly the case shingle Jaccard misses
/// when it is computed on headings alone.
fn shingles(text: &str, n: usize) -> BTreeSet<String> {
    let words: Vec<String> = dup_words(text);
    if words.len() < n {
        return BTreeSet::new();
    }
    (0..=words.len() - n)
        .map(|i| words[i..i + n].join(" "))
        .collect()
}

/// The corpus, indexed once and reused for every deterministic step.
pub struct Corpus {
    ids: Vec<String>,
    /// Inverse document frequency over identifying tokens.
    idf: HashMap<String, f32>,
    /// Identifying tokens per doc, with term frequency saturation.
    vectors: Vec<BTreeMap<String, f32>>,
    /// Raw identifying-token sets, needed for the mutual-kNN filter.
    sets: Vec<HashSet<String>>,
    /// Summed IDF per doc — the denominator of the shared-vocabulary ratio.
    idf_mass: HashMap<usize, f32>,
    params: ClusterParams,
}

impl Corpus {
    /// Build the index. `docs` is truncated to `params.max_docs`.
    pub fn new(docs: &[Doc], params: ClusterParams) -> Self {
        let capped = docs.len().min(params.max_docs);
        let mut df: HashMap<String, usize> = HashMap::new();
        let mut sets: Vec<HashSet<String>> = Vec::with_capacity(capped);
        let mut ids: Vec<String> = Vec::with_capacity(capped);

        for doc in docs.iter().take(capped) {
            let (_, identifying) = tokenize(&doc.text);
            sets.push(identifying.clone());
            ids.push(doc.id.clone());
            for tok in &identifying {
                *df.entry(tok.clone()).or_default() += 1;
            }
        }

let n = sets.len().max(1) as f32;
        // A token is a hub only if it is both proportionally and absolutely
        // common. `max_count` is the proportional ceiling, applied only once
        // the absolute floor is cleared.
        let hub_floor = params.max_df_min_count as f32;
        let max_count = params.max_df_ratio * n;
        // Hub tokens (present in more than `max_df_ratio` of the corpus) are
        // discarded outright rather than merely down-weighted. They are the
        // bridges that chain unrelated notes into one component.
        let idf: HashMap<String, f32> = df
            .iter()
            .filter(|(_, count)| {
                let c = **count as f32;
                c < hub_floor || c <= max_count
            })
            .map(|(tok, count)| {
                let weight = (n / (1.0 + *count as f32)).ln().max(0.05);
                (tok.clone(), weight)
            })
            .collect();
        // Hub removal can leave a doc with nothing at all, when every token it
        // had was shared by the whole corpus. Those entries are genuinely
        // low-information and are routed to junk by the `min_identifying_
        // tokens` gate below — restoring their hub tokens instead just
        // re-bridges unrelated notes into one giant component, which is exactly
        // what an earlier attempt did (239 and 237-member blobs across
        // unrelated projects).
        for set in sets.iter_mut() {
            set.retain(|tok| idf.contains_key(tok));
        }

        let vectors: Vec<BTreeMap<String, f32>> = sets
            .iter()
            .map(|set| {
                let mut vec: BTreeMap<String, f32> = BTreeMap::new();
                for tok in set {
                    // Presence-only with saturation: a token repeated ten times
                    // in a note is not ten times more identifying.
                    *vec.entry(tok.clone()).or_insert(0.0) += idf.get(tok).copied().unwrap_or(0.05);
                }
                for value in vec.values_mut() {
                    *value = 1.0 + value.min(8.0).ln();
                }
                vec
            })
            .collect();

        let idf_mass = vectors
            .iter()
            .enumerate()
            .map(|(idx, vec): (usize, &BTreeMap<String, f32>)| {
                let mass = vec.keys().map(|t| idf.get(t).copied().unwrap_or(0.05)).sum::<f32>();
                (idx, mass.max(f32::EPSILON))
            })
            .collect();

        Self {
            ids,
            idf,
            vectors,
            sets,
            idf_mass,
            params,
        }
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn id_at(&self, idx: usize) -> &str {
        &self.ids[idx]
    }

    /// Identifying tokens kept for a doc, after hub removal. Exposed for
    /// diagnostics and tests.
    pub fn tokens_of(&self, idx: usize) -> Vec<String> {
        let mut out: Vec<String> = self.sets[idx].iter().cloned().collect();
        out.sort();
        out
    }

    /// (cosine, shared token count, shared-vocabulary ratio) for two docs.
    pub fn similarity_between(&self, a: usize, b: usize) -> (f32, usize, f32) {
        self.similarity(a, b)
    }

    /// Mean "best similarity to another member" — see `min_cohesion`.
    pub fn cohesion_of(&self, group: &[usize]) -> f32 {
        self.cohesion(group)
    }

    fn idf_of(&self, token: &str) -> f32 {
        self.idf.get(token).copied().unwrap_or(0.05)
    }

    /// Cosine similarity plus how much of the smaller entry's identifying
    /// vocabulary the two entries share.
    ///
    /// Both are needed: cosine alone can be satisfied by a single strong
    /// token, and the shared ratio says the two entries genuinely talk about
    /// the same rare vocabulary rather than coincidentally sharing one word.
    fn similarity(&self, a: usize, b: usize) -> (f32, usize, f32) {
        let (va, vb) = (&self.vectors[a], &self.vectors[b]);
        let (small, large) = if va.len() <= vb.len() { (va, vb) } else { (vb, va) };
        let mut dot = 0.0f32;
        let mut shared = 0usize;
        let mut shared_idf = 0.0f32;
        for (tok, weight) in small {
            if let Some(other) = large.get(tok) {
                dot += weight * other;
                shared += 1;
                shared_idf += self.idf_of(tok);
            }
        }
        let mass_a = self.idf_mass.get(&a).copied().unwrap_or(1.0);
        let mass_b = self.idf_mass.get(&b).copied().unwrap_or(1.0);
        let ratio = match mass_a.min(mass_b) {
            m if m <= 0.0 => 0.0,
            m => (shared_idf / m).clamp(0.0, 1.0),
        };
        if dot <= 0.0 {
            return (0.0, shared, ratio);
        }
        let na: f32 = va.values().map(|v| v * v).sum::<f32>().sqrt();
        let nb: f32 = vb.values().map(|v| v * v).sum::<f32>().sqrt();
        if na <= 0.0 || nb <= 0.0 {
            return (0.0, shared, ratio);
        }
        (dot / (na * nb).sqrt(), shared, ratio)
    }

    /// Groups the corpus into clusters, singletons and junk.
    /// Groups the corpus into clusters, singletons and junk.
    ///
    /// Components larger than `params.max_cluster_size` are re-partitioned with
    /// progressively stricter thresholds. A component that stays oversized is
    /// reported as singletons rather than as one enormous "cluster": a group
    /// that cannot be broken apart is not a topic, and presenting it as one
    /// would hand the model stage 2 a meaningless pile.
    pub fn partition(&self, docs: &[Doc]) -> Partition {
        let mut result = Partition::default();
        if self.is_empty() {
            return result;
        }

        // --- junk gate -------------------------------------------------------
        let mut eligible: Vec<usize> = Vec::new();
        for idx in 0..self.len() {
            let doc_text_len = docs
                .iter()
                .find(|d| d.id == self.ids[idx])
                .map(|d| d.text.trim().chars().count())
                .unwrap_or(0);
            if doc_text_len < self.params.min_chars
                || self.sets[idx].len() < self.params.min_identifying_tokens
            {
                result.junk.push(self.ids[idx].clone());
            } else {
                eligible.push(idx);
            }
        }

        let mut diffuse: Vec<usize> = Vec::new();
        for component in self.components_within(&eligible, 0) {
            if component.len() <= self.params.max_cluster_size && self.cohesion(&component) >= self.params.min_cohesion {
                self.emit_cluster(&component, &mut result);
            } else {
                diffuse.extend(component);
            }
        }
        result
            .singletons
            .extend(diffuse.into_iter().map(|idx| self.ids[idx].clone()));

        result.junk.sort();
        result.singletons.sort();
        result.clusters.sort_by(|a, b| {
            b.members
                .len()
                .cmp(&a.members.len())
                .then(a.id.cmp(&b.id))
        });
        self.attach_assignments(&mut result);
        result
    }

    /// Connected components over the eligible set, escalating the edge
    /// thresholds while a component is too large to be actionable.
    fn components_within(&self, eligible: &[usize], round: usize) -> Vec<Vec<usize>> {
        if round >= self.params.max_split_rounds {
            return vec![eligible.to_vec()];
        }

        // Escalate: each round demands both a higher cosine floor and a larger
        // shared-IDF mass, so loosely related notes fall out of the component.
        let step = 1.0 + round as f32 * 0.6;
        let mut escalated = self.params.clone();
        escalated.min_similarity = (self.params.min_similarity * step).min(0.9);
        escalated.min_shared_ratio = (self.params.min_shared_ratio * step).min(0.95);
        escalated.knn = self.params.knn.saturating_sub(round + 1).max(3);

        let edges = self.edges_within(eligible, &escalated);
        let mut groups = components_from_edges(eligible, &edges);
        groups.sort_by(|a, b| b.len().cmp(&a.len()).then(a[0].cmp(&b[0])));

        let mut out = Vec::new();
        for group in groups {
            let too_big = group.len() > self.params.max_cluster_size;
            let incoherent = self.cohesion(&group) < self.params.min_cohesion;
            if (too_big || incoherent) && group.len() > self.params.min_cluster_size {
                out.extend(self.components_within(&group, round + 1));
            } else {
                out.push(group);
            }
        }
        out
    }

    /// Mean over members of each member's best similarity to another member.
    fn cohesion(&self, group: &[usize]) -> f32 {
        if group.len() < 2 {
            return 0.0;
        }
        let mut total = 0.0f32;
        for (pos, &i) in group.iter().enumerate() {
            let mut best = 0.0f32;
            for (other_pos, &j) in group.iter().enumerate() {
                if other_pos == pos {
                    continue;
                }
                let (sim, _, _) = self.similarity(i, j);
                if sim > best {
                    best = sim;
                }
            }
            total += best;
        }
        total / group.len() as f32
    }

    /// Candidate edges inside `eligible` under the given thresholds.
    fn edges_within(&self, eligible: &[usize], params: &ClusterParams) -> Vec<(usize, usize)> {
        let mut edges: Vec<(usize, usize, f32)> = Vec::new();
        for (pos_i, &i) in eligible.iter().enumerate() {
            for &j in eligible.iter().skip(pos_i + 1) {
                let (sim, shared, ratio) = self.similarity(i, j);
                if sim >= params.min_similarity
                    && shared >= params.min_shared_tokens
                    && ratio >= params.min_shared_ratio
                {
                    edges.push((i, j, sim));
                }
            }
        }
        edges.sort_by(|a, b| {
            b.2.partial_cmp(&a.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
                .then(a.1.cmp(&b.1))
        });

        // Mutual kNN: an edge survives only when each side is inside the
        // other's top `knn`. This bounds degree per node, which is what stops
        // one generic note from chaining the corpus into a single component.
        let mut rank: HashMap<(usize, usize), usize> = HashMap::new();
        for (i, j, _) in &edges {
            *rank.entry((*i, *j)).or_insert(0) += 1;
            *rank.entry((*j, *i)).or_insert(0) += 1;
        }
        let mut per_node: HashMap<usize, usize> = HashMap::new();
        let mut mutual: Vec<(usize, usize)> = Vec::new();
        for (i, j, _) in &edges {
            let ri = rank.get(&(*i, *j)).copied().unwrap_or(usize::MAX);
            let rj = rank.get(&(*j, *i)).copied().unwrap_or(usize::MAX);
            if ri >= params.knn || rj >= params.knn {
                continue;
            }
            let ci = *per_node.entry(*i).or_insert(0);
            let cj = *per_node.entry(*j).or_insert(0);
            if ci < params.knn && cj < params.knn {
                per_node.insert(*i, ci + 1);
                per_node.insert(*j, cj + 1);
                mutual.push((*i, *j));
            }
        }
        mutual
    }

    fn emit_cluster(&self, group: &[usize], result: &mut Partition) {
        if group.len() < self.params.min_cluster_size {
            result
                .singletons
                .extend(group.iter().map(|&i| self.ids[i].clone()));
            return;
        }
        let mut member_ids: Vec<String> = group.iter().map(|&i| self.ids[i].clone()).collect();
        member_ids.sort();
        let id = cluster_id(&member_ids);
        let label = self.label_for(group);
        result.clusters.push(Cluster {
            id,
            label,
            members: member_ids,
        });
    }

    fn attach_assignments(&self, result: &mut Partition) {
        result.assignments = result
            .clusters
            .iter()
            .flat_map(|c| {
                c.members.iter().map(move |m| Assignment {
                    id: m.clone(),
                    cluster_id: c.id.clone(),
                    bucket: Bucket::Cluster,
                })
            })
            .chain(result.singletons.iter().map(|id| Assignment {
                id: id.clone(),
                cluster_id: String::new(),
                bucket: Bucket::Singleton,
            }))
            .chain(result.junk.iter().map(|id| Assignment {
                id: id.clone(),
                cluster_id: String::new(),
                bucket: Bucket::Junk,
            }))
            .collect();
        result.assignments.sort_by(|a, b| a.id.cmp(&b.id));
    }

    /// Deterministic label: the highest-IDF identifying tokens shared by the
    /// group, which in practice reads as `web/src · internal/adapters ·
    /// circuit`. No model call, and it changes only when membership does.
    fn label_for(&self, group: &[usize]) -> String {
        let mut score: HashMap<&str, f32> = HashMap::new();
        for &idx in group {
            for tok in &self.sets[idx] {
                *score.entry(tok.as_str()).or_default() += self.idf_of(tok);
            }
        }
        let mut ranked: Vec<(&str, f32)> = score.into_iter().collect();
        ranked.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(b.0))
        });
        // Prefer identifier-shaped tokens for the label: a cluster called
        // `web/src · internal/adapters · circuit` is navigable, one called
        // `needed · present · deny` is not.
        let shaped = |t: &str| {
            t.contains('/') || t.contains('.') || t.contains('_') || t.chars().any(|c| c.is_ascii_digit())
        };
        let top: Vec<&str> = ranked
            .iter()
            .map(|(t, _)| *t)
            .filter(|t| t.len() >= 4)
            .collect();
        let top: Vec<&str> = {
            let shaped_only: Vec<&str> = top.iter().copied().filter(|t| shaped(t)).take(3).collect();
            if shaped_only.len() >= 2 {
                shaped_only
            } else {
                top.into_iter().take(3).collect()
            }
        };
        if top.is_empty() {
            return format!("cluster-{}", group.len());
        }
        top.join(" · ")
    }
}

/// Union-find components over an undirected edge list.
fn components_from_edges(nodes: &[usize], edges: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut parent: HashMap<usize, usize> = nodes.iter().map(|&n| (n, n)).collect();
    fn find(parent: &mut HashMap<usize, usize>, x: usize) -> usize {
        let mut root = x;
        while parent.get(&root).copied().unwrap_or(root) != root {
            root = parent.get(&root).copied().unwrap_or(root);
        }
        let mut cur = x;
        while cur != root {
            let next = parent.get(&cur).copied().unwrap_or(cur);
            parent.insert(cur, root);
            cur = next;
        }
        root
    }
    for (i, j) in edges {
        let (ri, rj) = (find(&mut parent, *i), find(&mut parent, *j));
        if ri != rj {
            parent.insert(rj, ri);
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for &n in nodes {
        let root = find(&mut parent, n);
        groups.entry(root).or_default().push(n);
    }
    groups.into_values().collect()
}

/// Stable cluster id derived from the member set, so re-running a round with
/// the same corpus yields the same ids and the audit log stays readable.
fn cluster_id(members: &[String]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for m in members {
        for byte in m.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("dream-cluster-{hash:016x}")
}

/// Near-duplicate detection over word shingles.
///
/// Containment (not Jaccard) is the right measure: a short note repeated
/// verbatim inside a longer one is a duplicate even though its Jaccard against
/// the longer entry is tiny.
pub fn near_duplicates(docs: &[Doc], params: &ClusterParams) -> Vec<DuplicatePair> {
    let capped = docs.len().min(params.max_docs);
    let mut sets: Vec<BTreeSet<String>> = Vec::with_capacity(capped);
    let mut tokens: Vec<BTreeSet<String>> = Vec::with_capacity(capped);
    for doc in docs.iter().take(capped) {
        sets.push(shingles(&doc.text, params.shingle_len));
        tokens.push(word_tokens(&doc.text));
    }
    let mut pairs = Vec::new();
    for i in 0..capped {
        for j in (i + 1)..capped {
            // Shingle path: needs enough shingles on both sides. Very short
            // notes (fewer words than `shingle_len`, e.g. "Adopt useLiveData
            // everywhere") produce no shingles at all and used to evade the
            // pass entirely even when quoted verbatim inside a longer entry.
            let shingle_hit = if sets[i].len() >= 3 && sets[j].len() >= 3 {
                let (small, large) = if sets[i].len() <= sets[j].len() {
                    (&sets[i], &sets[j])
                } else {
                    (&sets[j], &sets[i])
                };
                let shared = small.iter().filter(|s| large.contains(*s)).count();
                (shared >= 3).then(|| shared as f32 / small.len() as f32)
            } else {
                None
            };
            // Token fallback: when either side is too short for shingles,
            // fall back to word-token containment so a terse note subsumed
            // by a longer entry is still proposed. Still requires 3 shared
            // tokens so single-word overlaps never fire.
            let token_hit = if shingle_hit.is_none()
                && !tokens[i].is_empty()
                && !tokens[j].is_empty()
            {
                let (small, large) = if tokens[i].len() <= tokens[j].len() {
                    (&tokens[i], &tokens[j])
                } else {
                    (&tokens[j], &tokens[i])
                };
                let shared = small.iter().filter(|t| large.contains(*t)).count();
                (shared >= 3).then(|| shared as f32 / small.len() as f32)
            } else {
                None
            };
            let containment = match (shingle_hit, token_hit) {
                (Some(c), _) => c,
                (None, Some(c)) => c,
                (None, None) => continue,
            };
            if containment >= params.dup_containment {
                pairs.push(DuplicatePair {
                    a: docs[i].id.clone(),
                    b: docs[j].id.clone(),
                    containment,
                });
            }
        }
    }
    pairs.sort_by(|a, b| {
        b.containment
            .partial_cmp(&a.containment)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.a.cmp(&b.a))
            .then(a.b.cmp(&b.b))
    });
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(id: &str, text: &str) -> Doc {
        Doc {
            id: id.to_owned(),
            text: text.to_owned(),
        }
    }

    fn partition(docs: &[Doc]) -> Partition {
        let params = ClusterParams::default();
        Corpus::new(docs, params.clone()).partition(docs)
    }

    #[test]
    fn splits_two_unrelated_projects() {
        // Modelled on real agent session notes: identifier-dense, no prose
        // overlap between the two projects.
        let docs = vec![
            doc("a1", "edit-plugin.mjs stays in sync with web/src/design-viewer/DesignViewer.tsx and derivation.tsx"),
            doc("a2", "DesignViewer.tsx wraps DesignView.tsx and design-viewer.css; derivation harness writes styles.css"),
            doc("a3", "design.check flags design-ignore without a reason across DesignView.tsx and design-viewer.css"),
            doc("b1", "internal/adapters/tools/bash/readonly.go enforces an allowlisted read-only command set"),
            doc("b2", "readonly.go bash allowlist runs approved commands in an isolated shell per session"),
            doc("b3", "internal/adapters/tools/code resolves callers through code_callers graph edges"),
        ];
        let p = partition(&docs);
        let clusters = p.clusters_by_size();
        assert!(!clusters.is_empty(), "expected at least one cluster");
        let members: HashSet<&str> = clusters[0].members.iter().map(String::as_str).collect();
        assert!(
            members.contains("a1") && members.contains("a3"),
            "expected the design-viewer notes together, got {:?}",
            clusters[0].members
        );
        assert!(
            !members.contains("b1"),
            "unrelated project leaked into the cluster: {:?}",
            clusters[0].members
        );
    }

    #[test]
    fn deterministic_across_runs() {
        let docs: Vec<Doc> = (0..40)
            .map(|i| doc(&format!("id{i}"), &format!("internal/adapters/tools/bash allowlist entry number {i}")))
            .collect();
        let first = partition(&docs);
        let second = partition(&docs);
        assert_eq!(first.clusters.len(), second.clusters.len());
        assert_eq!(first.clusters[0].id, second.clusters[0].id);
        assert_eq!(first.clusters[0].label, second.clusters[0].label);
    }

    #[test]
    fn labels_are_readable() {
        let docs = vec![
            doc("a1", "web/src/lib/otel.ts processes OpenTelemetry data for the resource editor"),
            doc("a2", "web/src/components/ResourceOtelLogs.tsx renders otel logs using lib/otel.ts"),
            doc("a3", "otel helper web/src/lib/otel.ts plus ResourceOtelLogs component"),
        ];
        let p = partition(&docs);
        let label = &p.clusters_by_size()[0].label;
        assert!(label.contains("otel"), "label should name the topic: {label}");
    }

    #[test]
    fn short_notes_are_junk_not_singletons() {
        let docs = vec![
            doc("j1", "ok"),
            doc("j2", "Fyll vattenloopen"),
            doc("r1", "the internal/adapters/tools/bash allowlist is enforced in readonly.go at commit abc1234"),
            doc("r2", "readonly.go allowlist runs approved bash commands inside an isolated shell per session"),
            doc("r3", "bash allowlist enforcement added in internal/adapters/tools/bash/readonly.go"),
        ];
        let p = partition(&docs);
        assert!(p.junk.contains(&"j1".to_owned()));
        assert!(p.junk.contains(&"j2".to_owned()));
        assert_eq!(p.clusters.len(), 1);
    }

    #[test]
    fn finds_verbatim_duplicates() {
        let text = "internal/adapters/tools/bash/readonly.go enforces an allowlisted read-only command set for every session";
        let docs = vec![doc("d1", text), doc("d2", text), doc("u", "something completely different about gardening tools and water")];
        let p = ClusterParams::default();
        let dups = near_duplicates(&docs, &p);
        assert_eq!(dups.len(), 1);
        assert!(dups[0].containment >= 0.99);
    }

    #[test]
    fn containment_catches_a_short_note_inside_a_long_one() {
        let short = "the bash allowlist is enforced in readonly.go for every agent session";
        let long = format!(
            "# ACP WebSocket API\n\n## Connection\nEndpoint ws://host:9001/ with bearer auth.\n\n### Notes\n{short}\n\n### Commands\nsend_prompt, cancel, spawn_session and permission_response are the client commands."
        );
        let docs = vec![doc("short", short), doc("long", &long)];
        let dups = near_duplicates(&docs, &ClusterParams::default());
        assert!(!dups.is_empty(), "a verbatim quote inside a doc is a containment duplicate");
    }

    #[test]
    fn sub_shingle_note_inside_longer_entry_is_flagged() {
        // Real miss from the 2026-10-20 manual dreaming pass: a 3-word note
        // has no 5-shingles, so the old pass never proposed it even though it
        // sits verbatim inside the longer entry.
        let docs = vec![
            doc("short", "Adopt useLiveData everywhere"),
            doc(
                "long",
                "Adopt useLiveData everywhere to replace hand-rolled setInterval loaders; use SWR-style visible-tab polling.",
            ),
            doc("u", "something completely different about gardening tools and water"),
        ];
        let dups = near_duplicates(&docs, &ClusterParams::default());
        assert_eq!(dups.len(), 1, "expected the subsumed short note, got {dups:?}");
        assert!(dups[0].containment >= 0.99);
    }

    #[test]
    fn single_shared_word_is_not_a_duplicate() {
        let docs = vec![
            doc("a", "Ship dark theme"),
            doc("b", "the theme song ships free with every garden gnome"),
        ];
        let dups = near_duplicates(&docs, &ClusterParams::default());
        assert!(dups.is_empty(), "1-2 shared words must not fire: {dups:?}");
    }

    #[test]
    fn max_docs_bounds_the_corpus() {
        let docs: Vec<Doc> = (0..50)
            .map(|i| doc(&format!("id{i}"), &format!("internal/adapters/tools/bash allowlist entry {i} unique{i}")))
            .collect();
        let params = ClusterParams {
            max_docs: 10,
            ..ClusterParams::default()
        };
        let corpus = Corpus::new(&docs, params);
        assert_eq!(corpus.len(), 10);
    }

    #[test]
    fn empty_corpus_is_safe() {
        let p = partition(&[]);
        assert!(p.clusters.is_empty());
        assert!(p.assignments.is_empty());
    }
}