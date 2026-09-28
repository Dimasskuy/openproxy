//! Exporting OpenProxy state to a self-contained backup bundle.

use super::crypto::encrypt_bundle_payload;
use openproxy_db::MasterKey;
use openproxy_types::backup::{
    BACKUP_FORMAT_VERSION, BackupAccount, BackupApiKey, BackupAppConfig, BackupBundle, BackupCombo,
    BackupComboTarget, BackupModel, BackupPayload, BackupProvider, BackupProxySource,
};
use openproxy_types::{
    CoreError, ModelId, ProviderFormat, ProviderId, RateLimitScope, Result, TargetFormat,
    combos::{CooldownMode, PriorityMode, Strategy},
    providers::AuthType,
};
use rusqlite::Connection;

/// Decrypt an encrypted BLOB using [`MasterKey`], ignoring errors defensively.
fn decrypt_blob_opt(master_key: &MasterKey, blob_opt: Option<Vec<u8>>) -> Option<String> {
    let blob = blob_opt?;
    if blob.is_empty() {
        return None;
    }
    master_key.decrypt(&blob).ok()
}

/// Decrypt an OAuth token encrypted BLOB.
fn decrypt_token_opt(master_key: &MasterKey, blob_opt: Option<Vec<u8>>) -> Option<String> {
    decrypt_blob_opt(master_key, blob_opt)
}

/// Decrypt oauth_provider_specific string if present.
fn decrypt_oauth_specific(master_key: &MasterKey, val_opt: Option<String>) -> Option<String> {
    let val = val_opt?;
    if val.is_empty() {
        return None;
    }
    // If it is base64 encoded ciphertext:
    if let Ok(blob) = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &val)
        && let Ok(decrypted) = master_key.decrypt(&blob)
    {
        Some(decrypted)
    } else {
        Some(val)
    }
}

pub fn export_backup(
    conn: &Connection,
    master_key: &MasterKey,
    passphrase: Option<&str>,
) -> Result<BackupBundle> {
    // 1. Providers
    let mut stmt = conn
        .prepare(
            "SELECT id, name, base_url, auth_type, format, extra_headers_json, \
             auto_activate_keyword, active, use_proxies, current_proxy_id, \
             proxy_rotation_errors, rate_limit_scope, notif_keyword_only, \
             proxy_rotation_mode FROM providers ORDER BY id",
        )
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?;

    let providers = stmt
        .query_map([], |row| {
            let auth_str: String = row.get(3)?;
            let fmt_str: String = row.get(4)?;
            let scope_str: String = row.get(11)?;

            let auth_type = AuthType::parse(&auth_str).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
                )
            })?;
            let format = ProviderFormat::parse(&fmt_str).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    4,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
                )
            })?;
            let rate_limit_scope = RateLimitScope::parse(&scope_str).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    11,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
                )
            })?;

            Ok(BackupProvider {
                id: ProviderId::new(row.get::<_, String>(0)?),
                name: row.get(1)?,
                base_url: row.get(2)?,
                auth_type,
                format,
                extra_headers_json: row.get(5)?,
                auto_activate_keyword: row.get(6)?,
                active: row.get::<_, i64>(7)? != 0,
                use_proxies: row.get::<_, i64>(8)? != 0,
                current_proxy_id: row.get(9)?,
                proxy_rotation_errors: row.get(10)?,
                rate_limit_scope,
                notif_keyword_only: row.get::<_, i64>(12)? != 0,
                proxy_rotation_mode: row.get(13)?,
            })
        })
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?;

    // 2. Accounts
    let mut stmt = conn
        .prepare(
            "SELECT id, provider_id, api_key_encrypted, label, priority, \
             extra_config_json, health_status, rate_limited_until, auth_type, \
             email, oauth_scope, oauth_provider_specific, expires_at, \
             access_token_encrypted, refresh_token_encrypted, current_proxy_id \
             FROM accounts ORDER BY id",
        )
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?;

    let accounts = stmt
        .query_map([], |row| {
            let id: i64 = row.get(0)?;
            let provider_id_str: String = row.get(1)?;
            let api_key_encrypted: Option<Vec<u8>> = row.get(2)?;
            let label: Option<String> = row.get(3)?;
            let priority: i32 = row.get(4)?;
            let extra_config_json: Option<String> = row.get(5)?;
            let health_status: String = row.get(6)?;
            let rate_limited_until: Option<String> = row.get(7)?;
            let auth_type: Option<String> = row.get(8)?;
            let email: Option<String> = row.get(9)?;
            let oauth_scope: Option<String> = row.get(10)?;
            let oauth_provider_specific: Option<String> = row.get(11)?;
            let expires_at: Option<String> = row.get(12)?;
            let access_token_encrypted: Option<Vec<u8>> = row.get(13)?;
            let refresh_token_encrypted: Option<Vec<u8>> = row.get(14)?;
            let current_proxy_id: Option<String> = row.get(15)?;

            let api_key = decrypt_blob_opt(master_key, api_key_encrypted);
            let access_token = decrypt_token_opt(master_key, access_token_encrypted);
            let refresh_token = decrypt_token_opt(master_key, refresh_token_encrypted);
            let oauth_specific = decrypt_oauth_specific(master_key, oauth_provider_specific);

            Ok(BackupAccount {
                id,
                provider_id: ProviderId::new(provider_id_str),
                api_key,
                label,
                priority,
                extra_config_json,
                health_status,
                rate_limited_until,
                auth_type,
                email,
                oauth_scope,
                oauth_provider_specific: oauth_specific,
                expires_at,
                access_token,
                refresh_token,
                current_proxy_id,
            })
        })
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?;

    // 3. Models
    let mut stmt = conn
        .prepare(
            "SELECT id, provider_id, model_id, display_name, target_format, \
             timeout_overrides_json, active, custom, context_length, \
             max_output_tokens, capabilities_json, family, model_type, \
             input_modalities_json, output_modalities_json, manually_disabled_at \
             FROM models ORDER BY id",
        )
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?;

    let models = stmt
        .query_map([], |row| {
            let fmt_str: String = row.get(4)?;
            let target_format = TargetFormat::parse(&fmt_str).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    4,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
                )
            })?;

            Ok(BackupModel {
                id: row.get(0)?,
                provider_id: ProviderId::new(row.get::<_, String>(1)?),
                model_id: ModelId::new(row.get::<_, String>(2)?),
                display_name: row.get(3)?,
                target_format,
                timeout_overrides_json: row.get(5)?,
                active: row.get::<_, i64>(6)? != 0,
                custom: row.get::<_, i64>(7)? != 0,
                context_length: row.get(8)?,
                max_output_tokens: row.get(9)?,
                capabilities_json: row.get(10)?,
                family: row.get(11)?,
                model_type: row.get(12)?,
                input_modalities_json: row.get(13)?,
                output_modalities_json: row.get(14)?,
                manually_disabled_at: row.get(15)?,
            })
        })
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?;

    // 4. Combos
    let mut stmt = conn
        .prepare(
            "SELECT id, name, strategy, race_size, preventive_rate_limit, \
             context_window, priority_mode, cooldown_mode, cooldown_base_secs, \
             cooldown_max_secs, cooldown_factor, lkgp_exploration_rate, \
             selection_window_secs, decision_model, decision_timeout_ms \
             FROM combos ORDER BY id",
        )
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?;

    let combos = stmt
        .query_map([], |row| {
            let strat_str: String = row.get(2)?;
            let strategy = Strategy::parse(&strat_str).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
                )
            })?;

            let prio_mode_str: Option<String> = row.get(6)?;
            let priority_mode = prio_mode_str
                .as_deref()
                .and_then(|s| PriorityMode::parse(s).ok())
                .unwrap_or_default();

            let cd_mode_str: Option<String> = row.get(7)?;
            let cooldown_mode = cd_mode_str
                .as_deref()
                .and_then(|s| CooldownMode::parse(s).ok())
                .unwrap_or_default();

            Ok(BackupCombo {
                id: row.get(0)?,
                name: row.get(1)?,
                strategy,
                race_size: row.get(3)?,
                preventive_rate_limit: row.get::<_, i64>(4)? != 0,
                context_window: row.get(5)?,
                priority_mode,
                cooldown_mode,
                cooldown_base_secs: row.get::<_, Option<i64>>(8)?.map(|v| v as u64),
                cooldown_max_secs: row.get::<_, Option<i64>>(9)?.map(|v| v as u64),
                cooldown_factor: row.get(10)?,
                lkgp_exploration_rate: row.get(11)?,
                selection_window_secs: row.get::<_, Option<i64>>(12)?.map(|v| v as u64),
                decision_model: row.get(13)?,
                decision_timeout_ms: row.get::<_, Option<i64>>(14)?.map(|v| v as u64),
            })
        })
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?;

    // 5. Combo targets
    let mut stmt = conn
        .prepare(
            "SELECT ct.id, ct.combo_id, ct.provider_id, ct.account_id, ct.model_row_id, \
             ct.sub_combo_id, ct.upstream_model_id, ct.priority_order, ct.weight, \
             ct.active, p.rate_limit_scope, ct.cooldown_mode, ct.cooldown_base_secs, \
             ct.cooldown_max_secs, ct.cooldown_factor, ct.thinking_effort, ct.description \
             FROM combo_targets ct \
             LEFT JOIN providers p ON p.id = ct.provider_id \
             ORDER BY ct.combo_id, ct.priority_order",
        )
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?;

    let combo_targets = stmt
        .query_map([], |row| {
            let scope_str: Option<String> = row.get(10)?;
            let rate_limit_scope = scope_str
                .as_deref()
                .and_then(|s| RateLimitScope::parse(s).ok())
                .unwrap_or(RateLimitScope::Account);

            let cd_str: Option<String> = row.get(11)?;
            let cooldown_mode = cd_str.as_deref().and_then(|s| CooldownMode::parse(s).ok());

            Ok(BackupComboTarget {
                id: row.get(0)?,
                combo_id: row.get(1)?,
                provider_id: ProviderId::new(row.get::<_, String>(2)?),
                account_id: row.get(3)?,
                model_row_id: row.get(4)?,
                sub_combo_id: row.get(5)?,
                upstream_model_id: row.get(6)?,
                priority_order: row.get(7)?,
                weight: row.get(8)?,
                active: row.get::<_, i64>(9)? != 0,
                rate_limit_scope,
                cooldown_mode,
                cooldown_base_secs: row.get::<_, Option<i64>>(12)?.map(|v| v as u64),
                cooldown_max_secs: row.get::<_, Option<i64>>(13)?.map(|v| v as u64),
                cooldown_factor: row.get(14)?,
                thinking_effort: row.get(15)?,
                description: row.get(16)?,
            })
        })
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?;

    // 6. Proxy sources
    let mut stmt = conn
        .prepare(
            "SELECT id, name, url, priority, active, is_builtin \
             FROM proxy_sources ORDER BY priority DESC, id ASC",
        )
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?;

    let proxy_sources = stmt
        .query_map([], |row| {
            Ok(BackupProxySource {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                priority: row.get(3)?,
                active: row.get::<_, i64>(4)? != 0,
                is_builtin: row.get::<_, i64>(5)? != 0,
            })
        })
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?;

    // 7. API Keys
    let mut stmt = conn
        .prepare(
            "SELECT id, key_hash, key_prefix, label, scopes_json, \
             allowed_models_json, allowed_combos_json, is_active, revoked_at, \
             expires_at, created_by, blacklisted_providers_json, \
             blacklisted_models_json FROM api_keys ORDER BY id",
        )
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?;

    let api_keys = stmt
        .query_map([], |row| {
            Ok(BackupApiKey {
                id: row.get(0)?,
                key_hash: row.get(1)?,
                key_prefix: row.get(2)?,
                label: row.get(3)?,
                scopes_json: row.get(4)?,
                allowed_models_json: row.get(5)?,
                allowed_combos_json: row.get(6)?,
                is_active: row.get::<_, i64>(7)? != 0,
                revoked_at: row.get(8)?,
                expires_at: row.get(9)?,
                created_by: row.get(10)?,
                blacklisted_providers_json: row.get(11)?,
                blacklisted_models_json: row.get(12)?,
            })
        })
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?;

    // 8. App config
    let mut stmt = conn
        .prepare("SELECT key, value, updated_at FROM app_config ORDER BY key")
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?;

    let app_config = stmt
        .query_map([], |row| {
            Ok(BackupAppConfig {
                key: row.get(0)?,
                value: row.get(1)?,
                updated_at: row.get(2)?,
            })
        })
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| CoreError::Database {
            message: e.to_string(),
            source: Some(std::sync::Arc::new(e)),
        })?;

    let payload = BackupPayload {
        providers,
        accounts,
        models,
        combos,
        combo_targets,
        proxy_sources,
        api_keys,
        app_config,
    };

    if let Some(passphrase) = passphrase
        && !passphrase.trim().is_empty()
    {
        encrypt_bundle_payload(&payload, passphrase)
    } else {
        Ok(BackupBundle {
            version: BACKUP_FORMAT_VERSION,
            exported_at: chrono::Utc::now().to_rfc3339(),
            openproxy_version: env!("CARGO_PKG_VERSION").to_string(),
            encrypted: false,
            kdf: None,
            kdf_salt: None,
            kdf_iterations: None,
            nonce: None,
            ciphertext: None,
            payload: Some(payload),
        })
    }
}
