/// LLM error types for Recovery system (design doc 7.11).
/// Each variant maps to a character-appropriate reaction, not a system error.
#[derive(Debug)]
pub enum LlmError {
    Timeout,
    Network,
    Auth,
    RateLimit,
    Server(String),
    Parse(String),
    NotConfigured,
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmError::Timeout => write!(f, "Request timed out"),
            LlmError::Network => write!(f, "Network error"),
            LlmError::Auth => write!(f, "Authentication failed (check API key)"),
            LlmError::RateLimit => write!(f, "Rate limited"),
            LlmError::Server(msg) => write!(f, "Server error: {}", msg),
            LlmError::Parse(msg) => write!(f, "Parse error: {}", msg),
            LlmError::NotConfigured => write!(f, "LLM not configured (API key empty)"),
        }
    }
}

impl std::error::Error for LlmError {}
