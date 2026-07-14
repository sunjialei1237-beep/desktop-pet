pub mod download;
pub mod error;
pub mod model;

pub use download::{DownloadProgress, ModelDownloader, ProgressCallback};
pub use error::{EmbeddingError, Result};
pub use model::{cosine_similarity, EmbeddingModel, EMBEDDING_DIM};

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// High-level embedding service that wraps the model with lifecycle management.
/// Owns an Option<Arc<Mutex<EmbeddingModel>>> - None means model not loaded yet.
/// Mutex provides interior mutability (ort session.run() needs &mut self).
pub struct EmbeddingService {
    model: Option<Arc<Mutex<EmbeddingModel>>>,
    model_dir: PathBuf,
}

impl EmbeddingService {
    /// Creates a service with no model loaded yet.
    /// Call load after verifying the model files are present.
    pub fn new(model_dir: &Path) -> Self {
        EmbeddingService {
            model: None,
            model_dir: model_dir.to_path_buf(),
        }
    }

    /// Attempts to load the model from disk. Returns an error if files are missing.
    pub fn load(&mut self) -> Result<()> {
        let model = EmbeddingModel::load(&self.model_dir)?;
        self.model = Some(Arc::new(Mutex::new(model)));
        log::info!("EmbeddingService model loaded");
        Ok(())
    }

    /// Returns true if the model is loaded and ready for inference.
    pub fn is_ready(&self) -> bool {
        self.model.is_some()
    }

    /// Embeds a single text. Returns an error if the model isn't loaded.
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let model = self
            .model
            .as_ref()
            .ok_or(EmbeddingError::ModelNotConfigured)?;
        let mut guard = model
            .lock()
            .map_err(|e| EmbeddingError::Onnx(format!("Model lock: {}", e)))?;
        guard.embed(text)
    }

    /// Embeds multiple texts.
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let model = self
            .model
            .as_ref()
            .ok_or(EmbeddingError::ModelNotConfigured)?;
        let mut guard = model
            .lock()
            .map_err(|e| EmbeddingError::Onnx(format!("Model lock: {}", e)))?;
        guard.embed_batch(texts)
    }

    /// Returns a clone of the Arc to the loaded model, if available.
    pub fn model_handle(&self) -> Option<Arc<Mutex<EmbeddingModel>>> {
        self.model.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_not_ready() {
        let svc = EmbeddingService::new(std::path::Path::new("/nonexistent"));
        assert!(!svc.is_ready());
        assert!(svc.embed("test").is_err());
    }

    #[test]
    fn test_service_load_missing_files() {
        let dir = std::env::temp_dir().join("dpet_test_svc_missing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut svc = EmbeddingService::new(&dir);
        let result = svc.load();
        assert!(result.is_err());
        assert!(!svc.is_ready());
    }
}
