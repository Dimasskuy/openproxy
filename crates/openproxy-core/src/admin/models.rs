//! Model administration service layer.

use crate::error::{CoreError, Result};
use crate::ids::{ModelId, ModelRowId, ProviderId};
use crate::models;
use crate::providers;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Known models for a provider, optionally filtered. `provider = None` returns
/// every row in the `models` table.
pub fn list_models(conn: &Connection, provider: Option<&ProviderId>) -> Result<Vec<models::Model>> {
    match provider {
        Some(p) => Ok(models::list_all(conn)?
            .into_iter()
            .filter(|m| &m.provider_id == p)
            .collect()),
        None => models::list_all(conn),
    }
}

/// Inputs for [`create_custom_model`], distinct from the adapter-driven
/// [`refresh_models`] path: the operator hand-picks `(provider_id, model_id)`,
/// `display_name`, the upstream's `target_format` wire format, and a
/// `ttl_seconds` cache lifetime (`0` never expires).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCustomModelInput {
    pub provider_id: String,
    pub model_id: String,
    pub display_name: Option<String>,
    /// `"openai"` or `"anthropic"`. Anything else surfaces as
    /// [`CoreError::Validation`].
    pub target_format: String,
    pub ttl_seconds: i64,
    #[serde(default)]
    pub model_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateModelInput {
    pub display_name: Option<String>,
    pub model_type: Option<String>,
    pub target_format: Option<String>,
}

/// Create a hand-picked model row, returning the id of the new (or upserted) row.
/// See [`models::create_custom`] for the SQL semantics.
pub fn create_custom_model(conn: &Connection, input: CreateCustomModelInput) -> Result<ModelRowId> {
    let provider = ProviderId::new(input.provider_id);
    let model = ModelId::new(input.model_id);
    let target_format = models::TargetFormat::parse(&input.target_format)?;
    models::create_custom(
        conn,
        &provider,
        &model,
        input.display_name.as_deref(),
        target_format,
        input.ttl_seconds,
        input.model_type.as_deref(),
    )
}

/// Update display_name, model_type and target_format on an existing model row.
pub fn update_model(conn: &Connection, id: ModelRowId, input: UpdateModelInput) -> Result<()> {
    let target_format = if let Some(tf) = input.target_format.as_deref() {
        Some(models::TargetFormat::parse(tf)?)
    } else {
        None
    };
    models::update_model_details(
        conn,
        id,
        input.display_name.as_deref(),
        input.model_type.as_deref(),
        target_format,
    )
}

/// Fetch a provider's model list through the adapter and upsert the results.
///
/// The caller resolves the adapter, decrypts an account's API key into plaintext,
/// and supplies the shared upstream client and `ttl_seconds`.
///
/// Returns [`models::UpsertResult`] with the touched count and the newly inserted
/// `model_id`s, or a [`CoreError`] describing the upstream or DB failure.
///
/// Lock safety: the provider existence check runs without the writer lock, the
/// HTTP fetch holds no database lock, and the write goes through `spawn_blocking`
/// on the pool's writer.
pub async fn refresh_models<A: openproxy_adapters::adapters::ProviderAdapter>(
    pool: &openproxy_db::DbPool,
    provider: &ProviderId,
    api_key: &str,
    adapter: &A,
    upstream_client: &std::sync::Arc<openproxy_adapters::upstream::UpstreamClient>,
    ttl_seconds: i64,
    account_label: &str,
) -> Result<models::UpsertResult> {
    let pool_reader = pool.clone();
    let provider_clone = provider.clone();
    let provider_row = tokio::task::spawn_blocking(move || {
        let r = pool_reader.reader();
        providers::get(&r, &provider_clone)
    })
    .await
    .map_err(|e| CoreError::Internal(format!("join error: {e}")))??;

    if provider_row.is_none() {
        return Err(CoreError::ProviderNotFound(provider.to_string()));
    }

    let discovered = adapter
        .fetch_models_for_account(upstream_client, api_key, account_label)
        .await?;
    if discovered.is_empty() {
        return Err(CoreError::UpstreamConnection(format!(
            "provider {provider} returned 0 models on /models; skipping update to preserve existing catalog"
        )));
    }
    let ttl = Duration::from_secs(ttl_seconds.max(0) as u64);
    let pool_writer = pool.clone();
    let provider_clone = provider.clone();
    tokio::task::spawn_blocking(move || {
        let conn = pool_writer.writer();
        if providers::get(&conn, &provider_clone)?.is_none() {
            return Err(CoreError::ProviderNotFound(provider_clone.to_string()));
        }
        models::upsert_many(&conn, &provider_clone, &discovered, ttl)
    })
    .await
    .map_err(|e| CoreError::Internal(format!("join error: {e}")))?
}

/// Inputs for [`set_active_bulk`], sent by the "Enable all" / "Disable all" buttons
/// to toggle every non-custom row of the provider in one UPDATE.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkToggleInput {
    pub provider_id: String,
    pub active: bool,
}

/// Bulk set `active` for all non-custom models of a provider. One
/// `UPDATE ... WHERE provider_id = ? AND custom = 0` flips every row at once, so a
/// concurrent `apply_auto_activation` cannot interleave and leave the table
/// half-toggled.
///
/// Returns the updated row count. A missing provider matches nothing.
pub fn set_active_bulk(conn: &Connection, input: BulkToggleInput) -> Result<u64> {
    let provider = ProviderId::new(input.provider_id);
    models::set_active_bulk(conn, &provider, input.active)
}
