// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct ServiceRegistration {
    pub name: String,
    pub id: String,
    pub address: String,
    pub port: u16,
    pub health_path: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceEndpoint {
    pub service_name: String,
    pub address: String,
    pub port: u16,
    pub scheme: String,
}

impl ServiceEndpoint {
    pub fn base_url(&self) -> String {
        format!("{}://{}:{}", self.scheme, self.address, self.port)
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct ConsulRegisterRequest {
    #[serde(rename = "ID")]
    pub(crate) id: String,
    #[serde(rename = "Name")]
    pub(crate) name: String,
    #[serde(rename = "Address")]
    pub(crate) address: String,
    #[serde(rename = "Port")]
    pub(crate) port: u16,
    #[serde(rename = "Tags")]
    pub(crate) tags: Vec<String>,
    #[serde(rename = "Check")]
    pub(crate) check: ConsulRegisterCheck,
}

#[derive(Debug, Serialize)]
pub(crate) struct ConsulRegisterCheck {
    #[serde(rename = "HTTP")]
    pub(crate) http: String,
    #[serde(rename = "Interval")]
    pub(crate) interval: String,
    #[serde(rename = "Timeout")]
    pub(crate) timeout: String,
    #[serde(rename = "DeregisterCriticalServiceAfter")]
    pub(crate) deregister_critical_service_after: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ConsulHealthEntry {
    #[serde(rename = "Node")]
    pub(crate) node: ConsulNode,
    #[serde(rename = "Service")]
    pub(crate) service: ConsulService,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ConsulNode {
    #[serde(rename = "Node", default)]
    pub(crate) name: String,
    #[serde(rename = "Address", default)]
    pub(crate) address: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ConsulService {
    #[serde(rename = "Address", default)]
    pub(crate) address: String,
    #[serde(rename = "Port", default)]
    pub(crate) port: u16,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ConsulKvEntry {
    #[serde(rename = "Value", default)]
    pub(crate) value: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{ConsulRegisterCheck, ConsulRegisterRequest, ServiceEndpoint};

    #[test]
    fn endpoint_formats_base_url() {
        let endpoint = ServiceEndpoint {
            service_name: "user-service".to_string(),
            address: "user-service-backend".to_string(),
            port: 39190,
            scheme: "http".to_string(),
        };
        assert_eq!(
            endpoint.base_url(),
            "http://user-service-backend:39190".to_string()
        );
    }

    #[test]
    fn serializes_consul_registration_with_expected_field_names() {
        let request = ConsulRegisterRequest {
            id: "user-service-local-1".to_string(),
            name: "user-service".to_string(),
            address: "user-service-backend".to_string(),
            port: 39190,
            tags: vec!["local".to_string()],
            check: ConsulRegisterCheck {
                http: "http://user-service-backend:39190/api/health".to_string(),
                interval: "10s".to_string(),
                timeout: "3s".to_string(),
                deregister_critical_service_after: "1m".to_string(),
            },
        };
        let value = serde_json::to_value(request).expect("serialize request");
        assert!(value.get("ID").is_some());
        assert!(value
            .get("Check")
            .and_then(|check| check.get("HTTP"))
            .is_some());
        assert!(value
            .get("Check")
            .and_then(|check| check.get("DeregisterCriticalServiceAfter"))
            .is_some());
    }
}
