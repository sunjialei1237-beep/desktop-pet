import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

interface LlmConfig {
  base_url: string;
  api_key_set: boolean;
  main_model: string;
  reflection_model: string;
}

interface SettingsPanelProps {
  onClose: () => void;
}

export function SettingsPanel({ onClose }: SettingsPanelProps) {
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [mainModel, setMainModel] = useState("");
  const [reflectionModel, setReflectionModel] = useState("");
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [embReady, setEmbReady] = useState(false);
  const [embFilesPresent, setEmbFilesPresent] = useState(false);
  const [embDownloading, setEmbDownloading] = useState(false);
  const [embProgress, setEmbProgress] = useState<string>("");
  // P2 lazy lifecycle: "Standby" = files on disk, model not resident (loads on
  // first use, unloads after idle). Null until the status query resolves.
  const [embLazy, setEmbLazy] = useState<boolean | null>(null);
  const [embLoaded, setEmbLoaded] = useState(true);
  const [embLoadCount, setEmbLoadCount] = useState(0);
  const [embUnloadCount, setEmbUnloadCount] = useState(0);

  useEffect(() => {
    invoke<LlmConfig>("get_llm_config")
      .then((c) => {
        setBaseUrl(c.base_url);
        setMainModel(c.main_model);
        setReflectionModel(c.reflection_model);
      })
      .catch(() => {});
    invoke<{
      ready: boolean;
      files_present: boolean;
      lazy_load: boolean;
      loaded: boolean;
      load_count: number;
      unload_count: number;
    }>("get_embedding_status")
      .then((s) => {
        setEmbReady(s.ready);
        setEmbFilesPresent(s.files_present);
        setEmbLazy(s.lazy_load);
        setEmbLoaded(s.loaded);
        setEmbLoadCount(s.load_count);
        setEmbUnloadCount(s.unload_count);
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    listen<{ file_name: string; downloaded: number; total: number; fraction: number }>(
      "download-progress",
      (e) => {
        const pct = Math.round(e.payload.fraction * 100);
        setEmbProgress(`${e.payload.file_name}: ${pct}%`);
      }
    ).then((un) => (unlisten = un));
    return () => { if (unlisten) unlisten(); };
  }, []);

  const handleDownloadModel = useCallback(async () => {
    setEmbDownloading(true);
    setEmbProgress("Starting download...");
    try {
      await invoke<boolean>("download_embedding_model");
      setEmbReady(true);
      setEmbFilesPresent(true);
      setEmbLoaded(true);
      setEmbProgress("");
    } catch (e) {
      setEmbProgress(`Error: ${String(e)}`);
    }
    setEmbDownloading(false);
  }, []);

  const handleSave = useCallback(async () => {
    setSaving(true);
    try {
      await invoke("update_llm_config", {
        baseUrl,
        apiKey,
        mainModel,
        reflectionModel,
      });
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch {
      // ignore
    }
    setSaving(false);
  }, [baseUrl, apiKey, mainModel, reflectionModel]);

  return (
    <div className="settings-overlay" onClick={onClose}>
      <div className="settings-panel" onClick={(e) => e.stopPropagation()}>
        <div className="settings-header">
          <span>LLM Settings</span>
          <button className="settings-close" onClick={onClose}>&times;</button>
        </div>

        <label>API Base URL</label>
        <input
          type="text"
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          placeholder="https://api.deepseek.com/v1"
        />

        <label>API Key {apiKey === "" ? "" : "(changed)"}</label>
        <input
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="sk-..."
        />

        <label>Main Model</label>
        <input
          type="text"
          value={mainModel}
          onChange={(e) => setMainModel(e.target.value)}
          placeholder="deepseek-chat"
        />

        <label>Reflection Model</label>
        <input
          type="text"
          value={reflectionModel}
          onChange={(e) => setReflectionModel(e.target.value)}
          placeholder="deepseek-chat"
        />

        <button
          className="settings-save"
          onClick={handleSave}
          disabled={saving}
        >
          {saving ? "..." : saved ? "OK" : "Save"}
        </button>

        <div className="settings-divider" />

        <div className="settings-header">
          <span>Memory Model</span>
        </div>
        <div className="emb-status">
          {embReady && (!embLazy || embLoaded) ? (
            <span className="emb-badge emb-ok">Ready</span>
          ) : embReady && embLazy ? (
            <span className="emb-badge emb-warn">
              Standby (lazy load, unloads when idle)
            </span>
          ) : embFilesPresent ? (
            <span className="emb-badge emb-warn">Files present (not loaded)</span>
          ) : (
            <span className="emb-badge emb-missing">Not downloaded</span>
          )}
          {embLazy && embLoadCount + embUnloadCount > 0 && (
            <span className="emb-hint">
              {" "}
              loads: {embLoadCount} / unloads: {embUnloadCount}
            </span>
          )}
        </div>
        {embProgress && <p className="emb-progress-text">{embProgress}</p>}
        {!embFilesPresent && (
          <button
            className="settings-save emb-download-btn"
            onClick={handleDownloadModel}
            disabled={embDownloading}
          >
            {embDownloading ? "Downloading..." : "Download BGE-M3 Model"}
          </button>
        )}
        <p className="emb-hint">Local semantic search for memory recall (int8, ~570 MB; lazy-loaded)</p>
      </div>
    </div>
  );
}
