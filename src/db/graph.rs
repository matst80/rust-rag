use anyhow::Result;
use rusqlite::{Connection, params};
use std::collections::HashSet;

use super::{
    GraphConfig, GraphEdgeRecord, GraphEdgeType, GraphNodeDistance, bool_to_sqlite,
    current_timestamp_millis, parse_json_column,
};

pub(super) fn list_graph_edges_internal(
    connection: &Connection,
    item_id: Option<&str>,
    edge_type: Option<GraphEdgeType>,
) -> Result<Vec<GraphEdgeRecord>> {
    let edge_type = edge_type.map(GraphEdgeType::as_str);
    let mut statement = connection.prepare(
        "
        SELECT
            id,
            from_item_id,
            to_item_id,
            edge_type,
            relation,
            weight,
            directed,
            metadata,
            created_at,
            updated_at
        FROM graph_edges
        WHERE (?1 IS NULL OR from_item_id = ?1 OR to_item_id = ?1)
          AND (?2 IS NULL OR edge_type = ?2)
        ORDER BY updated_at DESC, id ASC
        ",
    )?;
    let rows = statement.query_map(params![item_id, edge_type], map_graph_edge_row)?;

    let mut edges = Vec::new();
    for row in rows {
        edges.push(row?);
    }
    Ok(edges)
}

pub(super) fn list_pairwise_distances_for_ids(
    connection: &Connection,
    ids: &[String],
) -> Result<Vec<GraphNodeDistance>> {
    if ids.len() < 2 {
        return Ok(Vec::new());
    }

    let placeholders = std::iter::repeat("?")
        .take(ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "
        SELECT
            left_vec.id,
            right_vec.id,
            CAST(vec_distance_L2(left_vec.embedding, right_vec.embedding) AS REAL) AS distance
        FROM vec_items AS left_vec
        JOIN vec_items AS right_vec ON left_vec.id < right_vec.id
        WHERE left_vec.id IN ({placeholders})
          AND right_vec.id IN ({placeholders})
        ORDER BY left_vec.id ASC, right_vec.id ASC
        "
    );

    let mut statement = connection.prepare(&sql)?;
    let mut params_vec: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(ids.len() * 2);
    for id in ids {
        params_vec.push(id);
    }
    for id in ids {
        params_vec.push(id);
    }

    let rows = statement.query_map(rusqlite::params_from_iter(params_vec), |row| {
        Ok(GraphNodeDistance {
            from_item_id: row.get(0)?,
            to_item_id: row.get(1)?,
            distance: row.get(2)?,
        })
    })?;

    let mut distances = Vec::new();
    for row in rows {
        distances.push(row?);
    }
    Ok(distances)
}

pub(super) fn rebuild_similarity_graph_locked(
    connection: &mut Connection,
    graph_config: GraphConfig,
) -> Result<usize> {
    if !graph_config.enabled {
        return Ok(0);
    }

    let mut item_statement = connection.prepare(
        "
        SELECT items.id
        FROM items
        JOIN vec_items ON vec_items.id = items.id
        ORDER BY items.id ASC
        ",
    )?;
    let item_ids = item_statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(item_statement);

    let transaction = connection.transaction()?;
    transaction.execute("DELETE FROM graph_edges WHERE edge_type = 'similarity'", [])?;

    let mut inserted_pairs = HashSet::new();
    let timestamp = current_timestamp_millis()?;
    let mut inserted = 0usize;

    for item_id in item_ids {
        let candidates = {
            let mut statement = transaction.prepare(
                "
                SELECT
                    other.id,
                    CAST(vec_distance_L2(base.embedding, other_vec.embedding) AS REAL) AS distance
                FROM vec_items AS base
                JOIN items AS base_item ON base_item.id = base.id
                JOIN vec_items AS other_vec ON other_vec.id != base.id
                JOIN items AS other ON other.id = other_vec.id
                WHERE base.id = ?1
                  AND (?2 = 1 OR other.source_id = base_item.source_id)
                ORDER BY distance ASC, other.id ASC
                LIMIT ?3
                ",
            )?;
            let rows = statement.query_map(
                params![
                    item_id,
                    bool_to_sqlite(graph_config.cross_source),
                    graph_config.similarity_top_k as i64
                ],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, f32>(1)?)),
            )?;

            let mut candidates = Vec::new();
            for row in rows {
                candidates.push(row?);
            }
            candidates
        };

        for (other_id, distance) in candidates {
            if distance > graph_config.similarity_max_distance {
                continue;
            }
            let (from_item_id, to_item_id) = canonical_edge_pair(&item_id, &other_id);
            if !inserted_pairs.insert((from_item_id.clone(), to_item_id.clone())) {
                continue;
            }

            let weight = 1.0 / (1.0 + distance);
            let metadata = serde_json::json!({ "distance": distance });
            transaction.execute(
                "
                INSERT INTO graph_edges (
                    id, from_item_id, to_item_id, edge_type, relation, weight, directed, metadata, created_at, updated_at
                )
                VALUES (?1, ?2, ?3, 'similarity', NULL, ?4, 0, ?5, ?6, ?6)
                ",
                params![
                    format!("similarity:{from_item_id}:{to_item_id}"),
                    from_item_id,
                    to_item_id,
                    weight,
                    serde_json::to_string(&metadata)?,
                    timestamp
                ],
            )?;
            inserted += 1;
        }
    }

    transaction.commit()?;
    Ok(inserted)
}

pub(super) fn map_graph_edge_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GraphEdgeRecord> {
    let edge_type_raw = row.get::<_, String>(3)?;
    let edge_type = GraphEdgeType::from_str(&edge_type_raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    })?;

    Ok(GraphEdgeRecord {
        id: row.get(0)?,
        from_item_id: row.get(1)?,
        to_item_id: row.get(2)?,
        edge_type,
        relation: row.get(4)?,
        weight: row.get(5)?,
        directed: row.get::<_, i64>(6)? != 0,
        metadata: parse_json_column(row.get::<_, String>(7)?, 7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

pub(super) fn canonical_edge_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_owned(), right.to_owned())
    } else {
        (right.to_owned(), left.to_owned())
    }
}
