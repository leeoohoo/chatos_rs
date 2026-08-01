// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn validate_loopback_api_base(value: &str) -> Result<()> {
    let url = Url::parse(value).context("parse Chrome rendezvous API URL")?;
    if url.scheme() != "http"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || !url
            .host_str()
            .is_some_and(|host| host == "localhost" || host == "::1" || host.starts_with("127."))
        || url.port().is_none()
    {
        bail!("Chrome rendezvous API URL must be an explicit loopback HTTP origin");
    }
    Ok(())
}
