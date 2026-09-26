use std::time::Duration;

use crate::types::ServerInstanceConfig;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const QUICK_READ_TIMEOUT: Duration = Duration::from_secs(10);
const WARM_LOAD_READ_TIMEOUT: Duration = Duration::from_secs(300);

pub fn base_url(config: &ServerInstanceConfig) -> String {
    format!("http://{}:{}", config.host, config.port)
}

pub fn quick_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(QUICK_READ_TIMEOUT)
        .build()
}

pub fn warm_load_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(WARM_LOAD_READ_TIMEOUT)
        .build()
}
