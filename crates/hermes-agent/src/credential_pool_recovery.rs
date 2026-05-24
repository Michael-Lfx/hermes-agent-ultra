//! Credential pool recovery helpers — parity with Python `_pool_may_recover_from_rate_limit`.

use std::time::Duration;

use crate::credential_pool::CredentialPool;

/// Decide whether to wait for credential-pool rotation instead of falling back.
pub fn pool_may_recover_from_rate_limit(
    pool: Option<&CredentialPool>,
    provider: &str,
    base_url: Option<&str>,
) -> bool {
    let Some(pool) = pool else {
        return false;
    };
    if !pool.has_available() {
        return false;
    }
    // CloudCode / Gemini CLI quotas are account-wide — rotation cannot recover.
    if provider.eq_ignore_ascii_case("google-gemini-cli")
        || base_url
            .map(|u| u.starts_with("cloudcode-pa://"))
            .unwrap_or(false)
    {
        return false;
    }
    pool.active_key_count() > 1
}

/// Attempt in-turn credential rotation after rate limit; returns true if retry should proceed.
pub fn try_recover_with_credential_pool(
    pool: Option<&CredentialPool>,
    provider: &str,
    base_url: Option<&str>,
    has_retried_429_same_cred: bool,
) -> (bool, bool) {
    let Some(pool) = pool else {
        return (false, has_retried_429_same_cred);
    };
    if !pool_may_recover_from_rate_limit(Some(pool), provider, base_url) {
        return (false, has_retried_429_same_cred);
    }
    if !has_retried_429_same_cred {
        return (false, true);
    }
    let rotated = pool.mark_last_issued_rate_limited_and_has_alternate(Duration::from_secs(60));
    (rotated, false)
}
