// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::compose::*;
use super::super::*;

pub(in crate::services::environment_agent::tool_provider) fn generated_environment_variables(
    required_services: &Value,
    agent_env_vars: Option<&Value>,
) -> Value {
    let mut env_vars = agent_env_vars
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for service in required_services.as_array().into_iter().flatten() {
        let service_type = service
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        match service_type.as_str() {
            "redis" => {
                insert_text_default(&mut env_vars, "REDIS_HOST", "redis");
                insert_text_default(&mut env_vars, "REDIS_PORT", "6379");
                insert_secret_default(&mut env_vars, "REDIS_PASSWORD");
                insert_text_default(&mut env_vars, "SPRING_DATA_REDIS_HOST", "redis");
                insert_text_default(&mut env_vars, "SPRING_DATA_REDIS_PORT", "6379");
                copy_text_default(
                    &mut env_vars,
                    "REDIS_PASSWORD",
                    "SPRING_DATA_REDIS_PASSWORD",
                );
            }
            "postgres" | "postgresql" => {
                insert_text_default(&mut env_vars, "POSTGRES_HOST", "postgres");
                insert_text_default(&mut env_vars, "POSTGRES_PORT", "5432");
                insert_text_default(&mut env_vars, "POSTGRES_USER", "app");
                insert_secret_default(&mut env_vars, "POSTGRES_PASSWORD");
                insert_text_default(&mut env_vars, "POSTGRES_DB", "app");
                insert_text_default(
                    &mut env_vars,
                    "SPRING_DATASOURCE_URL",
                    "jdbc:postgresql://postgres:5432/app",
                );
                copy_text_default(&mut env_vars, "POSTGRES_USER", "SPRING_DATASOURCE_USERNAME");
                copy_text_default(
                    &mut env_vars,
                    "POSTGRES_PASSWORD",
                    "SPRING_DATASOURCE_PASSWORD",
                );
            }
            "mysql" | "mariadb" => {
                insert_text_default(&mut env_vars, "MYSQL_HOST", "mysql");
                insert_text_default(&mut env_vars, "MYSQL_PORT", "3306");
                insert_secret_default(&mut env_vars, "MYSQL_ROOT_PASSWORD");
                insert_text_default(&mut env_vars, "MYSQL_DATABASE", "app");
                insert_text_default(&mut env_vars, "MYSQL_USER", "app");
                insert_secret_default(&mut env_vars, "MYSQL_PASSWORD");
                insert_text_default(
                    &mut env_vars,
                    "SPRING_DATASOURCE_URL",
                    "jdbc:mysql://mysql:3306/app?useSSL=false&allowPublicKeyRetrieval=true",
                );
                copy_text_default(&mut env_vars, "MYSQL_USER", "SPRING_DATASOURCE_USERNAME");
                copy_text_default(
                    &mut env_vars,
                    "MYSQL_PASSWORD",
                    "SPRING_DATASOURCE_PASSWORD",
                );
            }
            "nacos" => {
                insert_text_default(&mut env_vars, "NACOS_SERVER_ADDR", "nacos:8848");
                insert_text_default(&mut env_vars, "NACOS_NAMESPACE", "public");
                insert_text_default(&mut env_vars, "NACOS_USERNAME", "nacos");
                insert_secret_default(&mut env_vars, "NACOS_PASSWORD");
                insert_secret_default(&mut env_vars, "NACOS_AUTH_TOKEN");
                insert_text_default(
                    &mut env_vars,
                    "SPRING_CLOUD_NACOS_SERVER_ADDR",
                    "nacos:8848",
                );
                copy_text_default(
                    &mut env_vars,
                    "NACOS_USERNAME",
                    "SPRING_CLOUD_NACOS_USERNAME",
                );
                copy_text_default(
                    &mut env_vars,
                    "NACOS_PASSWORD",
                    "SPRING_CLOUD_NACOS_PASSWORD",
                );
            }
            "mongodb" | "mongo" => {
                insert_text_default(&mut env_vars, "MONGODB_HOST", "mongodb");
                insert_text_default(&mut env_vars, "MONGODB_PORT", "27017");
                insert_text_default(&mut env_vars, "MONGO_INITDB_ROOT_USERNAME", "app");
                insert_secret_default(&mut env_vars, "MONGO_INITDB_ROOT_PASSWORD");
                insert_text_default(&mut env_vars, "SPRING_DATA_MONGODB_HOST", "mongodb");
                insert_text_default(&mut env_vars, "SPRING_DATA_MONGODB_PORT", "27017");
                copy_text_default(
                    &mut env_vars,
                    "MONGO_INITDB_ROOT_USERNAME",
                    "SPRING_DATA_MONGODB_USERNAME",
                );
                copy_text_default(
                    &mut env_vars,
                    "MONGO_INITDB_ROOT_PASSWORD",
                    "SPRING_DATA_MONGODB_PASSWORD",
                );
            }
            "rabbitmq" => {
                insert_text_default(&mut env_vars, "RABBITMQ_HOST", "rabbitmq");
                insert_text_default(&mut env_vars, "RABBITMQ_PORT", "5672");
                insert_text_default(&mut env_vars, "RABBITMQ_DEFAULT_USER", "app");
                insert_secret_default(&mut env_vars, "RABBITMQ_DEFAULT_PASS");
            }
            _ => {}
        }
    }
    Value::Object(env_vars)
}
