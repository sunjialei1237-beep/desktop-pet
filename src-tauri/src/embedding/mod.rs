pub mod download;
pub mod error;
pub mod model;

pub use download::{DownloadProgress, ModelDownloader, ProgressCallback};
pub use error::{EmbeddingError, Result};
pub use model::{cosine_similarity, current_model_key, EmbeddingModel, EMBEDDING_DIM};

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// High-level embedding service that wraps the model with lifecycle management.
/// Uses interior mutability so `load()` can be called on a shared reference
/// (required for Tauri's `State<AppState>` which only provides `&self`).
///
/// P2 memory reduction: the service supports lazy load + idle unload so an
/// all-day-running pet doesn't hold the ~570 MB int8 model while the user is
/// away. `lazy_load` makes `embed()` load the model on demand (the DeepSeek
/// reply already takes seconds, so a ~1s load is swallowed); the idle watcher
/// (lib.rs) drops it after `idle_unload` of inactivity. Scheduler paths
/// (60min proactive window) reload it — the sawtooth is by design.
pub struct EmbeddingService {
    model: Mutex<Option<Arc<Mutex<EmbeddingModel>>>>,
    model_dir: PathBuf,
    /// App-config key of the currently loaded model (fp32/int8). Used at
    /// startup to reconcile stored episode vectors with the active vector
    /// space (P1 memory reduction: switching fp32 -> int8 must re-embed).
    model_key: Mutex<Option<String>>,
    lazy_load: bool,
    idle_unload: Duration,
    /// Last time the model produced (or started producing) an embedding, or
    /// was (lazy-)loaded. Idle-unload decisions read this.
    last_used: Mutex<Option<Instant>>,
    loads: AtomicU32,
    unloads: AtomicU32,
}

impl EmbeddingService {
    /// Creates a service with no model loaded yet, in the legacy eager mode
    /// (embed errors until `load()` succeeds; never unloads). Production code
    /// should chain `with_lazy` from the app config; tests keep this default.
    pub fn new(model_dir: &Path) -> Self {
        EmbeddingService {
            model: Mutex::new(None),
            model_dir: model_dir.to_path_buf(),
            model_key: Mutex::new(None),
            lazy_load: false,
            idle_unload: Duration::ZERO,
            last_used: Mutex::new(None),
            loads: AtomicU32::new(0),
            unloads: AtomicU32::new(0),
        }
    }

    /// Switches the service to lazy-load lifecycle (P2). `idle_unload_minutes`
    /// <= 0 keeps the model resident once loaded.
    pub fn with_lazy(mut self, lazy_load: bool, idle_unload_minutes: i64) -> Self {
        self.lazy_load = lazy_load;
        self.idle_unload = if idle_unload_minutes > 0 {
            Duration::from_secs(idle_unload_minutes as u64 * 60)
        } else {
            Duration::ZERO
        };
        self
    }

    /// True when a usable model file set exists on disk (either quantized or
    /// legacy fp32) — i.e. embedding CAN run, regardless of whether the model
    /// is currently resident.
    pub fn files_present(&self) -> bool {
        ModelDownloader::new(&self.model_dir).check_complete()
    }

    /// Attempts to load the model from disk. Returns an error if files are missing.
    /// Safe to call on a shared reference (uses interior mutability).
    pub fn load(&self) -> Result<()> {
        let t0 = Instant::now();
        let model = EmbeddingModel::load(&self.model_dir)?;
        let key = model.model_key();
        let mut guard = self.model.lock()
            .map_err(|e| EmbeddingError::Onnx(format!("Service lock: {}", e)))?;
        *guard = Some(Arc::new(Mutex::new(model)));
        drop(guard);
        if let Ok(mut key_guard) = self.model_key.lock() {
            *key_guard = Some(key);
        }
        self.loads.fetch_add(1, Ordering::Relaxed);
        self.mark_used();
        log::info!(
            "EmbeddingService model loaded ({} ms)",
            t0.elapsed().as_millis()
        );
        Ok(())
    }

    /// Loads the model only if not already resident (P2 lazy load). Concurrent
    /// callers serialize on the service lock; the loser's duplicate model is
    /// dropped without installing.
    pub fn ensure_loaded(&self) -> Result<()> {
        {
            let guard = self.model.lock()
                .map_err(|e| EmbeddingError::Onnx(format!("Service lock: {}", e)))?;
            if guard.is_some() {
                return Ok(());
            }
        }
        let t0 = Instant::now();
        let model = EmbeddingModel::load(&self.model_dir)?;
        let key = model.model_key();
        let mut guard = self.model.lock()
            .map_err(|e| EmbeddingError::Onnx(format!("Service lock: {}", e)))?;
        if guard.is_some() {
            log::debug!("[embedding] another thread finished loading first; dropping duplicate");
            return Ok(());
        }
        *guard = Some(Arc::new(Mutex::new(model)));
        drop(guard);
        if let Ok(mut key_guard) = self.model_key.lock() {
            *key_guard = Some(key);
        }
        self.loads.fetch_add(1, Ordering::Relaxed);
        self.mark_used();
        log::info!(
            "[embedding] model lazy-loaded in {} ms (load #{})",
            t0.elapsed().as_millis(),
            self.loads.load(Ordering::Relaxed)
        );
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

    /// Drops the resident model if it has been idle longer than the configured
    /// unload window. Returns true when an unload actually happened. An
    /// in-flight embed() keeps its model alive via the shared Arc; the next
    /// embed() lazy-loads again (~1s, absorbed by the LLM reply latency).
    pub fn unload_if_idle(&self) -> bool {
        if !self.lazy_load || self.idle_unload.is_zero() {
            return false;
        }
        let idle = match self.last_used.lock() {
            Ok(g) => match *g {
                Some(t) => t.elapsed(),
                None => return false,
            },
            Err(_) => return false,
        };
        if idle < self.idle_unload {
            return false;
        }
        let mut guard = match self.model.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        if guard.is_none() {
            return false;
        }
        // Re-check under the model lock: embed() marks used BEFORE cloning the
        // Arc, so a fresh timestamp here means someone is mid-request and we
        // must yield (closing the mark-then-clone race window).
        let busy = match self.last_used.lock() {
            Ok(g) => match *g {
                Some(t) => t.elapsed() < self.idle_unload,
                None => true,
            },
            Err(_) => true,
        };
        if busy {
            return false;
        }
        *guard = None;
        drop(guard);
        if let Ok(mut key_guard) = self.model_key.lock() {
            *key_guard = None;
        }
        let n = self.unloads.fetch_add(1, Ordering::Relaxed) + 1;
        log::info!(
            "[embedding] model idle-unloaded after {:?} (unload #{}; next embed re-loads in ~1s)",
            idle,
            n
        );
        true
    }

    /// Load/unload counters for status display (Architecture #11).
    pub fn lifecycle_stats(&self) -> (u32, u32) {
        (
            self.loads.load(Ordering::Relaxed),
            self.unloads.load(Ordering::Relaxed),
        )
    }

    /// Whether the service lazy-loads on demand (status display).
    pub fn is_lazy(&self) -> bool {
        self.lazy_load
    }

    /// Configured idle-unload window in minutes; 0 = resident (status display).
    pub fn idle_unload_minutes(&self) -> i64 {
        if self.idle_unload.is_zero() {
            0
        } else {
            (self.idle_unload.as_secs() / 60) as i64
        }
    }

    /// Test hook: pretends the last use happened long enough ago to satisfy
    /// any idle threshold, so unload timing is testable without sleeping.
    pub fn force_idle_for_test(&self) {
        if let Ok(mut g) = self.last_used.lock() {
            *g = Some(Instant::now() - Duration::from_secs(24 * 3600));
        }
    }

    fn mark_used(&self) {
        if let Ok(mut g) = self.last_used.lock() {
            *g = Some(Instant::now());
        }
    }

    /// Embeds a single text. Errors if the model isn't loaded (eager mode) or
    /// can't be loaded (lazy mode).
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        if self.lazy_load {
            self.ensure_loaded()?;
        }
        self.mark_used();
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
        if self.lazy_load {
            self.ensure_loaded()?;
        }
        self.mark_used();
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

    #[test]
    fn test_lazy_embed_without_files_errors_not_silently() {
        // Lazy mode with missing files must surface an error (callers degrade
        // to keyword fallback), never pretend to succeed.
        let dir = std::env::temp_dir().join("dpet_test_svc_lazy_missing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let svc = EmbeddingService::new(&dir).with_lazy(true, 30);
        assert!(svc.is_lazy());
        assert_eq!(svc.idle_unload_minutes(), 30);
        assert!(svc.embed("test").is_err());
        assert!(!svc.is_ready());
    }

    #[test]
    fn test_unload_if_idle_noop_when_not_loaded_or_eager() {
        let dir = std::env::temp_dir().join("dpet_test_svc_unload_noop");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Not loaded -> nothing to unload, no panic.
        let lazy = EmbeddingService::new(&dir).with_lazy(true, 30);
        lazy.force_idle_for_test();
        assert!(!lazy.unload_if_idle());

        // Eager mode (default) never unloads even when idle.
        let eager = EmbeddingService::new(&dir);
        assert!(!eager.is_lazy());
        assert_eq!(eager.idle_unload_minutes(), 0);
        assert!(!eager.unload_if_idle());

        // Lazy but unload disabled (0 minutes) never unloads.
        let resident = EmbeddingService::new(&dir).with_lazy(true, 0);
        assert_eq!(resident.idle_unload_minutes(), 0);
        assert!(!resident.unload_if_idle());
    }

    #[test]
    fn test_lazy_accessors_and_defaults() {
        let svc = EmbeddingService::new(std::path::Path::new("/nonexistent"));
        assert!(!svc.is_lazy());
        assert_eq!(svc.idle_unload_minutes(), 0);
        assert_eq!(svc.lifecycle_stats(), (0, 0));
        assert!(!svc.files_present());
        let lazy = svc.with_lazy(true, 45);
        assert!(lazy.is_lazy());
        assert_eq!(lazy.idle_unload_minutes(), 45);
    }
}
