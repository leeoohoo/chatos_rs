// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::{
    ChatosProvider, ChatosProviderConfig, LocalConnectorProvider, PluginComponentProvider,
    PluginLocalProvider, PluginRouteDispatcher, ProviderDispatcher, ProviderRuntimeConfig,
};
use std::sync::Arc;

impl ProviderDispatcher {
    pub(crate) fn new(
        chatos: ChatosProviderConfig,
        local_connector_http: reqwest::Client,
        local_connector_service_base_url: impl Into<String>,
        local_connector_internal_secret: Option<String>,
        runtime: ProviderRuntimeConfig,
        skill_attestations: Arc<super::plugin_components::SkillActivationAttestationService>,
    ) -> Result<Self, String> {
        let local_connector_service_base_url = local_connector_service_base_url.into();
        let plugin_local = PluginLocalProvider::new(
            local_connector_http.clone(),
            local_connector_service_base_url.clone(),
            runtime.local_connector_request_timeout,
            local_connector_internal_secret.clone(),
            runtime.response_limit_bytes,
            skill_attestations.clone(),
        )?;
        let plugin_components = PluginComponentProvider::new(
            local_connector_http.clone(),
            local_connector_service_base_url.clone(),
            runtime.local_connector_request_timeout,
            local_connector_internal_secret.clone(),
            runtime.response_limit_bytes,
            skill_attestations,
        )?;
        Ok(Self {
            local_connector: LocalConnectorProvider::new(
                local_connector_http.clone(),
                local_connector_service_base_url.clone(),
                runtime.local_connector_request_timeout,
                local_connector_internal_secret.clone(),
                runtime.response_limit_bytes,
            )?,
            plugins: PluginRouteDispatcher::new(plugin_local, plugin_components),
            chatos: ChatosProvider::new(
                chatos.http,
                chatos.base_url,
                chatos.request_timeout,
                chatos.internal_secret,
                runtime.response_limit_bytes,
            )?,
        })
    }
}
