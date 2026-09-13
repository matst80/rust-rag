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
    REL_CONTAINS_TODO, REL_ENFORCES_DOC, REL_GOVERNED_BY, assemble_tree,
};
use rust_rag::db::postgres::{PostgresVectorStore, connect};
use rust_rag::db::{
    GraphConfig, GraphEdgeType, ItemRecord, ListItemsRequest, ManualEdgeInput, SortOrder,
    VectorStore,
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
        pool,
        tokio::runtime::Handle::current(),
        GraphConfig {
            enabled: true,
            ..Default::default()
        },
    );
    (container, store)
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
        ("sprint-1", "todo-1", REL_CONTAINS_TODO),
        ("plan-1", "doc-1", REL_GOVERNED_BY),
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
    let (container, store) = start_postgres().await;
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
    for expected in [REL_GOVERNED_BY, REL_CONTAINS_TODO, REL_ENFORCES_DOC, REL_AUDITED, "BREAKS_INTO"] {
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
