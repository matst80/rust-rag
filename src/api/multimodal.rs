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
    tracing::info!("starting image ingestion");
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut source_id = "images".to_owned();
    let mut metadata_extra: Option<serde_json::Value> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| {
            tracing::error!("failed to read multipart field: {e}");
            ApiError::BadRequest(e.to_string())
        })?
    {
        let name = field.name().unwrap_or("").to_owned();
        match name.as_str() {
            "file" => {
                filename = field.file_name().map(ToOwned::to_owned);
                tracing::debug!(filename = ?filename, "received file field");
                file_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| {
                            tracing::error!("failed to read file bytes: {e}");
                            ApiError::BadRequest(e.to_string())
                        })?
                        .to_vec(),
                );
            }
            "source_id" => {
                source_id = field
                    .text()
                    .await
                    .map_err(|e| {
                        tracing::error!("failed to read source_id text: {e}");
                        ApiError::BadRequest(e.to_string())
                    })?;
                tracing::debug!(source_id = %source_id, "received source_id field");
            }
            "metadata" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| {
                        tracing::error!("failed to read metadata text: {e}");
                        ApiError::BadRequest(e.to_string())
                    })?;
                metadata_extra = Some(serde_json::from_str(&text).map_err(|e| {
                    tracing::error!("failed to parse metadata JSON: {e}");
                    ApiError::BadRequest(format!("invalid metadata JSON: {e}"))
                })?);
                tracing::debug!("received metadata field");
            }
            _ => {
                tracing::debug!(field_name = %name, "ignoring unknown multipart field");
            }
        }
    }

    let bytes = file_bytes.ok_or_else(|| {
        tracing::warn!("missing 'file' field in multipart request");
        ApiError::BadRequest("missing 'file' field".to_owned())
    })?;
    let original_name = filename.unwrap_or_else(|| "image.jpg".to_owned());

    let ext = Path::new(&original_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg")
        .to_lowercase();

    let item_id = uuid::Uuid::now_v7().to_string();
    let stored_name = format!("{item_id}.{ext}");
    let upload_dir = state.upload_path.as_str();

    tracing::info!(upload_dir = %upload_dir, stored_name = %stored_name, "saving uploaded image to disk");
    fs::create_dir_all(upload_dir)
        .await
        .map_err(|e| {
            let err = anyhow::anyhow!("failed to create upload dir {upload_dir:?}: {e}");
            tracing::error!("{err}");
            ApiError::Internal(err)
        })?;
    fs::write(format!("{upload_dir}/{stored_name}"), &bytes)
        .await
        .map_err(|e| {
            let err = anyhow::anyhow!("failed to write {stored_name}: {e}");
            tracing::error!("{err}");
            ApiError::Internal(err)
        })?;

    tracing::info!("extracting text from image via LLM");
    let extracted_text = extract_image_text(&state, &bytes, &ext).await?;
    tracing::debug!(extracted_len = extracted_text.len(), "successfully extracted text from image");

    let mut metadata = json!({
        "source_type": "image",
        "source_file": format!("/assets/{stored_name}"),
        "original_filename": original_name,
    });

    if let Some(extra) = metadata_extra {
        if let Some(obj) = extra.as_object() {
            for (k, v) in obj {
                metadata[k] = v.clone();
            }
        }
    }

    let store_req = StoreRequest {
        id: Some(item_id),
        text: extracted_text,
        metadata,
        source_id,
        chunk: None,
    };

    tracing::info!("storing image metadata and extracted text in vector store");
    let response = store_entry_core(&state, store_req, session.0).await.map_err(|e| {
        tracing::error!("failed to store image entry: {e}");
        e
    })?;
    
    tracing::info!(item_id = %response.id, "image ingestion completed successfully");
    Ok((StatusCode::CREATED, Json(response)))
}

async fn extract_image_text(
    state: &AppState,
    bytes: &[u8],
    ext: &str,
) -> Result<String, ApiError> {
    // Prefer dedicated multimodal config; fall back to the default LLM endpoint.
    let (base_url, api_key, model, client) = if state.multimodal.is_configured() {
        tracing::debug!("using dedicated multimodal configuration");
        (
            state.multimodal.base_url.as_deref().unwrap(),
            state.multimodal.api_key.as_deref(),
            state.multimodal.model.as_deref().unwrap_or("qwen2.5-vl"),
            &state.multimodal_client,
        )
    } else if state.openai_chat.is_configured() {
        tracing::debug!("using default OpenAI chat configuration for vision");
        (
            state.openai_chat.base_url.as_deref().unwrap(),
            state.openai_chat.api_key.as_deref(),
            state.openai_chat.default_model.as_deref().unwrap_or("gpt-4o"),
            &state.http_client,
        )
    } else {
        let err_msg = "no vision-capable LLM configured (set RAG_MULTIMODAL_BASE_URL or RAG_OPENAI_API_BASE_URL)";
        tracing::error!(err_msg);
        return Err(ApiError::ServiceUnavailable(err_msg.to_owned()));
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
    tracing::debug!(url = %url, model = %model, "sending request to multimodal LLM");
    
    let mut builder = client.post(&url).json(&body);
    if let Some(key) = api_key {
        builder = builder.header("Authorization", format!("Bearer {key}"));
    }

    let response_res = builder.send().await;
    
    let response_body = match response_res {
        Ok(resp) => {
            let status = resp.status();
            if !status.is_success() {
                let error_text = resp.text().await.unwrap_or_else(|_| "could not read error body".to_owned());
                tracing::error!(status = %status, error_body = %error_text, "LLM request failed");
                return Err(ApiError::Internal(anyhow::anyhow!("LLM request failed with status {status}: {error_text}")));
            }
            resp.json::<serde_json::Value>().await.map_err(|e| {
                tracing::error!("failed to parse LLM response JSON: {e}");
                ApiError::Internal(e.into())
            })?
        }
        Err(e) => {
            tracing::error!("failed to send request to LLM: {e}");
            return Err(ApiError::Internal(e.into()));
        }
    };

    response_body["choices"][0]["message"]["content"]
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            let err = anyhow::anyhow!("unexpected LLM response format: {response_body}");
            tracing::error!("{err}");
            ApiError::Internal(err)
        })
}
