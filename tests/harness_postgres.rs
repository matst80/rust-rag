//! Postgres-backed harness integration tests.
//!
//! Requires Docker: starts a pgvector-enabled Postgres container, runs the
//! embedded migrations, then exercises the harness node/edge storage paths
//! end-to-end. Skipped by default — run via `make test-integration`
//! (passes `--ignored`).

use std::time::Duration;

use anyhow::Result;
use rust_rag::api::harness::{
    Badge, HARNESS_AUDIT, HARNESS_DOC, HARNESS_PLAN, HARNESS_SPRINT, HARNESS_TODO, REL_AUDITED,
    REL_CONTAINS, REL_ENFORCES_DOC, assemble_tree,
};
use rust_rag::db::postgres::{PostgresVectorStore, connect};
use rust_rag::db::{
    CodeStore, CodeSymbolGraph, GraphConfig, GraphEdgeType, ItemRecord, ListItemsRequest,
    LooseLinkRule, ManualEdgeInput, ParsedCodeCall, ParsedCodeFile, ParsedCodeRepo,
    ParsedCodeSymbol, SortOrder, VectorStore,
};

use testcontainers::core::{ContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{GenericImage, ImageExt};

const DIM: usize = 1024;

fn embedding() -> Vec<f32> {
    vec![0.05f32; DIM]
}

fn item(id: &str, type_name: &str, data: serde_json::Value) -> ItemRecord {
    ItemRecord {
        id: id.to_owned(),
        text: format!("content of {id}"),
        metadata: serde_json::json!({ "author": "harness-test" }),
        source_id: "harness".to_owned(),
        created_at: 1,
        updated_at: 1,
        path: None,
        type_name: Some(type_name.to_owned()),
        data: Some(data),
        analysis: None,
        dreamed_at: None,
    }
}

async fn start_postgres() -> (
    testcontainers::ContainerAsync<GenericImage>,
    PostgresVectorStore,
    CodeStore,
) {
    let image = GenericImage::new("pgvector/pgvector", "pg16")
        .with_wait_for(WaitFor::message_on_stderr(
            "database system is ready to accept connections",
        ))
        .with_exposed_port(ContainerPort::Tcp(5432))
        .with_env_var("POSTGRES_USER", "rag")
        .with_env_var("POSTGRES_PASSWORD", "rag")
        .with_env_var("POSTGRES_DB", "rag");
    let container = image.start().await.expect("starting postgres container");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("mapped port");
    let url = format!("postgres://rag:rag@127.0.0.1:{port}/rag");
    // The init flow can print the readiness line before the server accepts
    // connections; retry until `connect` (which runs migrations) succeeds.
    let pool = loop {
        match connect(&url, 5).await {
            Ok(pool) => break pool,
            Err(e) => {
                eprintln!("postgres not ready yet: {e}");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    };
    let store = PostgresVectorStore::new(
        pool.clone(),
        tokio::runtime::Handle::current(),
        GraphConfig {
            enabled: true,
            ..Default::default()
        },
    );
    let code_store = CodeStore::new(pool, tokio::runtime::Handle::current());
    (container, store, code_store)
}

/// Shared scenario: plan --BREAKS_INTO--> sprint --CONTAINS_TODO--> todo,
/// todo --ENFORCES_DOC--> doc, plan --GOVERNED_BY--> doc,
/// audit node --AUDITED--> todo.
fn seed_scenario(store: &PostgresVectorStore) -> Result<()> {
    let plan = item(
        "plan-1",
        HARNESS_PLAN,
        serde_json::json!({"title": "Cache plan", "state": "PROPOSED"}),
    );
    let sprint = item(
        "sprint-1",
        HARNESS_SPRINT,
        serde_json::json!({"plan_id": "plan-1", "cadence": "weekly", "goal": "Cache", "state": "ACTIVE"}),
    );
    let doc = item(
        "doc-1",
        HARNESS_DOC,
        serde_json::json!({"doc_type": "ADR", "title": "ADR 7", "version": "1.1", "status": "ACTIVE"}),
    );
    let todo = item(
        "todo-1",
        HARNESS_TODO,
        serde_json::json!({"sprint_id": "sprint-1", "title": "Build cache", "action_spec": {"tool": "edit"}, "state": "PENDING", "sequence_order": 1}),
    );
    let audit = item(
        "audit-1",
        HARNESS_AUDIT,
        serde_json::json!({
            "target_id": "todo-1",
            "target_type": "harness_todo",
            "passed": false,
            "score": 0.4,
            "violations": [{"doc_id": "doc-1", "title": "ADR 7", "violation_reason": "process-local cache violates shared-pool invariant", "severity": "BLOCKING"}],
            "unanchored_assumptions": ["cache is per-process"]
        }),
    );

    store.upsert_item(plan, &embedding())?;
    store.upsert_item(sprint, &embedding())?;
    store.upsert_item(doc, &embedding())?;
    store.upsert_item(todo, &embedding())?;
    store.upsert_item(audit, &embedding())?;

    for (from, to, relation) in [
        ("plan-1", "sprint-1", "BREAKS_INTO"),
        ("sprint-1", "todo-1", REL_CONTAINS),
        ("plan-1", "doc-1", REL_ENFORCES_DOC),
        ("todo-1", "doc-1", REL_ENFORCES_DOC),
        ("audit-1", "todo-1", REL_AUDITED),
    ] {
        store.add_manual_edge(ManualEdgeInput {
            from_item_id: from.to_owned(),
            to_item_id: to.to_owned(),
            relation: Some(relation.into()),
            sort_order: None,
            weight: 1.0,
            directed: true,
            metadata: serde_json::json!({}),
        })?;
    }
    Ok(())
}

#[tokio::test]
#[ignore]
async fn harness_nodes_edges_and_tree_on_postgres() {
    // PostgresVectorStore blocks on a dedicated pool runtime, so every store
    // call must run off the async reactor — same as the production handlers.
    // The container handle is moved into the blocking task so the container
    // stays alive (and is removed) only when the test body finishes.
    let (container, store, _code_store) = start_postgres().await;
    tokio::task::spawn_blocking(move || {
        let _container = container;
        seed_scenario(&store).expect("seeding scenario");

    // Typed listing resolves every harness node type.
    let (plans, _) = store
        .list_items(ListItemsRequest {
            type_name: Some(HARNESS_PLAN.to_owned()),
            limit: Some(100),
            sort_order: SortOrder::Desc,
            ..Default::default()
        })
        .expect("listing plans");
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].id, "plan-1");

    // Structural edges persist as manual directed edges.
    let edges = store
        .list_graph_edges(None, Some(GraphEdgeType::Manual), None)
        .expect("listing edges");
    let relations: Vec<_> = edges.iter().filter_map(|e| e.relation.as_deref()).collect();
    for expected in [REL_CONTAINS, REL_ENFORCES_DOC, REL_AUDITED, "BREAKS_INTO"] {
        assert!(relations.contains(&expected), "missing {expected}");
    }

    // 2-hop traversal from the plan reaches both the doc and the todo.
    let neighborhood = store
        .graph_neighborhood("plan-1", 2, 50, Some(GraphEdgeType::Manual))
        .expect("traversal");
    let neighbor_ids: Vec<&str> = neighborhood
        .nodes
        .iter()
        .map(|n| n.id.as_str())
        .collect();
    assert!(neighbor_ids.contains(&"doc-1"));
    assert!(neighbor_ids.contains(&"todo-1"));

    // Type-scoped vector search finds the todo (1024-dim cosine over pgvector).
    let hits = store
        .search(&embedding(), 10, None, Some(HARNESS_TODO))
        .expect("typed search");
    assert!(hits.iter().any(|h| h.id == "todo-1"));

    // Tree assembly resolves the latest audit verdict + badges.
    let mut items = Vec::new();
    for tn in [HARNESS_PLAN, HARNESS_SPRINT, HARNESS_TODO, HARNESS_DOC, HARNESS_AUDIT] {
        let (page, _) = store
            .list_items(ListItemsRequest {
                type_name: Some(tn.to_owned()),
                limit: Some(1000),
                sort_order: SortOrder::Desc,
                ..Default::default()
            })
            .expect("tree listing");
        items.extend(page);
    }
    let tree = assemble_tree(items, edges, Vec::new());
    let todo = tree.nodes.iter().find(|n| n.id == "todo-1").unwrap();
    assert_eq!(todo.badge, Some(Badge::Red));
    let verdict = todo.verdict.as_ref().unwrap();
    assert!(!verdict.passed);
    assert_eq!(&verdict.violations[0].doc_id, "doc-1");
    // Upserting with the same id replaces the row (idempotent projection).
    let updated = item("todo-1", HARNESS_TODO, serde_json::json!({"sprint_id": "sprint-1", "title": "Build cache v2", "action_spec": {"tool": "edit"}, "state": "IN_PROGRESS", "sequence_order": 1}));
    store.upsert_item(updated, &embedding()).unwrap();
    let (todos, _) = store
        .list_items(ListItemsRequest {
            type_name: Some(HARNESS_TODO.to_owned()),
            limit: Some(100),
            sort_order: SortOrder::Desc,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(todos.len(), 1);
    assert_eq!(todos[0].data.as_ref().unwrap()["title"], "Build cache v2");
    })
    .await
    .expect("blocking test body");
}

/// Synthetic `code_symbols` row for code-search graph tests.
fn code_symbol(path: &str, line: i32, name: &str) -> ParsedCodeSymbol {
    ParsedCodeSymbol {
        path: path.to_owned(),
        line,
        end_line: line + 5,
        kind: "function".to_owned(),
        name: name.to_owned(),
        qualname: name.to_owned(),
        signature: format!("fn {name}()"),
        doc: String::new(),
        lang: "rust".to_owned(),
        text: String::new(),
        visibility: "public".to_owned(),
        exemplar: String::new(),
        hotpath: String::new(),
        embedding: None,
    }
}

/// Synthetic `code_calls` row for code-search graph tests.
fn code_call(
    caller_path: &str,
    caller_symbol: &str,
    call_line: i32,
    callee_name: &str,
) -> ParsedCodeCall {
    ParsedCodeCall {
        caller_path: caller_path.to_owned(),
        caller_symbol: caller_symbol.to_owned(),
        caller_line: call_line,
        callee_name: callee_name.to_owned(),
        callee_operand: String::new(),
        call_line,
        call_column: 4,
    }
}

fn node_ids(g: &CodeSymbolGraph) -> Vec<&str> {
    g.nodes.iter().map(|n| n.symbol.name.as_str()).collect()
}

fn edge_ids(g: &CodeSymbolGraph) -> Vec<(String, String)> {
    let mut v: Vec<_> = g
        .edges
        .iter()
        .map(|e| (e.from.name.clone(), e.to.name.clone()))
        .collect();
    v.sort();
    v
}

/// Code-search scenario: call chain a -> b -> c -> d inside src/a.rs, plus
/// x (in src/x.rs) calling a. Anchoring on `a` must walk the whole chain in
/// both directions as depth grows.
#[tokio::test]
#[ignore]
async fn code_symbol_graph_multi_hop_on_postgres() {
    let (container, _store, code) = start_postgres().await;
    tokio::task::spawn_blocking(move || {
        let _container = container;

        let parsed = ParsedCodeRepo {
            source_version: "test".to_owned(),
            files: vec![
                ParsedCodeFile {
                    path: "src/a.rs".to_owned(),
                    size_bytes: 400,
                    mtime: 1,
                },
                ParsedCodeFile {
                    path: "src/x.rs".to_owned(),
                    size_bytes: 100,
                    mtime: 1,
                },
            ],
            symbols: vec![
                code_symbol("src/a.rs", 1, "a"),
                code_symbol("src/a.rs", 10, "b"),
                code_symbol("src/a.rs", 20, "c"),
                code_symbol("src/a.rs", 30, "d"),
                code_symbol("src/x.rs", 1, "x"),
            ],
            calls: vec![
                code_call("src/a.rs", "a", 3, "b"),
                code_call("src/a.rs", "b", 13, "c"),
                code_call("src/a.rs", "c", 23, "d"),
                code_call("src/x.rs", "x", 5, "a"),
            ],
            symbols_skipped: 0,
        };
        code.upsert_repo("code-graph-test", "/tmp/code-graph-test", parsed)
            .expect("uploading snapshot");

        // Depth 1: direct neighbors only — the pre-multi-hop behavior.
        let g1 = code
            .symbol_graph("code-graph-test", "src/a.rs", "a", 24, 1, 200)
            .expect("depth-1 graph")
            .expect("anchor found");
        assert_eq!(g1.depth, 1);
        assert_eq!(node_ids(&g1), vec!["a", "b", "x"]);
        assert_eq!(
            g1.nodes.iter().map(|n| n.depth).collect::<Vec<_>>(),
            vec![0, 1, 1]
        );
        assert_eq!(
            edge_ids(&g1),
            vec![
                ("a".to_owned(), "b".to_owned()),
                ("x".to_owned(), "a".to_owned())
            ]
        );
        assert_eq!(
            g1.callees
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            vec!["b"]
        );
        assert_eq!(
            g1.callers
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            vec!["x"]
        );

        // Depth 3: the whole chain, each node at its minimum BFS depth. Edges
        // rediscovered while expanding b/x/c must dedup against earlier hops.
        let g3 = code
            .symbol_graph("code-graph-test", "src/a.rs", "a", 24, 3, 200)
            .expect("depth-3 graph")
            .expect("anchor found");
        assert_eq!(g3.depth, 3);
        assert_eq!(node_ids(&g3), vec!["a", "b", "x", "c", "d"]);
        assert_eq!(
            g3.nodes
                .iter()
                .map(|n| (n.symbol.name.as_str(), n.depth))
                .collect::<Vec<_>>(),
            vec![("a", 0), ("b", 1), ("x", 1), ("c", 2), ("d", 3)]
        );
        assert_eq!(
            edge_ids(&g3),
            vec![
                ("a".to_owned(), "b".to_owned()),
                ("b".to_owned(), "c".to_owned()),
                ("c".to_owned(), "d".to_owned()),
                ("x".to_owned(), "a".to_owned()),
            ]
        );
    })
    .await
    .expect("blocking test body");
}

/// Cycle scenario: a -> b -> c -> a (mutual recursion) with a tail c -> d and
/// an external caller x -> a. The BFS must terminate: each symbol expands
/// exactly once, cycle edges dedup, and even max depth (5) yields the same
/// 5 distinct symbols — no runaway growth, tail d reached at depth 2.
#[tokio::test]
#[ignore]
async fn code_symbol_graph_cycle_terminates_on_postgres() {
    let (container, _store, code) = start_postgres().await;
    tokio::task::spawn_blocking(move || {
        let _container = container;

        let parsed = ParsedCodeRepo {
            source_version: "test".to_owned(),
            files: vec![
                ParsedCodeFile {
                    path: "src/a.rs".to_owned(),
                    size_bytes: 500,
                    mtime: 1,
                },
                ParsedCodeFile {
                    path: "src/x.rs".to_owned(),
                    size_bytes: 100,
                    mtime: 1,
                },
            ],
            symbols: vec![
                code_symbol("src/a.rs", 1, "a"),
                code_symbol("src/a.rs", 10, "b"),
                code_symbol("src/a.rs", 20, "c"),
                code_symbol("src/a.rs", 30, "d"),
                code_symbol("src/x.rs", 1, "x"),
            ],
            calls: vec![
                code_call("src/a.rs", "a", 3, "b"),
                code_call("src/a.rs", "b", 13, "c"),
                code_call("src/a.rs", "c", 21, "a"), // cycle back to the anchor
                code_call("src/a.rs", "c", 23, "d"),
                code_call("src/x.rs", "x", 5, "a"),
            ],
            symbols_skipped: 0,
        };
        code.upsert_repo("code-cycle-test", "/tmp/code-cycle-test", parsed)
            .expect("uploading snapshot");

        // Depth 1: callers of `a` include both x and c (the cycle edge).
        let g1 = code
            .symbol_graph("code-cycle-test", "src/a.rs", "a", 24, 1, 200)
            .expect("depth-1 graph")
            .expect("anchor found");
        assert_eq!(node_ids(&g1), vec!["a", "b", "c", "x"]);
        assert_eq!(
            edge_ids(&g1),
            vec![
                ("a".to_owned(), "b".to_owned()),
                ("c".to_owned(), "a".to_owned()),
                ("x".to_owned(), "a".to_owned()),
            ]
        );

        // Max depth over the cycle: still exactly 5 distinct symbols.
        let g5 = code
            .symbol_graph("code-cycle-test", "src/a.rs", "a", 24, 5, 200)
            .expect("depth-5 graph")
            .expect("anchor found");
        assert_eq!(node_ids(&g5), vec!["a", "b", "c", "x", "d"]);
        assert_eq!(
            g5.nodes
                .iter()
                .map(|n| (n.symbol.name.as_str(), n.depth))
                .collect::<Vec<_>>(),
            vec![("a", 0), ("b", 1), ("c", 1), ("x", 1), ("d", 2)]
        );
        assert_eq!(
            edge_ids(&g5),
            vec![
                ("a".to_owned(), "b".to_owned()),
                ("b".to_owned(), "c".to_owned()),
                ("c".to_owned(), "a".to_owned()),
                ("c".to_owned(), "d".to_owned()),
                ("x".to_owned(), "a".to_owned()),
            ]
        );
    })
    .await
    .expect("blocking test body");
}

#[tokio::test]
#[ignore]
async fn code_search_bm25_without_embeddings_on_postgres() {
    let (container, _store, code) = start_postgres().await;
    tokio::task::spawn_blocking(move || {
        let _container = container;

        let mut sym1 = code_symbol("src/parser.rs", 10, "parse_snapshot_db");
        sym1.signature =
            "pub fn parse_snapshot_db(bytes: &[u8]) -> Result<ParsedCodeRepo>".to_owned();
        sym1.doc = "Parse codesearch database snapshot without requiring dense vectors.".to_owned();
        sym1.text = "let conn = rusqlite::Connection::open(path)?;".to_owned();

        let mut sym2 = code_symbol("src/search.rs", 50, "search_symbols");
        sym2.signature = "pub fn search_symbols(query: &str) -> Vec<Hit>".to_owned();
        sym2.doc = "Full text BM25 ranking across code repository symbols.".to_owned();
        sym2.text = "SELECT id, name FROM code_symbols WHERE tsv @@ query".to_owned();

        let parsed = ParsedCodeRepo {
            source_version: "test-bm25".to_owned(),
            files: vec![
                ParsedCodeFile {
                    path: "src/parser.rs".to_owned(),
                    size_bytes: 500,
                    mtime: 1,
                },
                ParsedCodeFile {
                    path: "src/search.rs".to_owned(),
                    size_bytes: 400,
                    mtime: 1,
                },
            ],
            symbols: vec![sym1, sym2],
            calls: vec![],
            symbols_skipped: 0,
        };
        code.upsert_repo("bm25-test-repo", "/tmp/bm25-test", parsed)
            .expect("uploading snapshot without embeddings");

        // 1. Exact / partial symbol name search
        let hits = code
            .search("parse_snapshot_db", Some("bm25-test-repo"), None, None, 10)
            .expect("search exact name");
        assert!(!hits.is_empty(), "expected hits for exact name");
        assert_eq!(hits[0].symbol_name.as_deref(), Some("parse_snapshot_db"));
        assert!(hits[0].score > 0.5, "exact match score should be high");

        // 2. Snake-case keyword search ("snapshot")
        let hits_keyword = code
            .search("snapshot", Some("bm25-test-repo"), None, None, 10)
            .expect("search keyword");
        assert!(
            !hits_keyword.is_empty(),
            "expected hits for keyword snapshot"
        );
        assert_eq!(
            hits_keyword[0].symbol_name.as_deref(),
            Some("parse_snapshot_db")
        );

        // 3. Keyword in doc comments / BM25 search
        let hits_bm25 = code
            .search("BM25 ranking", Some("bm25-test-repo"), None, None, 10)
            .expect("search BM25 ranking");
        assert!(!hits_bm25.is_empty(), "expected hits for doc keyword");
        assert_eq!(hits_bm25[0].symbol_name.as_deref(), Some("search_symbols"));
        assert!(hits_bm25[0].score > 0.0);
    })
    .await
    .expect("blocking test body");
}

fn repo_item(id: &str, repo: &str) -> ItemRecord {
    ItemRecord {
        id: id.to_owned(),
        text: format!("content of {id}"),
        metadata: serde_json::json!({ "repo": repo }),
        source_id: "knowledge".to_owned(),
        created_at: 1,
        updated_at: 1,
        path: None,
        type_name: None,
        data: None,
        analysis: None,
        dreamed_at: None,
    }
}

#[tokio::test]
#[ignore]
async fn loose_links_on_postgres() {
    // Same shape as the harness test: the store blocks on its own pool runtime,
    // so every call must happen inside spawn_blocking.
    let (container, store, _code) = start_postgres().await;
    tokio::task::spawn_blocking(move || {
        let _container = container;
        let rule = LooseLinkRule {
            metadata_key: "repo".to_owned(),
            relation: "shares_repo".to_owned(),
            weight: 0.6,
        };

        for (id, repo) in [
            ("ll-a", "rust-rag"),
            ("ll-b", "rust-rag"),
            ("ll-c", "other"),
        ] {
            store
                .upsert_item(repo_item(id, repo), &embedding())
                .expect("seeding document");
        }
        // A hand-curated edge that must survive every rebuild.
        store
            .add_manual_edge(ManualEdgeInput {
                from_item_id: "ll-a".to_owned(),
                to_item_id: "ll-b".to_owned(),
                relation: Some("part_of".into()),
                sort_order: None,
                weight: 1.0,
                directed: false,
                metadata: serde_json::json!({}),
            })
            .expect("seeding manual edge");

        let report = store
            .rebuild_loose_links(std::slice::from_ref(&rule), 0)
            .expect("rebuild");
        assert_eq!(report.clusters, 1, "the `other` repo is a singleton");
        assert_eq!(report.hub_edges, 2);

        // No row is stored for the hub — the edges dangle — but the
        // neighborhood still resolves it synthetically.
        assert!(store
            .get_item("ext:repo:rust-rag")
            .expect("loading hub")
            .is_none());
        let hub_view = store
            .graph_neighborhood("ll-a", 1, 100, None)
            .expect("traversal");
        let hub = hub_view
            .nodes
            .iter()
            .find(|n| n.id == "ext:repo:rust-rag")
            .expect("synthetic hub node present");
        assert_eq!(hub.metadata["external"], serde_json::json!(true));

        // Rerunning must upsert (migration 0020's source dedup lane), not
        // duplicate. Postgres has an ON CONFLICT path here; SQLite does not.
        store
            .rebuild_loose_links(std::slice::from_ref(&rule), 0)
            .expect("rerun");
        let edges = store
            .list_graph_edges(None, None, None)
            .expect("listing edges");
        // A third identical pass must be a recognized no-op, not a rewrite.
        let third = store
            .rebuild_loose_links(std::slice::from_ref(&rule), 0)
            .expect("third pass");
        assert!(third.unchanged, "idle Postgres tick reported a write");
        // 1 hand-curated + 2 hub edges.
        assert_eq!(edges.len(), 3, "rerun duplicated edges: {edges:?}");

        assert!(
            edges
                .iter()
                .any(|e| e.relation.as_deref() == Some("part_of")
                    && e.metadata["source"].is_null()),
            "rebuild deleted or rewrote a hand-curated edge: {edges:?}"
        );

        // Depth-2 BFS from a member reaches the whole cluster through the hub.
        let neighborhood = store
            .graph_neighborhood("ll-a", 2, 100, None)
            .expect("traversal");
        let ids: Vec<&str> = neighborhood.nodes.iter().map(|n| n.id.as_str()).collect();
        for expected in ["ll-a", "ll-b", "ext:repo:rust-rag"] {
            assert!(ids.contains(&expected), "{expected} unreachable: {ids:?}");
        }

        // Re-pointing an entry shrinks the cluster on the next full rebuild.
        store
            .upsert_item(repo_item("ll-b", "moved"), &embedding())
            .expect("re-seeding document");
        let after = store.rebuild_loose_links(&[rule], 0).expect("rebuild");
        assert!(!after.unchanged, "a changed plan must not be a no-op");
        assert_eq!(after.clusters, 0);
        assert_eq!(after.edges_committed, 0);
        let remaining = store
            .list_graph_edges(None, None, None)
            .expect("listing edges");
        assert_eq!(remaining.len(), 1, "stale loose links left behind");
    })
    .await
    .expect("blocking test body");
}
