use crate::types::ServerInstanceConfig;

pub fn base_url(config: &ServerInstanceConfig) -> String {
    format!("http://{}:{}", config.host, config.port)
}
