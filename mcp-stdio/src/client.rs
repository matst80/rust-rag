use anyhow::{Context, Result, anyhow};
use reqwest::{
    Client, Method, StatusCode, Url,
    header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue},
};
use rust_rag::{
    api::{
        AdminItemPayload, AdminItemsResponse, CategoriesResponse, CreateManualEdgeRequest,
        DeleteResponse, GraphEdgesResponse, GraphNeighborhoodQuery, GraphNeighborhoodResponse,
        GraphRebuildResponse, GraphStatusResponse, HealthResponse, ListGraphEdgesQuery,
        ListItemsQuery, SearchRequest, SearchResponse, StoreRequest, StoreResponse,
        UpdateItemRequest,
    },
};
use serde::de::DeserializeOwned;
use std::time::Duration;

use crate::config::HeaderConfig;

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
}

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    pub base_url: String,
    pub timeout: Duration,
    pub auth_bearer: Option<String>,
    pub headers: Vec<HeaderConfig>,
}

#[derive(Clone)]
pub struct RustRagHttpClient {
    base_url: Url,
    http: Client,
}

impl RustRagHttpClient {
    pub fn new(config: HttpClientConfig) -> Result<Self> {
        let mut default_headers = HeaderMap::new();
        for header in config.headers {
            let name = HeaderName::from_bytes(header.name.as_bytes())
                .with_context(|| format!("invalid header name {}", header.name))?;
            let value = HeaderValue::from_str(&header.value)
                .with_context(|| format!("invalid header value for {}", name))?;
            default_headers.insert(name, value);
        }

        if let Some(token) = config.auth_bearer {
            let mut value = HeaderValue::from_str(&format!("Bearer {token}"))
                .context("invalid bearer token")?;
            value.set_sensitive(true);
            default_headers.insert(AUTHORIZATION, value);
        }

        let http = Client::builder()
            .default_headers(default_headers)
            .timeout(config.timeout)
            .build()
            .context("failed to build HTTP client")?;
        let base_url = ensure_base_url(&config.base_url)?;

        Ok(Self { base_url, http })
    }

    pub async fn health(&self) -> Result<HealthResponse> {
        let response = self
            .http
            .get(self.url("healthz")?)
            .send()
            .await
            .context("health request failed")?;

        match response.status() {
            StatusCode::OK | StatusCode::SERVICE_UNAVAILABLE => response
                .json::<HealthResponse>()
                .await
                .context("failed to decode health response"),
            _ => Err(response_error(response).await),
        }
    }

    pub async fn store(&self, request: &StoreRequest) -> Result<StoreResponse> {
        self.send_json(Method::POST, "store", Some(request), None::<&()>)
            .await
    }

    pub async fn search(&self, request: &SearchRequest) -> Result<SearchResponse> {
        self.send_json(Method::POST, "search", Some(request), None::<&()>)
            .await
    }

    pub async fn list_categories(&self) -> Result<CategoriesResponse> {
        self.send_json::<(), (), CategoriesResponse>(Method::GET, "admin/categories", None, None)
            .await
    }

    pub async fn list_items(&self, query: &ListItemsQuery) -> Result<AdminItemsResponse> {
        self.send_json(Method::GET, "admin/items", None::<&()>, Some(query))
            .await
    }

    pub async fn update_item(&self, id: &str, request: &UpdateItemRequest) -> Result<AdminItemPayload> {
        self.send_json(Method::PUT, &format!("admin/items/{id}"), Some(request), None::<&()>)
            .await
    }

    pub async fn delete_item(&self, id: &str) -> Result<DeleteResponse> {
        self.send_json::<(), (), DeleteResponse>(Method::DELETE, &format!("admin/items/{id}"), None, None)
            .await
    }

    pub async fn graph_status(&self) -> Result<GraphStatusResponse> {
        self.send_json::<(), (), GraphStatusResponse>(Method::GET, "graph/status", None, None)
            .await
    }

    pub async fn list_graph_edges(&self, query: &ListGraphEdgesQuery) -> Result<GraphEdgesResponse> {
        self.send_json(Method::GET, "graph/edges", None::<&()>, Some(query))
            .await
    }

    pub async fn graph_neighborhood(
        &self,
        id: &str,
        query: &GraphNeighborhoodQuery,
    ) -> Result<GraphNeighborhoodResponse> {
        self.send_json(
            Method::GET,
            &format!("graph/neighborhood/{id}"),
            None::<&()>,
            Some(query),
        )
        .await
    }

    pub async fn rebuild_graph(&self) -> Result<GraphRebuildResponse> {
        self.send_json::<(), (), GraphRebuildResponse>(Method::POST, "admin/graph/rebuild", None, None)
            .await
    }

    pub async fn create_manual_edge(
        &self,
        request: &CreateManualEdgeRequest,
    ) -> Result<rust_rag::api::GraphEdgePayload> {
        self.send_json(Method::POST, "admin/graph/edges", Some(request), None::<&()>)
            .await
    }

    pub async fn delete_graph_edge(&self, id: &str) -> Result<DeleteResponse> {
        self.send_json::<(), (), DeleteResponse>(Method::DELETE, &format!("admin/graph/edges/{id}"), None, None)
            .await
    }

    async fn send_json<Body, Query, Response>(
        &self,
        method: Method,
        path: &str,
        body: Option<&Body>,
        query: Option<&Query>,
    ) -> Result<Response>
    where
        Body: serde::Serialize + ?Sized,
        Query: serde::Serialize + ?Sized,
        Response: DeserializeOwned,
    {
        let mut builder = self.http.request(method, self.url(path)?);
        if let Some(query) = query {
            builder = builder.query(query);
        }
        if let Some(body) = body {
            builder = builder.json(body);
        }

        let response = builder
            .send()
            .await
            .with_context(|| format!("request to {path} failed"))?;
        if !response.status().is_success() {
            return Err(response_error(response).await);
        }

        response
            .json::<Response>()
            .await
            .with_context(|| format!("failed to decode response body for {path}"))
    }

    fn url(&self, path: &str) -> Result<Url> {
        self.base_url
            .join(path)
            .with_context(|| format!("failed to join path {path}"))
    }
}

fn ensure_base_url(value: &str) -> Result<Url> {
    let mut url = Url::parse(value).with_context(|| format!("invalid base URL {value}"))?;
    if !url.path().ends_with('/') {
        let next = format!("{}/", url.path());
        url.set_path(&next);
    }
    Ok(url)
}

async fn response_error(response: reqwest::Response) -> anyhow::Error {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if let Ok(error) = serde_json::from_str::<ErrorResponse>(&body) {
        return anyhow!("HTTP {status}: {}", error.error);
    }
    if body.trim().is_empty() {
        return anyhow!("HTTP {status}");
    }
    anyhow!("HTTP {status}: {body}")
}

#[cfg(test)]
mod tests {
    use super::{HttpClientConfig, RustRagHttpClient};
    use axum::{
        Json, Router,
        extract::{Path, Query, State},
        http::StatusCode,
        routing::{delete, get, post, put},
    };
    use rust_rag::api::{
        AdminItemPayload, CreateManualEdgeRequest, DeleteResponse, GraphNeighborhoodQuery,
        GraphNeighborhoodResponse, HealthResponse, ListItemsQuery, SearchRequest, SearchResponse,
        StoreRequest, StoreResponse, UpdateItemRequest,
    };
    use serde_json::{Value, json};
    use std::{net::SocketAddr, sync::{Arc, Mutex}, time::Duration};
    use tokio::{net::TcpListener, task::JoinHandle};

    #[derive(Clone, Default)]
    struct TestState {
        requests: Arc<Mutex<Vec<String>>>,
    }

    struct TestServer {
        address: SocketAddr,
        task: JoinHandle<()>,
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    #[tokio::test]
    async fn store_posts_json_body() {
        let state = TestState::default();
        let server = spawn_server(
            Router::new().route(
                "/store",
                post({
                    let state = state.clone();
                    move |Json(request): Json<StoreRequest>| {
                        let state = state.clone();
                        async move {
                            state.requests.lock().unwrap().push(request.text.clone());
                            (
                                StatusCode::CREATED,
                                Json(StoreResponse {
                                    id: request.id.unwrap_or_else(|| "generated".to_owned()),
                                    source_id: request.source_id,
                                    created_at: 123,
                                }),
                            )
                        }
                    }
                }),
            ),
        )
        .await;

        let client = client_for(&server);
        let response = client
            .store(&StoreRequest {
                id: Some("item-1".to_owned()),
                text: "hello".to_owned(),
                metadata: json!({"kind": "note"}),
                source_id: "docs".to_owned(),
            })
            .await
            .unwrap();

        assert_eq!(response.id, "item-1");
        assert_eq!(state.requests.lock().unwrap().as_slice(), ["hello"]);
    }

    #[tokio::test]
    async fn update_item_surfaces_api_errors() {
        let server = spawn_server(Router::new().route(
            "/admin/items/{id}",
            put(|| async { (StatusCode::NOT_FOUND, Json(json!({"error": "item missing"}))) }),
        ))
        .await;

        let client = client_for(&server);
        let error = client
            .update_item(
                "missing",
                &UpdateItemRequest {
                    text: "hello".to_owned(),
                    metadata: json!({}),
                    source_id: "docs".to_owned(),
                },
            )
            .await
            .unwrap_err();

        assert!(error.to_string().contains("item missing"));
    }

    #[tokio::test]
    async fn graph_neighborhood_serializes_query_string() {
        let state = TestState::default();
        let server = spawn_server(
            Router::new().route(
                "/graph/neighborhood/{id}",
                get({
                    let state = state.clone();
                    move |Path(id): Path<String>, Query(query): Query<GraphNeighborhoodQuery>| {
                        let state = state.clone();
                        async move {
                            state
                                .requests
                                .lock()
                                .unwrap()
                                .push(format!("{id}:{}:{}", query.depth.unwrap_or_default(), query.limit.unwrap_or_default()));
                            Json(GraphNeighborhoodResponse {
                                center_id: id,
                                nodes: Vec::new(),
                                edges: Vec::new(),
                            })
                        }
                    }
                }),
            ),
        )
        .await;

        let client = client_for(&server);
        let response = client
            .graph_neighborhood(
                "node-1",
                &GraphNeighborhoodQuery {
                    depth: Some(2),
                    limit: Some(25),
                    edge_type: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(response.center_id, "node-1");
        assert_eq!(state.requests.lock().unwrap().as_slice(), ["node-1:2:25"]);
    }

    fn client_for(server: &TestServer) -> RustRagHttpClient {
        RustRagHttpClient::new(HttpClientConfig {
            base_url: format!("http://{}", server.address),
            timeout: Duration::from_secs(5),
            auth_bearer: None,
            headers: Vec::new(),
        })
        .unwrap()
    }

    async fn spawn_server(router: Router) -> TestServer {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        TestServer { address, task }
    }
}