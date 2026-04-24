use axum::{
    Extension, Json,
    extract::{Multipart, State},
    http::StatusCode,
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde_json::json;
use std::path::Path;
use tokio::fs;

use super::{ApiError, AppState, SessionSubject, StoreRequest, StoreResponse, store_entry_core};

pub async fn ingest_image(
    State(state): State<AppState>,
    Extension(session): Extension<SessionSubject>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<StoreResponse>), ApiError> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut source_id = "images".to_owned();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_owned();
        match name.as_str() {
            "file" => {
                filename = field.file_name().map(ToOwned::to_owned);
                file_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| ApiError::BadRequest(e.to_string()))?
                        .to_vec(),
                );
            }
            "source_id" => {
                source_id = field
                    .text()
                    .await
                    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
            }
            _ => {}
        }
    }

    let bytes =
        file_bytes.ok_or_else(|| ApiError::BadRequest("missing 'file' field".to_owned()))?;
    let original_name = filename.unwrap_or_else(|| "image.jpg".to_owned());

    let ext = Path::new(&original_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg")
        .to_lowercase();

    let item_id = uuid::Uuid::now_v7().to_string();
    let stored_name = format!("{item_id}.{ext}");

    fs::create_dir_all("assets")
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    fs::write(format!("assets/{stored_name}"), &bytes)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let extracted_text = extract_image_text(&state, &bytes, &ext).await?;

    let metadata = json!({
        "source_type": "image",
        "source_file": format!("/assets/{stored_name}"),
        "original_filename": original_name,
    });

    let store_req = StoreRequest {
        id: Some(item_id),
        text: extracted_text,
        metadata,
        source_id,
        chunk: None,
    };

    let response = store_entry_core(&state, store_req, session.0).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn extract_image_text(
    state: &AppState,
    bytes: &[u8],
    ext: &str,
) -> Result<String, ApiError> {
    // Prefer dedicated multimodal config; fall back to the default LLM endpoint.
    let (base_url, api_key, model, client) = if state.multimodal.is_configured() {
        (
            state.multimodal.base_url.as_deref().unwrap(),
            state.multimodal.api_key.as_deref(),
            state.multimodal.model.as_deref().unwrap_or("qwen2.5-vl"),
            &state.multimodal_client,
        )
    } else if state.openai_chat.is_configured() {
        (
            state.openai_chat.base_url.as_deref().unwrap(),
            state.openai_chat.api_key.as_deref(),
            state.openai_chat.default_model.as_deref().unwrap_or("gpt-4o"),
            &state.http_client,
        )
    } else {
        return Err(ApiError::ServiceUnavailable(
            "no vision-capable LLM configured (set RAG_MULTIMODAL_BASE_URL or RAG_OPENAI_API_BASE_URL)".to_owned(),
        ));
    };

    let mime = match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "image/jpeg",
    };

    let data_url = format!("data:{mime};base64,{}", BASE64.encode(bytes));

    let body = json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": [
                {
                    "type": "image_url",
                    "image_url": { "url": data_url }
                },
                {
                    "type": "text",
                    "text": "Extract and describe all content from this image comprehensively. Include all visible text verbatim, describe diagrams, charts, tables, and visual elements in detail. Be thorough and structured."
                }
            ]
        }],
        "max_tokens": 2000
    });

    let url = format!("{base_url}/chat/completions");
    let mut builder = client.post(&url).json(&body);
    if let Some(key) = api_key {
        builder = builder.header("Authorization", format!("Bearer {key}"));
    }

    let response: serde_json::Value = builder
        .send()
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .json()
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    response["choices"][0]["message"]["content"]
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            ApiError::Internal(anyhow::anyhow!(
                "unexpected LLM response format: {response}"
            ))
        })
}
