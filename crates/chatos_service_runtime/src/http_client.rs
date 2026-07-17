// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const MINIMUM_TIMEOUT: Duration = Duration::from_millis(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpClientTimeouts {
    connect_timeout: Duration,
    request_timeout: Duration,
    read_timeout: Duration,
}

impl HttpClientTimeouts {
    pub fn new(request_timeout: Duration) -> Self {
        let request_timeout = non_zero_timeout(request_timeout);
        Self {
            connect_timeout: DEFAULT_CONNECT_TIMEOUT.min(request_timeout),
            request_timeout,
            read_timeout: request_timeout,
        }
    }

    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = non_zero_timeout(timeout).min(self.request_timeout);
        self
    }

    pub fn with_read_timeout(mut self, timeout: Duration) -> Self {
        self.read_timeout = non_zero_timeout(timeout).min(self.request_timeout);
        self
    }

    pub fn connect_timeout(self) -> Duration {
        self.connect_timeout
    }

    pub fn request_timeout(self) -> Duration {
        self.request_timeout
    }

    pub fn read_timeout(self) -> Duration {
        self.read_timeout
    }
}

pub fn http_client_builder(timeouts: HttpClientTimeouts) -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .connect_timeout(timeouts.connect_timeout)
        .timeout(timeouts.request_timeout)
        .read_timeout(timeouts.read_timeout)
}

pub fn build_http_client(timeouts: HttpClientTimeouts) -> Result<reqwest::Client, reqwest::Error> {
    http_client_builder(timeouts).build()
}

fn non_zero_timeout(timeout: Duration) -> Duration {
    if timeout.is_zero() {
        MINIMUM_TIMEOUT
    } else {
        timeout
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{build_http_client, HttpClientTimeouts};

    #[test]
    fn derives_bounded_connect_and_read_timeouts() {
        let short = HttpClientTimeouts::new(Duration::from_millis(500));
        assert_eq!(short.connect_timeout(), Duration::from_millis(500));
        assert_eq!(short.read_timeout(), Duration::from_millis(500));

        let normal = HttpClientTimeouts::new(Duration::from_secs(10));
        assert_eq!(normal.connect_timeout(), Duration::from_secs(3));
        assert_eq!(normal.request_timeout(), Duration::from_secs(10));
        assert_eq!(normal.read_timeout(), Duration::from_secs(10));
        build_http_client(normal).expect("standard HTTP client should build");
    }

    #[test]
    fn normalizes_zero_and_caps_overrides_to_total_timeout() {
        let timeouts = HttpClientTimeouts::new(Duration::ZERO)
            .with_connect_timeout(Duration::from_secs(30))
            .with_read_timeout(Duration::from_secs(30));
        assert_eq!(timeouts.request_timeout(), Duration::from_millis(1));
        assert_eq!(timeouts.connect_timeout(), Duration::from_millis(1));
        assert_eq!(timeouts.read_timeout(), Duration::from_millis(1));
    }
}
