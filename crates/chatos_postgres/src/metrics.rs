// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fmt::Write;

use crate::PgPool;

pub fn render_pool_metrics(pool: &PgPool, service: &str) -> String {
    let size = pool.size();
    let idle = u32::try_from(pool.num_idle()).unwrap_or(u32::MAX).min(size);
    let used = size.saturating_sub(idle);
    let max = pool.options().get_max_connections();
    let wait_pressure = u8::from(size >= max && idle == 0);
    let service = escape_label(service);
    let mut body = String::new();
    let _ = writeln!(
        body,
        "# HELP chatos_postgres_pool_connections PostgreSQL pool connections by state."
    );
    let _ = writeln!(body, "# TYPE chatos_postgres_pool_connections gauge");
    let _ = writeln!(
        body,
        "chatos_postgres_pool_connections{{service=\"{service}\",state=\"used\"}} {used}"
    );
    let _ = writeln!(
        body,
        "chatos_postgres_pool_connections{{service=\"{service}\",state=\"idle\"}} {idle}"
    );
    let _ = writeln!(
        body,
        "# HELP chatos_postgres_pool_max_connections Configured PostgreSQL pool connection limit."
    );
    let _ = writeln!(body, "# TYPE chatos_postgres_pool_max_connections gauge");
    let _ = writeln!(
        body,
        "chatos_postgres_pool_max_connections{{service=\"{service}\"}} {max}"
    );
    let _ = writeln!(
        body,
        "# HELP chatos_postgres_pool_wait_pressure Whether the pool is saturated and new acquisitions may be waiting."
    );
    let _ = writeln!(body, "# TYPE chatos_postgres_pool_wait_pressure gauge");
    let _ = writeln!(
        body,
        "chatos_postgres_pool_wait_pressure{{service=\"{service}\"}} {wait_pressure}"
    );
    body
}

fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn renders_empty_pool_metrics() {
        let pool =
            PgPool::connect_lazy("postgresql://unused:unused@localhost/unused").expect("lazy pool");
        let metrics = render_pool_metrics(&pool, "test-service");
        assert!(metrics.contains("state=\"used\"} 0"));
        assert!(metrics.contains("state=\"idle\"} 0"));
        assert!(metrics.contains("chatos_postgres_pool_max_connections"));
        assert!(metrics.contains("chatos_postgres_pool_wait_pressure"));
    }
}
