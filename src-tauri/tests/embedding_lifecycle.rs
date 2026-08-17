//! Embedding lifecycle harness (P2 memory reduction: lazy load + idle unload).
//!
//! Exercises the full residency cycle against the REAL model files, the same
//! way production does: a lazy service starts with no model resident, the
//! first embed loads it on demand, the idle watcher unloads it, and the next
//! embed loads it back — with the resulting vector identical across
//! load/unload cycles (same model, deterministic inference).
//!
//! Also pins the startup-reconcile invariant: `current_model_key(dir)` (file
//! inspection, no ONNX load) must equal the key of the actually loaded model,
//! so a lazily-absent service reconciles vectors to the same space the first
//! embed will use.
//!
//! Uses the real model dir from the live config; skips (early-pass) when no
//! complete file set exists so this stays green on machines without the model.
//! Run:
//!   cargo test --test embedding_lifecycle -- --nocapture --test-threads=1

use desktop_pet_lib::config;
use desktop_pet_lib::embedding::{current_model_key, EmbeddingService, EMBEDDING_DIM};

#[test]
fn lazy_load_idle_unload_reload_cycle() {
    let config = config::load_config().unwrap_or_default();
    let model_dir = config::resolve_model_dir(&config);
    let svc = EmbeddingService::new(&model_dir).with_lazy(true, 30);
    if !svc.files_present() {
        println!("[skip] no complete model file set at {}", model_dir.display());
        return;
    }

    // 1. Lazy start: files present but nothing resident.
    assert!(svc.files_present());
    assert!(!svc.is_ready(), "lazy service must not preload the model");
    assert_eq!(svc.lifecycle_stats(), (0, 0));

    // 2. File-inspection key matches the key of the model the first embed
    //    will actually load (startup reconcile relies on this).
    // 3. First embed lazy-loads on demand.
    let v1 = svc.embed("她下周要去面试前端实习").expect("lazy embed");
    assert_eq!(v1.len(), EMBEDDING_DIM);
    assert!(svc.is_ready(), "embed must have loaded the model");
    let (loads, unloads) = svc.lifecycle_stats();
    assert_eq!((loads, unloads), (1, 0));
    let loaded_key = svc.model_key().expect("model key after lazy load");
    assert_eq!(loaded_key, current_model_key(&model_dir));

    // 4. Recently used -> idle unload must yield.
    assert!(!svc.unload_if_idle(), "must not unload a freshly used model");
    assert!(svc.is_ready());

    // 5. Idle expiry -> unload drops residency, clears the key.
    svc.force_idle_for_test();
    assert!(svc.unload_if_idle(), "idle model must unload");
    assert!(!svc.is_ready(), "model must be gone after idle unload");
    assert!(svc.model_key().is_none());
    let (loads, unloads) = svc.lifecycle_stats();
    assert_eq!((loads, unloads), (1, 1));

    // 6. Next embed transparently reloads and produces the SAME vector
    //    (same model file, deterministic): no mixed-space surprise.
    let v2 = svc.embed("她下周要去面试前端实习").expect("re-load embed");
    assert_eq!(v2.len(), EMBEDDING_DIM);
    assert!(svc.is_ready());
    let (loads, unloads) = svc.lifecycle_stats();
    assert_eq!((loads, unloads), (2, 1));
    for (a, b) in v1.iter().zip(v2.iter()) {
        assert!(
            (a - b).abs() < 1e-5,
            "vector drifted across load/unload cycle: {} vs {}",
            a,
            b
        );
    }
    println!(
        "[lifecycle] load/unload cycle OK (dim={}, 2 loads, 1 unload)",
        EMBEDDING_DIM
    );
}
