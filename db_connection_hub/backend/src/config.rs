pub struct Config {
    pub host: String,
    pub port: u16,
}

impl Config {
    pub fn from_env() -> Self {
        let host = std::env::var("DB_HUB_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = std::env::var("DB_HUB_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(8099);

        Self { host, port }
    }
}
