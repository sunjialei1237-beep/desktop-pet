import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

interface LlmConfig {
  base_url: string;
  api_key: string;
  api_key_set: boolean;
  main_model: string;
  reflection_model: string;
}

/** Saved switchable model profile (backend `list_llm_profiles`). */
interface LlmProfile {
  name: string;
  base_url: string;
  main_model: string;
  reflection_model: string;
  api_key_set: boolean;
  active: boolean;
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
  // API key visibility: prefilled from the backend (stored locally anyway),
  // eye toggle switches between masked and plain.
  const [showKey, setShowKey] = useState(false);
  const [profiles, setProfiles] = useState<LlmProfile[]>([]);
  const [switching, setSwitching] = useState<string | null>(null);
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
  // 版本与更新：检查 GitHub Releases → 下载签名包 → 静默覆盖安装 → 重启。
  const [appVersion, setAppVersion] = useState("");
  const [updChecking, setUpdChecking] = useState(false);
  const [updAvailable, setUpdAvailable] = useState<Update | null>(null);
  const [updState, setUpdState] = useState<"idle" | "downloading" | "installing">("idle");
  const [updProgress, setUpdProgress] = useState("");
  const [updMsg, setUpdMsg] = useState("");

  useEffect(() => {
    invoke<LlmConfig>("get_llm_config")
      .then((c) => {
        setBaseUrl(c.base_url);
        setApiKey(c.api_key);
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
    refreshProfiles();
  }, []);

  // 桌面惯例：Esc 也能关闭设置，不依赖任何可见控件。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const refreshProfiles = useCallback(async () => {
    try {
      setProfiles(await invoke<LlmProfile[]>("list_llm_profiles"));
    } catch {
      // Profile list is a convenience — never block the panel on it.
    }
  }, []);

  /** One-click switch to a saved profile: applies it immediately (backend
   * rebuilds the LLM client) and refreshes the form to match. */
  const applyProfile = useCallback(
    async (name: string) => {
      setSwitching(name);
      try {
        const c = await invoke<LlmConfig>("apply_llm_profile", { name });
        setBaseUrl(c.base_url);
        setApiKey(c.api_key);
        setMainModel(c.main_model);
        setReflectionModel(c.reflection_model);
        setSaved(true);
        setTimeout(() => setSaved(false), 2000);
        await refreshProfiles();
      } catch {
        // ignore — list refresh keeps the UI truthful
      }
      setSwitching(null);
    },
    [refreshProfiles]
  );

  const deleteProfile = useCallback(
    async (name: string) => {
      try {
        await invoke("delete_llm_profile", { name });
        await refreshProfiles();
      } catch {
        // ignore
      }
    },
    [refreshProfiles]
  );

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

  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => {});
  }, []);

  const handleCheckUpdate = useCallback(async () => {
    setUpdChecking(true);
    setUpdMsg("");
    setUpdAvailable(null);
    try {
      const update = await check();
      if (update) {
        setUpdAvailable(update);
      } else {
        setUpdMsg("已是最新版本 ✓");
      }
    } catch (e) {
      setUpdMsg(`检查更新失败：${String(e)}`);
    }
    setUpdChecking(false);
  }, []);

  const handleDownloadUpdate = useCallback(async () => {
    if (!updAvailable) return;
    setUpdState("downloading");
    setUpdMsg("");
    let downloaded = 0;
    let total = 0;
    try {
      await updAvailable.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          setUpdProgress(
            total > 0
              ? `${(downloaded / 1024 / 1024).toFixed(1)} / ${(total / 1024 / 1024).toFixed(1)} MB（${Math.round((downloaded / total) * 100)}%）`
              : `${(downloaded / 1024 / 1024).toFixed(1)} MB`
          );
        }
      });
      setUpdState("installing");
      await relaunch();
    } catch (e) {
      setUpdMsg(`下载/安装失败：${String(e)}`);
      setUpdState("idle");
    }
  }, [updAvailable]);

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
      // The backend auto-records every saved config as a profile — pull the
      // updated list so it shows up immediately.
      await refreshProfiles();
    } catch {
      // ignore
    }
    setSaving(false);
  }, [baseUrl, apiKey, mainModel, reflectionModel, refreshProfiles]);

  return (
    <div className="settings-overlay" onClick={onClose}>
      <div className="settings-panel" onClick={(e) => e.stopPropagation()}>
        <div className="settings-header">
          <div className="settings-title">
            <span>设置</span>
            <span className="settings-subtitle">模型 · 工具 · 记忆</span>
          </div>
          <button className="settings-close" onClick={onClose} aria-label="关闭设置">&times;</button>
        </div>

        <div className="settings-body">
        <label>API Base URL</label>
        <input
          type="text"
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          placeholder="https://api.deepseek.com/v1"
        />

        <label>API Key {apiKey ? "（已保存）" : ""}</label>
        <div className="settings-key-row">
          <input
            type={showKey ? "text" : "password"}
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="sk-..."
            spellCheck={false}
          />
          <button
            className="settings-key-toggle"
            onClick={() => setShowKey((v) => !v)}
            title={showKey ? "隐藏" : "显示"}
          >
            {showKey ? "🙈" : "👁"}
          </button>
        </div>

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

        <div className="settings-grants">
          <div className="settings-grants-title">已保存的模型（点「使用」一键切换，无需重启）</div>
          {profiles.length === 0 && (
            <span className="emb-hint">暂无——保存过的配置会自动出现在这里</span>
          )}
          {profiles.map((p) => (
            <div className="settings-profile-row" key={p.name}>
              <span
                className="settings-grant-root"
                title={`接口：${p.base_url}\n主模型：${p.main_model}\n反思模型：${p.reflection_model}`}
              >
                {p.name}
                {p.reflection_model !== p.main_model && (
                  <span className="settings-profile-sub"> · {p.reflection_model}</span>
                )}
                {p.active && <span className="settings-profile-active">✓ 使用中</span>}
              </span>
              <button
                className="settings-grant-revoke"
                onClick={() => applyProfile(p.name)}
                disabled={p.active || switching === p.name}
              >
                {switching === p.name ? "切换中…" : p.active ? "当前" : "使用"}
              </button>
              <button className="settings-grant-revoke" onClick={() => deleteProfile(p.name)}>
                删除
              </button>
            </div>
          ))}
        </div>

        <div className="settings-divider" />

        <div className="settings-section-head">工具与授权</div>
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

        <div className="settings-section-head">记忆模型</div>
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

        <div className="settings-divider" />

        <div className="settings-section-head">版本与更新</div>
        <p className="emb-hint">当前版本 v{appVersion || "…"} · 新版本来自 GitHub Releases，下载后自动覆盖安装并重启</p>
        {updAvailable && updState === "idle" && (
          <p className="emb-hint">
            发现新版本 <b>v{updAvailable.version}</b>
            {updAvailable.body ? ` · ${updAvailable.body.split("\n")[0]}` : ""}
          </p>
        )}
        {updProgress && updState === "downloading" && <p className="emb-progress-text">{updProgress}</p>}
        {updMsg && <p className="emb-hint" style={{ color: updMsg.includes("失败") ? "#b3402f" : undefined }}>{updMsg}</p>}
        {updState === "idle" && !updAvailable && (
          <button className="settings-save" onClick={handleCheckUpdate} disabled={updChecking}>
            {updChecking ? "检查中…" : "检查更新"}
          </button>
        )}
        {updState === "idle" && updAvailable && (
          <button className="settings-save emb-download-btn" onClick={handleDownloadUpdate}>
            下载并安装 v{updAvailable.version}
          </button>
        )}
        {updState === "downloading" && (
          <button className="settings-save emb-download-btn" disabled>下载中…</button>
        )}
        {updState === "installing" && (
          <button className="settings-save emb-download-btn" disabled>安装中，即将重启…</button>
        )}
        </div>
      </div>
    </div>
  );
}
