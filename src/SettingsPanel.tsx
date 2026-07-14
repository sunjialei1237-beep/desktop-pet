import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

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

  useEffect(() => {
    invoke<LlmConfig>("get_llm_config")
      .then((c) => {
        setBaseUrl(c.base_url);
        setMainModel(c.main_model);
        setReflectionModel(c.reflection_model);
      })
      .catch(() => {});
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
      </div>
    </div>
  );
}
