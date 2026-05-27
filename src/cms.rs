use anyhow::{Result, anyhow};
use std::{
    collections::{HashSet, VecDeque, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    sync::{Arc, RwLock},
};

use crate::db::{GraphEdgeRecord, GraphEdgeType, ItemRecord, VectorStore};

const MAX_CMS_DEPTH: usize = 32;

#[derive(Debug, Clone, PartialEq)]
pub struct CmsTreeChild {
    pub edge: GraphEdgeRecord,
    pub node: CmsTreeNode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CmsTreeNode {
    pub entry: ItemRecord,
    pub children: Vec<CmsTreeChild>,
}

#[derive(Debug, Clone)]
struct CmsAdjacency {
    children: Vec<GraphEdgeRecord>,
    parents: Vec<String>,
}

#[derive(Debug, Clone)]
struct CachedCmsTree {
    fingerprint: String,
    tree: CmsTreeNode,
}

#[derive(Debug, Default)]
struct CmsCache {
    adjacency: std::collections::HashMap<String, CmsAdjacency>,
    subtrees: std::collections::HashMap<String, CachedCmsTree>,
}

#[derive(Clone)]
pub struct CmsRuntime {
    store: Arc<dyn VectorStore>,
    cache: Arc<RwLock<CmsCache>>,
}

impl CmsRuntime {
    pub fn new(store: Arc<dyn VectorStore>) -> Self {
        Self {
            store,
            cache: Arc::new(RwLock::new(CmsCache::default())),
        }
    }

    pub fn build_tree(&self, root_id: &str) -> Result<CmsTreeNode> {
        let mut visited = HashSet::new();
        self.build_tree_internal(root_id, 0, &mut visited)
    }

    pub fn invalidate_nodes<I, S>(&self, node_ids: I) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut queue: VecDeque<String> = node_ids.into_iter().map(Into::into).collect();
        let mut visited = HashSet::new();

        while let Some(node_id) = queue.pop_front() {
            if !visited.insert(node_id.clone()) {
                continue;
            }

            let parents = {
                let mut cache = self.cache.write().expect("cms cache poisoned");
                cache.subtrees.remove(&node_id);
                cache.adjacency.remove(&node_id).map(|entry| entry.parents)
            };

            let parents = match parents {
                Some(parents) => parents,
                None => self.fetch_adjacency(&node_id)?.parents,
            };

            for parent_id in parents {
                queue.push_back(parent_id);
            }
        }

        Ok(())
    }

    fn build_tree_internal(
        &self,
        node_id: &str,
        depth: usize,
        visited: &mut HashSet<String>,
    ) -> Result<CmsTreeNode> {
        if depth > MAX_CMS_DEPTH {
            return Err(anyhow!("CMS tree exceeded max depth at {node_id}"));
        }
        if !visited.insert(node_id.to_owned()) {
            return Err(anyhow!("CMS tree cycle detected at {node_id}"));
        }

        if let Some(tree) = self
            .cache
            .read()
            .expect("cms cache poisoned")
            .subtrees
            .get(node_id)
            .cloned()
        {
            visited.remove(node_id);
            return Ok(tree.tree);
        }

        let entry = self
            .store
            .get_item(node_id)?
            .ok_or_else(|| anyhow!("item {node_id} not found"))?;
        let adjacency = self.get_or_fetch_adjacency(node_id)?;

        let mut children = Vec::with_capacity(adjacency.children.len());
        let mut child_fingerprints = Vec::with_capacity(adjacency.children.len());

        for edge in adjacency.children {
            let child = self.build_tree_internal(&edge.to_item_id, depth + 1, visited)?;
            let child_fingerprint = self
                .cache
                .read()
                .expect("cms cache poisoned")
                .subtrees
                .get(&edge.to_item_id)
                .map(|entry| entry.fingerprint.clone())
                .unwrap_or_default();
            child_fingerprints.push((
                edge.id.clone(),
                edge.updated_at,
                edge.sort_order.clone(),
                child_fingerprint,
            ));
            children.push(CmsTreeChild { edge, node: child });
        }

        let tree = CmsTreeNode { entry, children };
        let fingerprint = fingerprint_tree(&tree, &child_fingerprints);
        self.cache
            .write()
            .expect("cms cache poisoned")
            .subtrees
            .insert(
                node_id.to_owned(),
                CachedCmsTree {
                    fingerprint,
                    tree: tree.clone(),
                },
            );
        visited.remove(node_id);
        Ok(tree)
    }

    fn get_or_fetch_adjacency(&self, node_id: &str) -> Result<CmsAdjacency> {
        if let Some(entry) = self
            .cache
            .read()
            .expect("cms cache poisoned")
            .adjacency
            .get(node_id)
            .cloned()
        {
            return Ok(entry);
        }

        let entry = self.fetch_adjacency(node_id)?;
        self.cache
            .write()
            .expect("cms cache poisoned")
            .adjacency
            .insert(node_id.to_owned(), entry.clone());
        Ok(entry)
    }

    fn fetch_adjacency(&self, node_id: &str) -> Result<CmsAdjacency> {
        let edges =
            self.store
                .list_graph_edges(Some(node_id), Some(GraphEdgeType::Manual), None)?;
        let mut children = Vec::new();
        let mut parents = HashSet::new();

        for edge in edges {
            if !is_structural_edge(&edge) {
                continue;
            }
            if edge.from_item_id == node_id {
                children.push(edge.clone());
            }
            if edge.to_item_id == node_id {
                parents.insert(edge.from_item_id.clone());
            }
        }

        children.sort_by(|a, b| {
            a.sort_order
                .cmp(&b.sort_order)
                .then_with(|| a.id.cmp(&b.id))
        });

        Ok(CmsAdjacency {
            children,
            parents: parents.into_iter().collect(),
        })
    }

    #[cfg(test)]
    fn cached_subtree_count(&self) -> usize {
        self.cache
            .read()
            .expect("cms cache poisoned")
            .subtrees
            .len()
    }

    #[cfg(test)]
    fn has_cached_subtree(&self, node_id: &str) -> bool {
        self.cache
            .read()
            .expect("cms cache poisoned")
            .subtrees
            .contains_key(node_id)
    }
}

fn is_structural_edge(edge: &GraphEdgeRecord) -> bool {
    if edge.edge_type != GraphEdgeType::Manual {
        return false;
    }
    if edge.metadata.get("status").and_then(|value| value.as_str()) == Some("suggested") {
        return false;
    }
    matches!(edge.relation.as_deref(), Some("contains") | Some("part_of"))
}

fn fingerprint_tree(
    tree: &CmsTreeNode,
    child_fingerprints: &[(String, i64, String, String)],
) -> String {
    let mut hasher = DefaultHasher::new();
    tree.entry.id.hash(&mut hasher);
    tree.entry.updated_at.hash(&mut hasher);
    tree.entry.type_name.hash(&mut hasher);
    tree.entry.path.hash(&mut hasher);
    for (edge_id, edge_updated_at, sort_order, child_fingerprint) in child_fingerprints {
        edge_id.hash(&mut hasher);
        edge_updated_at.hash(&mut hasher);
        sort_order.hash(&mut hasher);
        child_fingerprint.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::CmsRuntime;
    use crate::db::{GraphConfig, ItemRecord, ManualEdgeInput, SqliteVectorStore, VectorStore};
    use serde_json::json;
    use std::{
        borrow::Cow,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    fn test_store() -> Arc<dyn VectorStore> {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
        let db_id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        Arc::new(
            SqliteVectorStore::connect_uri(
                &format!("file:cms-cache-tests-{db_id}?mode=memory&cache=shared"),
                3,
                GraphConfig {
                    enabled: true,
                    build_on_startup: false,
                    similarity_top_k: 2,
                    similarity_max_distance: 1.0,
                    cross_source: false,
                },
            )
            .expect("sqlite cms test store"),
        )
    }

    fn insert_item(
        store: &Arc<dyn VectorStore>,
        id: &str,
        updated_at: i64,
        type_name: &str,
        title: &str,
        text: &str,
    ) {
        store
            .upsert_item(
                ItemRecord {
                    id: id.to_owned(),
                    text: text.to_owned(),
                    metadata: json!({"title": title}),
                    source_id: "cms".to_owned(),
                    created_at: updated_at,
                    updated_at,
                    path: None,
                    type_name: Some(type_name.to_owned()),
                    data: Some(json!({"title": title, "markdown": text})),
                    analysis: None,
                },
                &[updated_at as f32, 0.0, 0.0],
            )
            .expect("insert cms item");
    }

    fn connect(store: &Arc<dyn VectorStore>, from: &str, to: &str, sort_order: &str) {
        store
            .add_manual_edge(ManualEdgeInput {
                from_item_id: from.to_owned(),
                to_item_id: to.to_owned(),
                relation: Some(Cow::Borrowed("contains")),
                sort_order: Some(sort_order.to_owned()),
                weight: 1.0,
                directed: true,
                metadata: json!({}),
            })
            .expect("set sort order");
    }

    #[test]
    fn builds_deep_tree_and_caches_subtrees() {
        let store = test_store();
        insert_item(&store, "page", 1, "cms_page", "Page", "root");
        insert_item(&store, "section", 2, "cms_section", "Section", "section");
        insert_item(&store, "swiper", 3, "cms_swiper", "Swiper", "swiper");
        insert_item(&store, "image", 4, "cms_image", "Image", "image");
        insert_item(&store, "caption", 5, "cms_markdown", "Caption", "hello");

        connect(&store, "page", "section", "01024");
        connect(&store, "section", "swiper", "01024");
        connect(&store, "swiper", "image", "01024");
        connect(&store, "image", "caption", "01024");

        let runtime = CmsRuntime::new(store.clone());
        let tree = runtime.build_tree("page").expect("cms tree");

        assert_eq!(tree.children[0].node.entry.id, "section");
        assert_eq!(tree.children[0].node.children[0].node.entry.id, "swiper");
        assert_eq!(
            tree.children[0].node.children[0].node.children[0]
                .node
                .entry
                .id,
            "image"
        );
        assert!(runtime.cached_subtree_count() >= 5);
        assert!(runtime.has_cached_subtree("page"));
    }

    #[test]
    fn invalidates_ancestors_when_leaf_changes() {
        let store = test_store();
        insert_item(&store, "page", 1, "cms_page", "Page", "root");
        insert_item(&store, "section", 2, "cms_section", "Section", "section");
        insert_item(&store, "leaf", 3, "cms_markdown", "Leaf", "old");

        connect(&store, "page", "section", "01024");
        connect(&store, "section", "leaf", "01024");

        let runtime = CmsRuntime::new(store.clone());
        let first = runtime.build_tree("page").expect("first tree");
        assert_eq!(first.children[0].node.children[0].node.entry.text, "old");
        assert!(runtime.has_cached_subtree("page"));

        insert_item(&store, "leaf", 100, "cms_markdown", "Leaf", "new");
        runtime
            .invalidate_nodes([String::from("leaf")])
            .expect("invalidate leaf");
        assert!(!runtime.has_cached_subtree("page"));

        let second = runtime.build_tree("page").expect("second tree");
        assert_eq!(second.children[0].node.children[0].node.entry.text, "new");
    }

    #[test]
    fn respects_sort_order_for_structural_children() {
        let store = test_store();
        insert_item(&store, "page", 1, "cms_page", "Page", "root");
        insert_item(&store, "a", 2, "cms_markdown", "A", "A");
        insert_item(&store, "b", 3, "cms_markdown", "B", "B");

        connect(&store, "page", "b", "02048");
        connect(&store, "page", "a", "01024");

        let runtime = CmsRuntime::new(store);
        let tree = runtime.build_tree("page").expect("sorted tree");

        let child_ids = tree
            .children
            .into_iter()
            .map(|child| child.node.entry.id)
            .collect::<Vec<_>>();
        assert_eq!(child_ids, vec!["a", "b"]);
    }
}
