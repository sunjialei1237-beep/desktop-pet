pub mod download;
pub mod error;
pub mod model;

pub use download::{DownloadProgress, ModelDownloader, ProgressCallback};
pub use error::{EmbeddingError, Result};
pub use model::{cosine_similarity, EmbeddingModel, EMBEDDING_DIM};

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// High-level embedding service that wraps the model with lifecycle management.
/// Uses interior mutability so `load()` can be called on a shared reference
/// (required for Tauri's `State<AppState>` which only provides `&self`).
pub struct EmbeddingService {
    model: Mutex<Option<Arc<Mutex<EmbeddingModel>>>>,
    model_dir: PathBuf,
    /// App-config key of the currently loaded model (fp32/int8). Used at
    /// startup to reconcile stored episode vectors with the active vector
    /// space (P1 memory reduction: switching fp32 -> int8 must re-embed).
    model_key: Mutex<Option<String>>,
}

impl EmbeddingService {
    /// Creates a service with no model loaded yet.
    /// Call load after verifying the model files are present.
    pub fn new(model_dir: &Path) -> Self {
        EmbeddingService {
            model: Mutex::new(None),
            model_dir: model_dir.to_path_buf(),
            model_key: Mutex::new(None),
        }
    }

    /// Attempts to load the model from disk. Returns an error if files are missing.
    /// Safe to call on a shared reference (uses interior mutability).
    pub fn load(&self) -> Result<()> {
        let model = EmbeddingModel::load(&self.model_dir)?;
        let key = model.model_key();
        let mut guard = self.model.lock()
            .map_err(|e| EmbeddingError::Onnx(format!("Service lock: {}", e)))?;
        *guard = Some(Arc::new(Mutex::new(model)));
        if let Ok(mut key_guard) = self.model_key.lock() {
            *key_guard = Some(key);
        }
        log::info!("EmbeddingService model loaded");
        Ok(())
    }

    /// Returns true if the model is loaded and ready for inference.
    pub fn is_ready(&self) -> bool {
        self.model.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    /// Returns the app-config key of the loaded model, when ready.
    pub fn model_key(&self) -> Option<String> {
        self.model_key.lock().ok().and_then(|g| g.clone())
    }

    /// Embeds a single text. Returns an error if the model isn't loaded.
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let model_arc = {
            let guard = self.model.lock()
                .map_err(|e| EmbeddingError::Onnx(format!("Service lock: {}", e)))?;
            guard.as_ref().ok_or(EmbeddingError::ModelNotConfigured)?.clone()
        };
        let mut guard = model_arc
            .lock()
            .map_err(|e| EmbeddingError::Onnx(format!("Model lock: {}", e)))?;
        guard.embed(text)
    }

    /// Embeds multiple texts.
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let model_arc = {
            let guard = self.model.lock()
                .map_err(|e| EmbeddingError::Onnx(format!("Service lock: {}", e)))?;
            guard.as_ref().ok_or(EmbeddingError::ModelNotConfigured)?.clone()
        };
        let mut guard = model_arc
            .lock()
            .map_err(|e| EmbeddingError::Onnx(format!("Model lock: {}", e)))?;
        guard.embed_batch(texts)
    }

    /// Returns the model directory path.
    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }

    /// Returns a clone of the Arc to the loaded model, if available.
    pub fn model_handle(&self) -> Option<Arc<Mutex<EmbeddingModel>>> {
        self.model.lock().ok().and_then(|g| g.clone())
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
        let svc = EmbeddingService::new(&dir);
        let result = svc.load();
        assert!(result.is_err());
        assert!(!svc.is_ready());
    }
}
