use crate::embedding::error::{EmbeddingError, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Progress payload emitted via Tauri event "download-progress".
#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub file_name: String,
    pub downloaded: u64,
    pub total: u64,
    /// 0.0-1.0
    pub fraction: f64,
}

impl DownloadProgress {
    fn new(file_name: &str, downloaded: u64, total: u64) -> Self {
        let fraction = if total > 0 {
            downloaded as f64 / total as f64
        } else {
            0.0
        };
        DownloadProgress {
            file_name: file_name.to_string(),
            downloaded,
            total,
            fraction,
        }
    }
}

/// Quantized int8 model files (P1 memory reduction, 2026-08-16).
/// `model_quantized.onnx` is Xenova's single-file uint8 export (~570 MB,
/// weights embedded, no external data). Same tokenizer, same 1024-dim output,
/// so no DB schema change is needed. The DLL is needed because we use ort's
/// load-dynamic feature to avoid MSVC version conflicts with statically linked
/// binaries.
pub const QUANTIZED_MODEL_FILE: &str = "model_quantized.onnx";
pub const REQUIRED_FILES: &[&str] = &[
    QUANTIZED_MODEL_FILE,
    "tokenizer.json",
    "config.json",
    "onnxruntime.dll",
];

/// Legacy fp32 files. Kept as a fallback: existing installs already have
/// `model.onnx` (graph) + `model.onnx_data` (external fp32 weight blob,
/// ~2.1 GB). The app still loads them if the quantized model is absent, so
/// upgrading never bricks an install that only has the fp32 files.
pub const LEGACY_MODEL_FILE: &str = "model.onnx";
pub const LEGACY_WEIGHTS_FILE: &str = "model.onnx_data";
pub const LEGACY_FP32_FILES: &[&str] = &[
    LEGACY_MODEL_FILE,
    LEGACY_WEIGHTS_FILE,
    "tokenizer.json",
    "config.json",
    "onnxruntime.dll",
];

/// Base URL for the BGE-M3 ONNX export. We use the `Xenova/bge-m3` repo
/// (external-data format: graph + weight split) served via hf-mirror.com —
/// the original `Qdrant/bge-m3-onnx` now 401s, and huggingface.co itself is
/// slow/unreliable from China. Files live under `onnx/` except
/// tokenizer.json + config.json which are at the repo root.
const HF_BASE_URL: &str = "https://hf-mirror.com/Xenova/bge-m3/resolve/main";

/// ONNX Runtime GitHub release download URL (CPU-only, Windows x64).
/// Version must match what ort 2.0.0-rc.12 expects.
const ORT_RUNTIME_URL: &str =
    "https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-win-x64-1.20.1.zip";
/// The DLL file name inside the ORT zip (extracted from lib/ subfolder).
const ORT_DLL_NAME: &str = "onnxruntime.dll";

/// Callback type for download progress updates.
/// In production this pushes Tauri events; in tests it can be a no-op.
pub type ProgressCallback = Box<dyn Fn(DownloadProgress) + Send + Sync>;

/// Manages downloading and verifying the BGE-M3 ONNX model files.
pub struct ModelDownloader {
    model_dir: PathBuf,
    base_url: String,
}

impl ModelDownloader {
    pub fn new(model_dir: &Path) -> Self {
        ModelDownloader {
            model_dir: model_dir.to_path_buf(),
            base_url: HF_BASE_URL.to_string(),
        }
    }

    /// Returns true if all files in `files` exist and are non-empty.
    fn files_complete(&self, files: &[&str]) -> bool {
        files.iter().all(|f| {
            let p = self.model_dir.join(f);
            p.exists() && p.metadata().map(|m| m.len() > 0).unwrap_or(false)
        })
    }

    /// Returns true if the model can be loaded: either the quantized int8 set
    /// or the legacy fp32 set is complete.
    pub fn check_complete(&self) -> bool {
        self.files_complete(REQUIRED_FILES) || self.files_complete(LEGACY_FP32_FILES)
    }

    /// Returns true when the quantized model files are complete. The download
    /// command uses this to force-upgrade legacy fp32 installs to int8.
    pub fn quantized_complete(&self) -> bool {
        self.files_complete(REQUIRED_FILES)
    }

    /// Returns a list of missing files for the PREFERRED quantized set, unless
    /// the app can already load from the legacy fp32 set — then this returns
    /// an empty list so `files_present` semantics in the status UI stay true.
    pub fn missing_files(&self) -> Vec<String> {
        if self.check_complete() {
            return Vec::new();
        }
        REQUIRED_FILES
            .iter()
            .filter(|f| {
                let p = self.model_dir.join(f);
                !p.exists() || p.metadata().map(|m| m.len() == 0).unwrap_or(true)
            })
            .map(|s| s.to_string())
            .collect()
    }

    /// Downloads all missing files. Already-present files are skipped.
    /// The ONNX Runtime DLL comes from a GitHub zip release, not HuggingFace.
    pub async fn download_all(
        &self,
        http: &reqwest::Client,
        progress: Option<&ProgressCallback>,
    ) -> Result<()> {
        std::fs::create_dir_all(&self.model_dir)
            .map_err(|e| EmbeddingError::Download(format!("Failed to create model dir: {}", e)))?;

        // Model files from HuggingFace. Xenova/bge-m3 publishes the int8
        // quantized graph as a single file under onnx/ (weights embedded),
        // tokenizer.json + config.json at the repo root. We fetch each from
        // its remote path but save flat into model_dir. No model.onnx_data
        // needed for the quantized export.
        let hf_files: &[(&str, &str)] = &[
            ("onnx/model_quantized.onnx", QUANTIZED_MODEL_FILE),
            ("tokenizer.json", "tokenizer.json"),
            ("config.json", "config.json"),
        ];
        for (remote_path, local_name) in hf_files {
            let dest = self.model_dir.join(local_name);
            if dest.exists() && dest.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                log::info!("Model file already present: {}", local_name);
                continue;
            }
            self.download_file(http, remote_path, &dest, progress).await?;
        }

        // ONNX Runtime DLL from GitHub (packaged in a zip).
        let dll_dest = self.model_dir.join(ORT_DLL_NAME);
        if !(dll_dest.exists() && dll_dest.metadata().map(|m| m.len() > 0).unwrap_or(false)) {
            self.download_ort_dll(http, progress).await?;
        }

        Ok(())
    }

    /// Downloads and extracts the ONNX Runtime DLL from the GitHub zip release.
    async fn download_ort_dll(
        &self,
        http: &reqwest::Client,
        progress: Option<&ProgressCallback>,
    ) -> Result<()> {
        log::info!("Downloading ONNX Runtime from GitHub");

        let resp = http
            .get(ORT_RUNTIME_URL)
            .send()
            .await
            .map_err(|e| EmbeddingError::Download(format!("ORT HTTP: {}", e)))?;

        if !resp.status().is_success() {
            return Err(EmbeddingError::Download(format!(
                "HTTP {} for ORT",
                resp.status()
            )));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| EmbeddingError::Download(format!("ORT body: {}", e)))?;

        if let Some(cb) = progress {
            cb(DownloadProgress::new(ORT_DLL_NAME, bytes.len() as u64, bytes.len() as u64));
        }

        // Extract onnxruntime.dll from zip. The DLL is at lib/onnxruntime.dll.
        let reader = std::io::Cursor::new(&bytes);
        let mut archive = zip::ZipArchive::new(reader)
            .map_err(|e| EmbeddingError::Download(format!("Zip parse: {}", e)))?;

        let dll_path = self.model_dir.join(ORT_DLL_NAME);
        let mut found = false;

        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| EmbeddingError::Download(format!("Zip entry: {}", e)))?;
            let name = entry.name().to_string();

            if name.ends_with("lib/onnxruntime.dll") || name.ends_with(ORT_DLL_NAME) {
                log::info!("Extracting {} from zip", name);
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut entry, &mut buf)
                    .map_err(|e| EmbeddingError::Download(format!("Read DLL: {}", e)))?;
                std::fs::write(&dll_path, &buf)
                    .map_err(|e| EmbeddingError::Download(format!("Write DLL: {}", e)))?;
                found = true;
                break;
            }
        }

        if !found {
            return Err(EmbeddingError::Download(
                "onnxruntime.dll not found in zip".to_string(),
            ));
        }

        log::info!("ONNX Runtime DLL extracted");
        Ok(())
    }


    async fn download_file(
        &self,
        http: &reqwest::Client,
        file_name: &str,
        dest: &Path,
        progress: Option<&ProgressCallback>,
    ) -> Result<()> {
        let url = format!("{}/{}", self.base_url, file_name);
        log::info!("Downloading model file: {} -> {:?}", url, dest);

        let resp = http
            .get(&url)
            .send()
            .await
            .map_err(|e| EmbeddingError::Download(format!("HTTP request failed: {}", e)))?;

        if !resp.status().is_success() {
            return Err(EmbeddingError::Download(format!(
                "HTTP {} for {}",
                resp.status(),
                url
            )));
        }

        let total = resp.content_length().unwrap_or(0);

        // Stream bytes to temp file, then rename (atomic-ish on Windows).
        let tmp = dest.with_extension("download_tmp");
        let mut file = std::fs::File::create(&tmp)
            .map_err(|e| EmbeddingError::Download(format!("Failed to create temp file: {}", e)))?;

        use std::io::Write;

        let mut downloaded: u64 = 0;
        let report_every: u64 = 4 * 1024 * 1024; // report every ~4 MB
        let mut since_last_report: u64 = 0;

        let mut stream = resp.bytes_stream();
        use futures_util::StreamExt;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk
                .map_err(|e| EmbeddingError::Download(format!("Stream error: {}", e)))?;
            file.write_all(&chunk)
                .map_err(|e| EmbeddingError::Download(format!("Write error: {}", e)))?;
            downloaded += chunk.len() as u64;

            since_last_report += chunk.len() as u64;
            if since_last_report >= report_every || (total > 0 && downloaded >= total) {
                since_last_report = 0;
                if let Some(cb) = progress {
                    cb(DownloadProgress::new(file_name, downloaded, total));
                }
            }
        }

        // Final progress report.
        if let Some(cb) = progress {
            cb(DownloadProgress::new(file_name, downloaded, total));
        }

        drop(file);

        // Atomic rename.
        std::fs::rename(&tmp, dest)
            .map_err(|e| EmbeddingError::Download(format!("Rename failed: {}", e)))?;

        log::info!("Downloaded {} ({} bytes)", file_name, downloaded);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_complete_empty_dir() {
        let dir = std::env::temp_dir().join("dpet_test_dl_empty");
        let _ = std::fs::remove_dir_all(&dir);
        let dl = ModelDownloader::new(&dir);
        assert!(!dl.check_complete());
        assert!(!dl.quantized_complete());
        let missing = dl.missing_files();
        assert_eq!(missing.len(), REQUIRED_FILES.len());
    }

    #[test]
    fn test_check_complete_with_files() {
        let dir = std::env::temp_dir().join("dpet_test_dl_files");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in REQUIRED_FILES {
            std::fs::write(dir.join(f), "placeholder").unwrap();
        }
        let dl = ModelDownloader::new(&dir);
        assert!(dl.check_complete());
        assert!(dl.quantized_complete());
        assert!(dl.missing_files().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_check_complete_with_legacy_fp32_files() {
        // Existing installs with only the fp32 model still count as complete,
        // and missing_files() reports empty so the status UI shows "present".
        let dir = std::env::temp_dir().join("dpet_test_dl_legacy");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in LEGACY_FP32_FILES {
            std::fs::write(dir.join(f), "placeholder").unwrap();
        }
        let dl = ModelDownloader::new(&dir);
        assert!(dl.check_complete());
        assert!(!dl.quantized_complete(), "fp32-only install is not quantized-complete");
        assert!(dl.missing_files().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_check_complete_empty_file() {
        let dir = std::env::temp_dir().join("dpet_test_dl_empty_file");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in REQUIRED_FILES {
            std::fs::write(dir.join(f), "placeholder").unwrap();
        }
        // Overwrite the quantized model file with empty content
        std::fs::write(dir.join(QUANTIZED_MODEL_FILE), "").unwrap();
        let dl = ModelDownloader::new(&dir);
        assert!(!dl.check_complete());
        let missing = dl.missing_files();
        assert!(missing.contains(&QUANTIZED_MODEL_FILE.to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_download_progress_fraction() {
        let p = DownloadProgress::new("model.onnx", 500, 1000);
        assert!((p.fraction - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_download_progress_zero_total() {
        let p = DownloadProgress::new("config.json", 0, 0);
        assert!((p.fraction - 0.0).abs() < 0.001);
    }
}
