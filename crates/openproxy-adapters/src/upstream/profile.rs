//! Timeout profiles — per-use-type defaults, overrideable via `Custom`.
//!
//! Each `TimeoutProfile` variant maps to a coarse use type (chat, quota, oauth,
//! ...) and resolves to a `ResolvedTimeouts` holding a value for every pipeline
//! phase. `TimeoutProfile::Custom` overrides any value.
//!
//! The numbers mirror `TimeoutsConfig` in `crates/openproxy-core/src/timeouts.rs`
//! so the profile path and the legacy UpstreamClient path behave identically.

/// Fully-resolved per-phase timeouts in milliseconds: `dns` (resolve), `dial`
/// (TCP connect), `tls` (handshake), `write` (request line + headers + body),
/// `headers` (wait for response headers, composes dial+tls+write+wait),
/// `body_chunk` (max gap between two body chunks) and `total` (hard ceiling).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedTimeouts {
    pub dns_ms: u64,
    pub dial_ms: u64,
    pub tls_ms: u64,
    pub write_ms: u64,
    pub headers_ms: u64,
    pub body_chunk_ms: u64,
    pub total_ms: u64,
}

impl ResolvedTimeouts {
    /// Applied when no profile override is given. Kept equal to the
    /// `TimeoutsConfig` defaults in `timeouts.rs::Timeouts::from_config`
    /// (connect=5s, request_send=10s, ttft=30s, idle_chunk=120s, total=300s).
    pub const SYSTEM_DEFAULTS: Self = Self {
        dns_ms: 5_000,
        dial_ms: 5_000,         // == `connect_ms` system default
        tls_ms: 5_000,          // rolled into `connect_ms` for backward compat
        write_ms: 10_000,       // == `request_send_ms` system default
        headers_ms: 6_000,      // == `ttft_ms` system default (wait-for-headers / TTFT)
        body_chunk_ms: 120_000, // == `idle_chunk_ms` system default
        total_ms: 300_000,      // == `total_ms` system default
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutProfile {
    /// Tighter `headers_ms` than the system default, to fail fast on a dead
    /// upstream.
    Chat,
    Quota,
    OAuth,
    ModelDiscovery,
    ImageGeneration,
    Custom(ResolvedTimeouts),
}

impl TimeoutProfile {
    const CHAT: ResolvedTimeouts = ResolvedTimeouts {
        headers_ms: 6_000,
        body_chunk_ms: 90_000,
        ..ResolvedTimeouts::SYSTEM_DEFAULTS
    };

    const QUOTA: ResolvedTimeouts = ResolvedTimeouts {
        dns_ms: 3_000,
        dial_ms: 3_000,
        tls_ms: 3_000,
        write_ms: 5_000,
        headers_ms: 8_000,
        body_chunk_ms: 8_000,
        total_ms: 15_000,
    };

    const OAUTH: ResolvedTimeouts = ResolvedTimeouts {
        dns_ms: 2_000,
        dial_ms: 2_000,
        tls_ms: 2_000,
        write_ms: 3_000,
        headers_ms: 5_000,
        body_chunk_ms: 1_000,
        total_ms: 10_000,
    };

    const MODEL_DISCOVERY: ResolvedTimeouts = ResolvedTimeouts {
        headers_ms: 30_000,
        body_chunk_ms: 60_000,
        total_ms: 120_000,
        ..ResolvedTimeouts::SYSTEM_DEFAULTS
    };

    const IMAGE_GENERATION: ResolvedTimeouts = ResolvedTimeouts {
        headers_ms: 30_000,
        body_chunk_ms: 300_000,
        total_ms: 600_000,
        ..ResolvedTimeouts::SYSTEM_DEFAULTS
    };

    const fn builtin_timeouts(&self) -> ResolvedTimeouts {
        match self {
            TimeoutProfile::Chat => Self::CHAT,
            TimeoutProfile::Quota => Self::QUOTA,
            TimeoutProfile::OAuth => Self::OAUTH,
            TimeoutProfile::ModelDiscovery => Self::MODEL_DISCOVERY,
            _ => Self::IMAGE_GENERATION,
        }
    }

    pub const fn resolve(&self) -> ResolvedTimeouts {
        match self {
            TimeoutProfile::Custom(t) => *t,
            _ => self.builtin_timeouts(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_resolves_with_ttft_and_chunk_overrides() {
        let t = TimeoutProfile::Chat.resolve();
        assert_eq!(t.headers_ms, 6_000);
        assert_eq!(t.body_chunk_ms, 90_000);
        assert_eq!(t.total_ms, 300_000);
        assert_eq!(t.dns_ms, 5_000);
    }

    #[test]
    fn oauth_is_aggressive() {
        let t = TimeoutProfile::OAuth.resolve();
        assert!(t.total_ms <= 10_000);
        assert!(t.headers_ms <= 5_000);
    }

    #[test]
    fn custom_passes_through() {
        let custom = ResolvedTimeouts {
            dns_ms: 1,
            dial_ms: 2,
            tls_ms: 3,
            write_ms: 4,
            headers_ms: 5,
            body_chunk_ms: 6,
            total_ms: 7,
        };
        let resolved = TimeoutProfile::Custom(custom).resolve();
        assert_eq!(resolved, custom);
    }
}
