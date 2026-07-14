use std::fmt;

/// Errors that can occur in the embedding subsystem.
#[derive(Debug)]
pub enum EmbeddingError {
    /// Model directory not configured or not found.
    ModelNotConfigured,
    /// Model files missing on disk.
    ModelFilesMissing(String),
    /// ONNX Runtime error during session creation or inference.
    Onnx(String),
    /// Tokenizer error.
    Tokenizer(String),
    /// Network error during model download.
    Download(String),
    /// I/O error.
    Io(String),
}

impl fmt::Display for EmbeddingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EmbeddingError::ModelNotConfigured => {
                write!(f, "Embedding model directory not configured")
            }
            EmbeddingError::ModelFilesMissing(detail) => {
                write!(f, "Model files missing: {}", detail)
            }
            EmbeddingError::Onnx(detail) => {
                write!(f, "ONNX Runtime error: {}", detail)
            }
            EmbeddingError::Tokenizer(detail) => {
                write!(f, "Tokenizer error: {}", detail)
            }
            EmbeddingError::Download(detail) => {
                write!(f, "Model download error: {}", detail)
            }
            EmbeddingError::Io(detail) => {
                write!(f, "I/O error: {}", detail)
            }
        }
    }
}

impl std::error::Error for EmbeddingError {}

impl From<std::io::Error> for EmbeddingError {
    fn from(e: std::io::Error) -> Self {
        EmbeddingError::Io(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, EmbeddingError>;
