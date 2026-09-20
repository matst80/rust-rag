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
    CODE_EMBEDDING_DIM, CodeStore, CodeSymbolGraph, GraphConfig, GraphEdgeType, ItemRecord,
    ListItemsRequest, ManualEdgeInput, ParsedCodeCall, ParsedCodeFile, ParsedCodeRepo,
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
        embedding: vec![0.05f32; CODE_EMBEDDING_DIM],
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
                ParsedCodeFile { path: "src/a.rs".to_owned(), size_bytes: 400, mtime: 1 },
                ParsedCodeFile { path: "src/x.rs".to_owned(), size_bytes: 100, mtime: 1 },
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
        code
            .upsert_repo("code-graph-test", "/tmp/code-graph-test", parsed)
            .expect("uploading snapshot");

        // Depth 1: direct neighbors only — the pre-multi-hop behavior.
        let g1 = code
            .symbol_graph("code-graph-test", "src/a.rs", "a", 24, 1, 200)
            .expect("depth-1 graph")
            .expect("anchor found");
        assert_eq!(g1.depth, 1);
        assert_eq!(node_ids(&g1), vec!["a", "b", "x"]);
        assert_eq!(g1.nodes.iter().map(|n| n.depth).collect::<Vec<_>>(), vec![0, 1, 1]);
        assert_eq!(
            edge_ids(&g1),
            vec![("a".to_owned(), "b".to_owned()), ("x".to_owned(), "a".to_owned())]
        );
        assert_eq!(
            g1.callees.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            vec!["b"]
        );
        assert_eq!(
            g1.callers.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
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
                ParsedCodeFile { path: "src/a.rs".to_owned(), size_bytes: 500, mtime: 1 },
                ParsedCodeFile { path: "src/x.rs".to_owned(), size_bytes: 100, mtime: 1 },
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
        code
            .upsert_repo("code-cycle-test", "/tmp/code-cycle-test", parsed)
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
