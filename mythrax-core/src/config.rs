use std::env;

pub const DEFAULT_DAEMON_PORT: u16 = 8090;
pub const DEFAULT_LLM_PROXY_PORT: u16 = 8080;
pub const MAX_HYDRATION_CHARS: usize = 10_000;
pub const SECONDS_PER_DAY: f64 = 86_400.0;

pub fn daemon_port() -> u16 {
    env::var("MYTHRAX_DAEMON_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(DEFAULT_DAEMON_PORT)
}

pub fn daemon_url() -> String {
    format!("http://127.0.0.1:{}", daemon_port())
}

pub fn llm_proxy_url() -> String {
    env::var("MYTHRAX_PROXY_URL")
        .unwrap_or_else(|_| format!("http://127.0.0.1:{}/v1/chat/completions", DEFAULT_LLM_PROXY_PORT))
}
