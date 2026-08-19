import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open as openUrl } from "@tauri-apps/plugin-shell";

/** Backend snapshot from `get_setup_state`. */
interface SetupState {
  api_key_set: boolean;
  wizard_done: boolean;
  embedding_files_present: boolean;
  base_url: string;
  main_model: string;
  reflection_model: string;
}

interface DownloadProgressPayload {
  file_name: string;
  downloaded: number;
  total: number;
  fraction: number;
}

/** 一跳一转的用户路径（task: 安装包分发）：
 *  欢迎 → API Key（验证并保存）→ 记忆模型（下载 / 以后再说）→ 完成。
 *  完成后写入 setup_wizard_done，之后启动不再自动弹出。 */
export function SetupWizard({ onClose }: { onClose: () => void }) {
  const [step, setStep] = useState<"welcome" | "apikey" | "model" | "done">("welcome");
  const [baseUrl, setBaseUrl] = useState("https://api.deepseek.com/v1");
  const [apiKey, setApiKey] = useState("");
  const [mainModel, setMainModel] = useState("deepseek-v4-pro");
  const [reflectionModel, setReflectionModel] = useState("deepseek-v4-flash");
  const [verifying, setVerifying] = useState(false);
  const [verifyMsg, setVerifyMsg] = useState<string | null>(null);
  const [verifyError, setVerifyError] = useState<string | null>(null);
  const [modelDownloading, setModelDownloading] = useState(false);
  const [modelProgress, setModelProgress] = useState("");
  const [modelReady, setModelReady] = useState(false);
  const [finishing, setFinishing] = useState(false);

  useEffect(() => {
    invoke<SetupState>("get_setup_state")
      .then((s) => {
        setBaseUrl(s.base_url || "https://api.deepseek.com/v1");
        setMainModel(s.main_model || "deepseek-v4-pro");
        setReflectionModel(s.reflection_model || "deepseek-v4-flash");
        setModelReady(s.embedding_files_present);
      })
      .catch(() => {});
  }, []);

  // Model download progress (same event the Settings panel listens to).
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    listen<DownloadProgressPayload>("download-progress", (e) => {
      const pct = Math.round(e.payload.fraction * 100);
      setModelProgress(`${e.payload.file_name}: ${pct}%`);
    }).then((un) => (unlisten = un));
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const handleGetKey = useCallback(() => {
    void openUrl("https://platform.deepseek.com/");
  }, []);

  const handleVerifyAndSave = useCallback(async () => {
    if (!apiKey.trim()) {
      setVerifyError("请先粘贴你的 API Key。");
      return;
    }
    setVerifying(true);
    setVerifyMsg(null);
    setVerifyError(null);
    try {
      await invoke("update_llm_config", {
        baseUrl,
        apiKey,
        mainModel,
        reflectionModel,
      });
      const reply = await invoke<string>("test_llm_connection");
      setVerifyMsg(reply);
      // 稍等半拍让用户看到成功反馈，再进模型步。
      window.setTimeout(() => setStep("model"), 600);
    } catch (e) {
      setVerifyError(String(e));
    }
    setVerifying(false);
  }, [apiKey, baseUrl, mainModel, reflectionModel]);

  const handleDownloadModel = useCallback(async () => {
    setModelDownloading(true);
    setModelProgress("正在连接下载源…");
    try {
      await invoke<boolean>("download_embedding_model");
      setModelReady(true);
      setModelProgress("");
    } catch (e) {
      setModelProgress(`下载失败：${String(e)}`);
    }
    setModelDownloading(false);
  }, []);

  const handleDone = useCallback(async () => {
    setFinishing(true);
    try {
      await invoke("set_setup_wizard_done");
    } catch {
      // 幂等：写失败也不阻塞进入桌宠。
    }
    onClose();
  }, [onClose]);

  const handleSkip = useCallback(async () => {
    try {
      await invoke("set_setup_wizard_done");
    } catch {
      // 同上，忽略。
    }
    onClose();
  }, [onClose]);

  return (
    <div className="setup-overlay">
      <div className="setup-panel">
        <div className="setup-steps">
          <span className={step === "welcome" ? "setup-step-dot on" : "setup-step-dot"} />
          <span className={step === "apikey" ? "setup-step-dot on" : "setup-step-dot"} />
          <span className={step === "model" ? "setup-step-dot on" : "setup-step-dot"} />
          <span className={step === "done" ? "setup-step-dot on" : "setup-step-dot"} />
        </div>

        {step === "welcome" && (
          <div className="setup-body">
            <h2 className="setup-title">你好，我是璃 🦊</h2>
            <p className="setup-sub">一只住在你桌面的小狐灵。</p>
            <ul className="setup-list">
              <li>我会记住我们聊过的每一件事</li>
              <li>想你的时候会主动来找你说话</li>
              <li>所有数据只保存在你的电脑上</li>
            </ul>
            <p className="setup-hint">开始前需要两样东西：一个 API Key（让<br />我开口说话），以及可选的记忆模型。</p>
            <button className="setup-primary" onClick={() => setStep("apikey")}>
              开始配置
            </button>
            <button className="setup-ghost" onClick={handleSkip}>
              暂时跳过
            </button>
          </div>
        )}

        {step === "apikey" && (
          <div className="setup-body">
            <h2 className="setup-title">连接智能大脑</h2>
            <p className="setup-sub">璃使用 LLM 与你对话（默认 DeepSeek，支持任意 OpenAI 兼容服务）。</p>

            <label className="setup-label">API Base URL</label>
            <input
              className="setup-input"
              type="text"
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              spellCheck={false}
            />

            <label className="setup-label">API Key</label>
            <input
              className="setup-input"
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="sk-..."
              spellCheck={false}
            />
            <button className="setup-link" onClick={handleGetKey}>
              没有 Key？免费获取 DeepSeek API Key ↗
            </button>

            {verifyMsg && <p className="setup-ok">✓ 验证通过：{verifyMsg}</p>}
            {verifyError && <p className="setup-err">{verifyError}</p>}

            <button
              className="setup-primary"
              onClick={handleVerifyAndSave}
              disabled={verifying}
            >
              {verifying ? "验证中…" : "验证并保存"}
            </button>
            <button className="setup-ghost" onClick={handleSkip}>
              跳过（之后可在设置里配置）
            </button>
          </div>
        )}

        {step === "model" && (
          <div className="setup-body">
            <h2 className="setup-title">记忆模型</h2>
            <p className="setup-sub">
              她需要一个本地记忆模型才能真正"记住你"——有了它，跨会话她记得你说过的事，
              并能在合适的时候想起。约 570MB，一次下载，永久使用。
            </p>

            {modelReady ? (
              <p className="setup-ok">✓ 记忆模型已就绪</p>
            ) : (
              <>
                {modelProgress && <p className="setup-hint">{modelProgress}</p>}
                <button
                  className="setup-primary"
                  onClick={handleDownloadModel}
                  disabled={modelDownloading}
                >
                  {modelDownloading ? "正在下载…" : "立即下载（约 570MB）"}
                </button>
              </>
            )}

            <button
              className="setup-ghost"
              onClick={() => setStep("done")}
              disabled={modelDownloading}
            >
              {modelDownloading
                ? "正在下载…"
                : modelReady
                  ? "下一步"
                  : "以后再说（对话正常，只是记性差一点）"}
            </button>
          </div>
        )}

        {step === "done" && (
          <div className="setup-body">
            <h2 className="setup-title">准备好了 ✨</h2>
            <p className="setup-sub">
              双击她、或者在任意窗口按 Alt+Space 就能和她说话。
            </p>
            <p className="setup-hint">
              右键可以打开设置、导出记忆、让她去休息。<br />
              有什么想说的，现在就可以告诉她。
            </p>
            <button className="setup-primary" onClick={handleDone} disabled={finishing}>
              开始和璃相处
            </button>
          </div>
        )}
      </div>
    </div>
  );
}