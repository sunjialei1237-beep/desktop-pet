import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

interface LlmConfig {
  base_url: string;
  api_key_set: boolean;
  main_model: string;
  reflection_model: string;
}

interface ToolsConfig {
  enable_search_web: boolean;
  enable_open_application: boolean;
  enable_fs_observe: boolean;
  enable_fs_mutate: boolean;
}

interface FsGrant {
  root: string;
  mode: string;
  created_at: string;
  source: string;
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
  // U5 (plan §8.4): tools capability switches + fs grant observability live
  // INSIDE Settings (not only config.toml / DebugPanel).
  const [tools, setTools] = useState<ToolsConfig | null>(null);
  const [toolsSaved, setToolsSaved] = useState(false);
  const [toolsErr, setToolsErr] = useState("");
  const [grants, setGrants] = useState<FsGrant[]>([]);

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
    invoke<ToolsConfig>("get_tools_config")
      .then(setTools)
      .catch((e) => setToolsErr(String(e)));
    listGrants();
  }, []);

  const listGrants = useCallback(async () => {
    try {
      const g = await invoke<FsGrant[]>("list_fs_grants");
      setGrants(g);
    } catch (e) {
      setToolsErr(String(e));
    }
  }, []);

  const saveTools = useCallback(async (next: ToolsConfig) => {
    setTools(next);
    setToolsSaved(false);
    setToolsErr("");
    try {
      await invoke("save_tools_config", {
        enableSearchWeb: next.enable_search_web,
        enableOpenApplication: next.enable_open_application,
        enableFsObserve: next.enable_fs_observe,
        enableFsMutate: next.enable_fs_mutate,
      });
      setToolsSaved(true);
    } catch (e) {
      setToolsErr(String(e));
    }
  }, []);

  const revokeGrant = useCallback(async (root: string) => {
    try {
      await invoke("fs_revoke_access", { root });
      await listGrants();
    } catch (e) {
      setToolsErr(String(e));
    }
  }, [listGrants]);

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
          <span>Tools &amp; Permission</span>
        </div>
        <p className="emb-hint">功能开关（保存即生效，无需重启）；文件读取与修改还需要按路径授权/确认。</p>
        {tools && (
          <>
            <label className="settings-tools-toggle">
              <input
                type="checkbox"
                checked={tools.enable_search_web}
                onChange={(e) => saveTools({ ...tools, enable_search_web: e.target.checked })}
              />
              <span>联网搜索</span>
            </label>
            <label className="settings-tools-toggle">
              <input
                type="checkbox"
                checked={tools.enable_open_application}
                onChange={(e) => saveTools({ ...tools, enable_open_application: e.target.checked })}
              />
              <span>打开应用 / 网址 / 文件</span>
            </label>
            <label className="settings-tools-toggle">
              <input
                type="checkbox"
                checked={tools.enable_fs_observe}
                onChange={(e) => saveTools({ ...tools, enable_fs_observe: e.target.checked })}
              />
              <span>感知环境与读取授权文件（启用才能真正“看到”你在做什么）</span>
            </label>
            <label className="settings-tools-toggle">
              <input
                type="checkbox"
                checked={tools.enable_fs_mutate}
                onChange={(e) => saveTools({ ...tools, enable_fs_mutate: e.target.checked })}
              />
              <span>写笔记 / 改文件（每次仍要确认）</span>
            </label>
            {toolsSaved && !toolsErr && <p className="emb-hint">✓ 工具开关已保存并即时生效</p>}
            {toolsErr && <p className="emb-hint" style={{ color: "#b3402f" }}>{toolsErr}</p>}
            <div className="settings-grants">
              <div className="settings-grants-title">已授权的文件位置（右键…不对，点右侧按钮可撤销）</div>
              {grants.length === 0 && <span className="emb-hint">暂无路径授权（首次访问会先问你）</span>}
              {grants.map((g) => (
                <div className="settings-grant-row" key={g.root}>
                  <span className="settings-grant-root" title={g.root}>{g.root}</span>
                  <span className="settings-grant-mode">{g.mode}</span>
                  <button className="settings-grant-revoke" onClick={() => revokeGrant(g.root)}>
                    撤销
                  </button>
                </div>
              ))}
            </div>
          </>
        )}
        {!tools && toolsErr && <p className="emb-hint">工具开关加载失败，请检查日志。</p>}

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
