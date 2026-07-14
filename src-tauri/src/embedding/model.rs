use crate::embedding::error::{EmbeddingError, Result};
use ndarray::{Array1, Array2, ArrayBase, Data, Ix2};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Value;
use std::path::Path;
use tokenizers::Tokenizer;

/// Expected dense embedding dimension for BGE-M3.
pub const EMBEDDING_DIM: usize = 1024;
/// Max sequence length for BGE-M3 tokenizer.
const MAX_SEQ_LEN: usize = 8192;

/// Holds the loaded ONNX session + tokenizer.
/// Thread-safe to share via `Arc<EmbeddingModel>` for inference.
pub struct EmbeddingModel {
    session: Session,
    tokenizer: Tokenizer,
}

impl EmbeddingModel {
    /// Loads the ONNX model and tokenizer from the given directory.
    /// Expected files: model.onnx, tokenizer.json
    /// Also expects onnxruntime.dll in the model dir (for load-dynamic feature).
    pub fn load(model_dir: &Path) -> Result<Self> {
        // For ort load-dynamic: set the ORT_DYLIB_PATH env var so ort can
        // locate the ONNX Runtime DLL at runtime.
        let dll_path = model_dir.join("onnxruntime.dll");
        if dll_path.exists() {
            std::env::set_var("ORT_DYLIB_PATH", &dll_path);
            log::info!("ORT_DYLIB_PATH set to {:?}", dll_path);
        } else {
            log::warn!("onnxruntime.dll not found in model dir; ort will try system PATH");
        }

        let onnx_path = model_dir.join("model.onnx");
        let tokenizer_path = model_dir.join("tokenizer.json");

        if !onnx_path.exists() {
            return Err(EmbeddingError::ModelFilesMissing(format!(
                "{:?}",
                onnx_path
            )));
        }
        if !tokenizer_path.exists() {
            return Err(EmbeddingError::ModelFilesMissing(format!(
                "{:?}",
                tokenizer_path
            )));
        }

        log::info!("Loading ONNX model from {:?}", onnx_path);

        let session = Session::builder()
            .map_err(|e| EmbeddingError::Onnx(format!("Session builder: {}", e)))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| EmbeddingError::Onnx(format!("Opt level: {}", e)))?
            .with_intra_threads(2)
            .map_err(|e| EmbeddingError::Onnx(format!("Threads: {}", e)))?
            .commit_from_file(&onnx_path)
            .map_err(|e| EmbeddingError::Onnx(format!("Load model: {}", e)))?;

        let mut tokenizer =
            Tokenizer::from_file(&tokenizer_path).map_err(|e| {
                EmbeddingError::Tokenizer(format!("Load tokenizer: {}", e))
            })?;

        // BGE-M3 truncates to max length; no padding needed for single-text embed.
        tokenizer.with_truncation(Some(tokenizers::TruncationParams {
            max_length: MAX_SEQ_LEN,
            ..Default::default()
        }))
        .map_err(|e| EmbeddingError::Tokenizer(format!("Truncation: {}", e)))?;

        log::info!("ONNX model loaded successfully");

        Ok(EmbeddingModel { session, tokenizer })
    }

    /// Embeds a single text string into a 1024-dim normalized vector.
    pub fn embed(&mut self, text: &str) -> Result<Vec<f32>> {
        let (input_ids, attention_mask) = self.tokenize_single(text)?;
        let hidden = self.run_inference(&input_ids, &attention_mask)?;
        let pooled = mean_pool(&hidden.view(), &attention_mask);
        Ok(l2_normalize(&pooled))
    }

    /// Embeds multiple texts (sequential inference; can be parallelized later).
    pub fn embed_batch(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }

    /// Tokenizes a single text, returning (input_ids, attention_mask) as i64 tensors.
    fn tokenize_single(&self, text: &str) -> Result<(Array2<i64>, Array2<i64>)> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| EmbeddingError::Tokenizer(format!("Encode: {}", e)))?;

        let ids: Vec<i64> = encoding.get_ids().iter().map(|&v| v as i64).collect();
        let mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&v| v as i64)
            .collect();

        let seq_len = ids.len();
        let input_ids = Array2::from_shape_vec((1, seq_len), ids)
            .map_err(|e| EmbeddingError::Tokenizer(format!("Shape input_ids: {}", e)))?;
        let attention_mask = Array2::from_shape_vec((1, seq_len), mask)
            .map_err(|e| EmbeddingError::Tokenizer(format!("Shape attention_mask: {}", e)))?;

        Ok((input_ids, attention_mask))
    }

    /// Runs the ONNX session and extracts the last_hidden_state output.
    /// Returns a [seq_len, hidden_dim] array.
    fn run_inference(
        &mut self,
        input_ids: &Array2<i64>,
        attention_mask: &Array2<i64>,
    ) -> Result<Array2<f32>> {
        // Pass Array2<i64> directly: ort 2.0.0-rc.12 accepts owned arrays.
        // but we must avoid into_dyn() which causes ndarray version conflicts.
        let ids_value =
            Value::from_array(input_ids.clone()).map_err(|e| {
                EmbeddingError::Onnx(format!("Create input_ids tensor: {}", e))
            })?;
        let mask_value = Value::from_array(attention_mask.clone())
            .map_err(|e| EmbeddingError::Onnx(format!("Create attention_mask tensor: {}", e)))?;

        let inputs = ort::inputs!["input_ids" => ids_value, "attention_mask" => mask_value];

        let outputs = self
            .session
            .run(inputs)
            .map_err(|e| EmbeddingError::Onnx(format!("Session run: {}", e)))?;

        // Try known output names, fall back to first output via index.
        let output_value = outputs
            .get("last_hidden_state")
            .or_else(|| outputs.get("sentence_embedding"))
            .ok_or_else(|| EmbeddingError::Onnx("No output from model".to_string()))?;

        // In ort 2.0.0-rc.12, try_extract_tensor returns (&Shape, &[f32]).
        let (shape, data) = output_value
            .try_extract_tensor::<f32>()
            .map_err(|e| EmbeddingError::Onnx(format!("Extract output: {}", e)))?;

        let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();

        match dims.len() {
            // [1, seq_len, hidden_dim] - need mean pooling
            3 => {
                let seq_len = dims[1];
                let hidden_dim = dims[2];
                Array2::from_shape_vec((seq_len, hidden_dim), data.to_vec())
                    .map_err(|e| EmbeddingError::Onnx(format!("Reshape 3D output: {}", e)))
            }
            // [1, hidden_dim] - already pooled (some export configs do this)
            2 => {
                let hidden_dim = dims[1];
                Array2::from_shape_vec((1, hidden_dim), data.to_vec())
                    .map_err(|e| EmbeddingError::Onnx(format!("Reshape 2D output: {}", e)))
            }
            _ => Err(EmbeddingError::Onnx(format!(
                "Unexpected output ndim: {}",
                dims.len()
            ))),
        }
    }
}

/// Mean pooling: averages token embeddings weighted by the attention mask.
fn mean_pool(hidden: &ArrayBase<impl Data<Elem = f32>, Ix2>, attention_mask: &Array2<i64>) -> Array1<f32> {
    let seq_len = hidden.nrows();
    let dim = hidden.ncols();

    let mut sum = Array1::zeros(dim);
    let mut count = 0.0f32;

    for i in 0..seq_len {
        let mask_val = attention_mask[[0, i]] as f32;
        if mask_val > 0.0 {
            sum += &hidden.row(i).mapv(|v| v * mask_val);
            count += mask_val;
        }
    }

    if count > 0.0 {
        sum /= count;
    }
    sum
}

/// L2 normalization: divides by the L2 norm so the result has unit length.
fn l2_normalize(vec: &Array1<f32>) -> Vec<f32> {
    let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        vec.iter().map(|v| v / norm).collect()
    } else {
        vec.to_vec()
    }
}

/// Cosine similarity between two equal-length vectors.
/// Assumes inputs are already L2-normalized (so this is just a dot product).
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l2_normalize() {
        let vec = ndarray::array![3.0, 4.0];
        let result = l2_normalize(&vec);
        let norm: f32 = result.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.001,
            "L2 norm should be 1.0, got {}",
            norm
        );
    }

    #[test]
    fn test_l2_normalize_zero() {
        let vec = ndarray::array![0.0, 0.0, 0.0];
        let result = l2_normalize(&vec);
        assert!(result.iter().all(|v| (*v - 0.0).abs() < 0.001));
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &a);
        assert!((sim - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 0.001);
    }

    #[test]
    fn test_mean_pool_basic() {
        // 2 tokens, 3 dim each
        let hidden = ndarray::array![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]];
        let mask = ndarray::array![[1, 1]];
        let result = mean_pool(&hidden, &mask);
        // mean of [1,2,3] and [4,5,6] = [2.5, 3.5, 4.5]
        assert!((result[0] - 2.5).abs() < 0.001);
        assert!((result[1] - 3.5).abs() < 0.001);
        assert!((result[2] - 4.5).abs() < 0.001);
    }

    #[test]
    fn test_mean_pool_with_mask() {
        // Second token masked out
        let hidden = ndarray::array![[1.0, 2.0], [10.0, 20.0]];
        let mask = ndarray::array![[1, 0]];
        let result = mean_pool(&hidden, &mask);
        // Only first token counts
        assert!((result[0] - 1.0).abs() < 0.001);
        assert!((result[1] - 2.0).abs() < 0.001);
    }
}
