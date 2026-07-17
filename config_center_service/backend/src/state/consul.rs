// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl AppState {
    pub(super) async fn publish_consul(
        &self,
        environment: &str,
        revision: i64,
        definitions: &[ConfigDefinitionRecord],
        values: &BTreeMap<String, Value>,
    ) -> Result<(), String> {
        let Some(consul) = self.config.consul_http_addr.as_deref() else {
            return Ok(());
        };
        let shared = compatibility_env(definitions, values, |definition| {
            definition.scope == "shared"
        });
        self.put_consul(
            consul,
            format!("chatos/{environment}/shared/config").as_str(),
            &json!({ "revision": revision, "env": shared }),
        )
        .await?;
        let services = known_services(definitions);
        for service_name in services {
            let env = compatibility_env(definitions, values, |definition| {
                definition.service_name.as_deref() == Some(service_name.as_str())
            });
            self.put_consul(
                consul,
                format!("chatos/{environment}/services/{service_name}/config").as_str(),
                &json!({ "revision": revision, "env": env }),
            )
            .await?;
        }
        Ok(())
    }

    async fn put_consul(&self, base_url: &str, key: &str, value: &Value) -> Result<(), String> {
        let response = self
            .http
            .put(format!("{}/v1/kv/{key}", base_url.trim_end_matches('/')))
            .body(serde_json::to_vec(value).map_err(|err| err.to_string())?)
            .send()
            .await
            .map_err(|err| format!("Consul write {key} failed: {err}"))?;
        if !response.status().is_success() {
            return Err(format!("Consul write {key} returned {}", response.status()));
        }
        Ok(())
    }
}
