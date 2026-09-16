use std::time::Duration;

use crate::types::ServerInstanceConfig;

pub fn base_url(config: &ServerInstanceConfig) -> String {
    format!("http://{}:{}", config.host, config.port)
}

/// Agent for fast status/metadata calls: 5 s connect timeout, 10 s read timeout.
pub fn quick_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(10))
        .build()
}

/// Agent for model warm-load: 5 s connect timeout, 5 min read timeout.
pub fn load_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(300))
        .build()
}
