import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

interface DebugSnapshot {
  emotion: {
    mood: number;
    mood_label: string;
    physical_energy: number;
    social_battery: number;
    stress: number;
    loneliness: number;
    rest_need: number;
  };
  closeness: number;
  trust: number;
  days_known: number;
  total_conversations: number;
  episode_count: number;
  fact_count: number;
  pending_count: number;
  recent_episodes: { id: string; summary: string; strength: number; recall_count: number }[];
  recent_facts: { id: string; category: string; key: string; value: string; confidence: number }[];
  pending_events: { id: string; title: string; status: string; remind_date: string | null }[];
  change_log: { timestamp: string; module: string; action: string; target: string | null; field: string | null; old_value: string | null; new_value: string | null; reason: string | null }[];
  last_decision: {
    at: string;
    intent_goal: string;
    intent_tone: string;
    intent_action: string;
    memory_anchor: string;
    trigger_reason: string;
    route: string;
    grounding_violations: string[];
    retrieved: { summary: string; score: number; semantic: number; strength: number; recency: number; emotion: number }[];
    prompt_tokens: { system_tokens: number; input_tokens: number; budget: number; conversation_turns: number } | null;
  } | null;
  reflect: {
    last_thought: string | null;
    last_at: string | null;
    unsurfaced_thoughts: number;
  };
  cost: {
    date: string;
    calls: number;
    prompt_tokens: number;
    completion_tokens: number;
  };
  llm_configured: boolean;
  continuous_work_secs: number;
  is_deep_focus: boolean;
}

interface JobStat {
  name: string;
  cadence: string;
  enabled: boolean;
  disableable: boolean;
  last_run_at: string | null;
  last_status: string; // "idle" | "ok" | "skipped" | "error"
  last_message: string | null;
}

const EMO_KEYS = ["mood", "physical_energy", "social_battery", "stress", "loneliness", "rest_need"] as const;
type EmoKey = (typeof EMO_KEYS)[number];
type EmoDraft = Record<EmoKey, number>;

export function DebugPanel({ anim, onClose, onQuit }: {
  anim: { state: string; history: string[] };
  onClose: () => void;
  onQuit: () => void;
}) {
  const [snapshot, setSnapshot] = useState<DebugSnapshot | null>(null);
  const [jobs, setJobs] = useState<JobStat[]>([]);
  const [editError, setEditError] = useState<string | null>(null);
  // Debounce timer for drag-to-apply (sliders apply live, 250ms settle).
  const applyTimerRef = useRef<number | null>(null);
  // Emotion editor draft. Seeded once from the live state on first load, then
  // left alone (current values stay visible in the Brain row above) so polling
  // never overwrites the user's in-progress slider drags.
  const [emoDraft, setEmoDraft] = useState<EmoDraft>({
    mood: 0.5, physical_energy: 0.5, social_battery: 0.5, stress: 0.3, loneliness: 0.3, rest_need: 0.3,
  });
  const emoInitRef = useRef(false);

  const refresh = useCallback(() => {
    invoke<DebugSnapshot>("get_debug_snapshot")
      .then((s) => {
        setSnapshot(s);
        if (!emoInitRef.current) {
          emoInitRef.current = true;
          setEmoDraft({
            mood: s.emotion.mood,
            physical_energy: s.emotion.physical_energy,
            social_battery: s.emotion.social_battery,
            stress: s.emotion.stress,
            loneliness: s.emotion.loneliness,
            rest_need: s.emotion.rest_need,
          });
        }
      })
      .catch(() => {});
    invoke<JobStat[]>("get_scheduler_stats").then(setJobs).catch(() => {});
  }, []);

  useEffect(() => {
    refresh();
    const timer = setInterval(refresh, 2000);
    return () => clearInterval(timer);
  }, [refresh]);

  // Run a curation action, clear any prior error, and refresh so the edited
  // row disappears at once (no waiting on the 2s poll).
  const mutate = useCallback((action: Promise<unknown>) => {
    action
      .then(() => { setEditError(null); refresh(); })
      .catch((e) => setEditError(typeof e === "string" ? e : JSON.stringify(e)));
  }, [refresh]);

  // Drag-to-apply: sliders take effect live (250ms debounce) instead of
  // requiring a button press — a common "no visible change" cause was
  // dragging without clicking Apply.
  const applyEmotion = useCallback((draft: EmoDraft) => {
    if (applyTimerRef.current) window.clearTimeout(applyTimerRef.current);
    applyTimerRef.current = window.setTimeout(() => {
      mutate(invoke("set_emotion", { edit: draft }));
    }, 250);
  }, [mutate]);

  if (!snapshot) return null;

  return (
    <div className="debug-panel">
      <div className="debug-toolbar">
        <span className="debug-title">Debug</span>
        <span className="debug-hint">独立窗口 · 标题栏可拖到任意位置 · ✕ 关闭</span>
        <button className="debug-btn" type="button" onClick={onClose}>✕ 关闭面板</button>
        <button className="debug-btn debug-btn-danger" type="button" onClick={() => { onClose(); onQuit(); }}>⏻ 退出桌宠</button>
      </div>

      <div className="debug-section">
        <span className="debug-title">Emotion 编辑器（拖动即时生效）</span>
        {EMO_KEYS.map((k) => (
          <div key={k} className="debug-bar">
            <label className="debug-slider">
              <span>{k}</span>
              <input type="range" min={0} max={1} step={0.05}
                value={emoDraft[k]}
                onChange={(e) => {
                  const v = parseFloat(e.target.value);
                  const next = { ...emoDraft, [k]: v };
                  setEmoDraft(next);
                  applyEmotion(next);
                }} />
              <span>{emoDraft[k].toFixed(2)}</span>
            </label>
          </div>
        ))}
        <div className="debug-bar">
          <button className="debug-btn" type="button"
            onClick={() => mutate(invoke("set_emotion", { edit: emoDraft }))}>Apply emotion</button>
          {editError && <span className="debug-err"> ⚠ {editError}</span>}
        </div>
      </div>

      <div className="debug-section">
        <span className="debug-title">Face State（由后端情绪值计算）</span>
        {(() => {
          const e = snapshot.emotion;
          const fatigue = Math.max(0, 0.6 - e.physical_energy) * 1.4 + e.rest_need * 0.4;
          const halfOpen = fatigue > 0.5;
          const smiling = !halfOpen && e.mood > 0.65;
          return (
            <>
              <div className="debug-bar">
                <span>fatigue <strong>{fatigue.toFixed(2)}</strong>（阈值 0.5）| mood {e.mood.toFixed(2)}（笑眯阈值 0.65）| energy {e.physical_energy.toFixed(2)} | rest_need {e.rest_need.toFixed(2)}</span>
              </div>
              <div className="debug-bar">
                <span>
                  <strong className={halfOpen ? "debug-on" : ""}>{halfOpen ? "▶ 半睁眼 ON" : "半睁眼 OFF"}</strong>
                  {" | "}
                  <strong className={smiling ? "debug-on" : ""}>{smiling ? "▶ 笑眯眼+微笑嘴 ON" : "笑眯眼 OFF"}</strong>
                  <span className="debug-hint">（渲染端 blink/smile 动画期间会让位）</span>
                </span>
              </div>
              <div className="debug-bar">
                <span className="debug-hint">拖滑块后这里应实时切换 → 确认情绪→表情映射链路在工作</span>
              </div>
            </>
          );
        })()}
      </div>

      <div className="debug-section">
        <span className="debug-title">AnimFSM</span>
        <div className="debug-bar">
          <span>State: {anim.state}{anim.history.length > 0 ? "" : " (no history yet)"}</span>
        </div>
        {anim.history.length > 0 && (
          <div className="debug-bar">
            <span>Recent: {anim.history.slice().reverse().join(" ← ")}</span>
          </div>
        )}
      </div>

      <div className="debug-section">
        <span className="debug-title">Focus</span>
        <div className="debug-bar">
          <span>{snapshot.is_deep_focus ? "🔒 深度专注中（抑制主动气泡）" : `连续工作 ${Math.floor(snapshot.continuous_work_secs / 60)} min（≥25 min 进入专注）`}</span>
        </div>
      </div>

      <div className="debug-section">
        <span className="debug-title">Scheduler</span>
        <span className="debug-hint">后台任务心跳（#6 可关 / #11 可观测）。能力项在 config [scheduler] 关闭后显示 skipped。</span>
        {jobs.map((j) => {
          const statusIcon =
            j.last_status === "ok" ? "✅" :
            j.last_status === "skipped" ? "⏭️" :
            j.last_status === "error" ? "⚠️" : "⏸️";
          return (
            <div className="debug-bar" key={j.name}>
              <span>
                {statusIcon} <strong>{j.name}</strong> · {j.cadence}
                {!j.enabled && <em>（已关闭）</em>}
                {j.disableable && j.enabled && <sub> 可关</sub>}
                {j.last_run_at
                  ? ` · 最近 ${j.last_run_at.slice(11, 19)}`
                  : " · 尚未运行"}
                {j.last_message ? ` · ${j.last_message}` : ""}
              </span>
            </div>
          );
        })}
      </div>

      <div className="debug-section">
        <span className="debug-title">Brain</span>
        <div className="debug-bar">
          <span> Mood {snapshot.emotion.mood.toFixed(2)} ({snapshot.emotion.mood_label})</span>
        </div>
        <div className="debug-bar">
          <span>Energy {snapshot.emotion.physical_energy.toFixed(2)} | Social {snapshot.emotion.social_battery.toFixed(2)} | Stress {snapshot.emotion.stress.toFixed(2)} | Lonely {snapshot.emotion.loneliness.toFixed(2)}</span>
        </div>
        <div className="debug-bar">
          <span>Closeness {snapshot.closeness.toFixed(0)}/100 | Trust {snapshot.trust.toFixed(0)} | {snapshot.days_known}d | {snapshot.total_conversations} chats</span>
        </div>
      </div>

      <div className="debug-section">
        <span className="debug-title">Counts</span>
        <div className="debug-bar">
          <span>Episodes: {snapshot.episode_count} | Facts: {snapshot.fact_count} | Pending: {snapshot.pending_count} | LLM: {snapshot.llm_configured ? "OK" : "N/A"}</span>
        </div>
      </div>

      <div className="debug-section">
        <span className="debug-title">Cost (today)</span>
        <div className="debug-bar">
          <span>{snapshot.cost.calls} LLM calls | prompt {snapshot.cost.prompt_tokens} / completion {snapshot.cost.completion_tokens} tok ({snapshot.cost.date})</span>
        </div>
      </div>

      {snapshot.recent_facts.length > 0 && (
        <div className="debug-section">
          <span className="debug-title">Facts</span>
          {snapshot.recent_facts.map((f) => (
            <div key={f.id} className="debug-item">
              <span>[{f.category}] {f.key}: {f.value} ({f.confidence.toFixed(2)})</span>
              <button className="debug-x" type="button"
                title="忘掉这条 fact（软删除，可重新习得）"
                onClick={() => {
                  if (window.confirm(`忘掉这条记忆？\n[${f.category}] ${f.key}: ${f.value}`)) {
                    mutate(invoke("forget_fact", { id: f.id }));
                  }
                }}>✕</button>
            </div>
          ))}
        </div>
      )}

      {snapshot.recent_episodes.length > 0 && (
        <div className="debug-section">
          <span className="debug-title">Episodes</span>
          {snapshot.recent_episodes.map((e) => (
            <div key={e.id} className="debug-item">
              <span>{e.summary} (str {e.strength.toFixed(2)}, x{e.recall_count})</span>
              <button className="debug-x" type="button"
                title="删除这段 episode 及其向量（地标记忆不可删）"
                onClick={() => {
                  if (window.confirm(`删除这段记忆？\n${e.summary}`)) {
                    mutate(invoke("delete_episode", { id: e.id }));
                  }
                }}>✕</button>
            </div>
          ))}
        </div>
      )}

      {snapshot.pending_events.length > 0 && (
        <div className="debug-section">
          <span className="debug-title">Pending</span>
          {snapshot.pending_events.map((p) => (
            <div key={p.id} className="debug-item">
              <span>[{p.status}] {p.title} {p.remind_date ? `(${p.remind_date.split('T')[0]})` : ''}</span>
              {p.status === "pending" && (
                <button className="debug-x" type="button" title="取消这条提醒"
                  onClick={() => mutate(invoke("resolve_pending_event", { eventId: p.id }))}>✕</button>
              )}
            </div>
          ))}
        </div>
      )}

      {snapshot.change_log.length > 0 && (
        <div className="debug-section">
          <span className="debug-title">Timeline</span>
          {snapshot.change_log.slice(0, 8).map((c, i) => (
            <div key={i} className="debug-item">
              [{c.module}:{c.action}] {c.new_value ?? ''} {c.reason ? `(${c.reason})` : ''}
            </div>
          ))}
        </div>
      )}

      {snapshot.last_decision && (
        <div className="debug-section">
          <span className="debug-title">Last Turn</span>
          <div className="debug-bar">
            <span>Intent: {snapshot.last_decision.intent_goal} / {snapshot.last_decision.intent_tone} / {snapshot.last_decision.intent_action}</span>
          </div>
          {snapshot.last_decision.memory_anchor && (
            <div className="debug-bar">
              <span>Anchor: {snapshot.last_decision.memory_anchor}</span>
            </div>
          )}
          <div className="debug-bar">
            <span>Route: {snapshot.last_decision.route} | Trigger: {snapshot.last_decision.trigger_reason}</span>
          </div>
          {snapshot.last_decision.grounding_violations.length > 0 && (
            <div className="debug-item">
              ⚠ {snapshot.last_decision.grounding_violations.length} grounding violation(s)
            </div>
          )}
          {snapshot.last_decision.prompt_tokens && (
            <div className="debug-bar">
              <span>Prompt: sys {snapshot.last_decision.prompt_tokens.system_tokens}/{snapshot.last_decision.prompt_tokens.budget} tok | input {snapshot.last_decision.prompt_tokens.input_tokens} ({snapshot.last_decision.prompt_tokens.conversation_turns} turns)</span>
            </div>
          )}
        </div>
      )}

      {snapshot.last_decision && snapshot.last_decision.retrieved.length > 0 && (
        <div className="debug-section">
          <span className="debug-title">Retrieved</span>
          {snapshot.last_decision.retrieved.map((r, i) => (
            <div key={i} className="debug-item">
              {r.summary} ({r.score.toFixed(2)}: sem {r.semantic.toFixed(2)} str {r.strength.toFixed(2)} rec {r.recency.toFixed(2)} emo {r.emotion.toFixed(2)})
            </div>
          ))}
        </div>
      )}

      <div className="debug-section">
        <span className="debug-title">Reflect</span>
        <div className="debug-bar">
          <span>Unsurfaced thoughts: {snapshot.reflect.unsurfaced_thoughts}</span>
        </div>
        {snapshot.reflect.last_thought && (
          <div className="debug-item">{snapshot.reflect.last_thought}</div>
        )}
      </div>
    </div>
  );
}
