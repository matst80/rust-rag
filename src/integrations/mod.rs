//! Third-party service integrations.
//!
//! Provider-specific clients live in their own submodules. Each client takes
//! the authenticated subject + AppState handle and is responsible for loading
//! the user's stored OAuth credentials, decrypting them, refreshing expired
//! tokens, and persisting rotated tokens back through the OAuthCredsStore.
//!
//! The HTTP OAuth flow handlers (start/callback/status/disconnect) live in
//! `crate::api::integrations::*` — that module is for HTTP route handlers
//! only; this one is for the data-plane clients that read the vault and
//! call third-party APIs.

pub mod google;
