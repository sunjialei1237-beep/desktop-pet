import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface DebugSnapshot {
  emotion: {
    mood: number;
    mood_label: string;
    physical_energy: number;
    social_battery: number;
    stress: number;
    loneliness: number;
  };
  closeness: number;
  trust: number;
  days_known: number;
  total_conversations: number;
  episode_count: number;
  fact_count: number;
  pending_count: number;
  recent_episodes: { id: string; summary: string; strength: number; recall_count: number }[];
  recent_facts: { category: string; key: string; value: string; confidence: number }[];
  pending_events: { id: string; title: string; status: string; remind_date: string | null }[];
  change_log: { timestamp: string; module: string; action: string; target: string | null; field: string | null; old_value: string | null; new_value: string | null; reason: string | null }[];
  llm_configured: boolean;
}

export function DebugPanel() {
  const [snapshot, setSnapshot] = useState<DebugSnapshot | null>(null);

  useEffect(() => {
    const fetchSnapshot = () => {
      invoke<DebugSnapshot>("get_debug_snapshot")
        .then(setSnapshot)
        .catch(() => {});
    };
    fetchSnapshot();
    const timer = setInterval(fetchSnapshot, 2000);
    return () => clearInterval(timer);
  }, []);

  if (!snapshot) return null;

  return (
    <div className="debug-panel">
      <div className="debug-section">
        <span className="debug-title">Brain</span>
        <div className="debug-bar">
          <span> Mood {snapshot.emotion.mood.toFixed(2)} ({snapshot.emotion.mood_label})</span>
        </div>
        <div className="debug-bar">
          <span>Energy {snapshot.emotion.physical_energy.toFixed(2)} | Social {snapshot.emotion.social_battery.toFixed(2)} | Stress {snapshot.emotion.stress.toFixed(2)}</span>
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

      {snapshot.recent_facts.length > 0 && (
        <div className="debug-section">
          <span className="debug-title">Facts</span>
          {snapshot.recent_facts.map((f, i) => (
            <div key={i} className="debug-item">
              [{f.category}] {f.key}: {f.value} ({f.confidence.toFixed(2)})
            </div>
          ))}
        </div>
      )}

      {snapshot.recent_episodes.length > 0 && (
        <div className="debug-section">
          <span className="debug-title">Episodes</span>
          {snapshot.recent_episodes.map((e, i) => (
            <div key={i} className="debug-item">
              {e.summary} (str {e.strength.toFixed(2)}, x{e.recall_count})
            </div>
          ))}
        </div>
      )}

      {snapshot.pending_events.length > 0 && (
        <div className="debug-section">
          <span className="debug-title">Pending</span>
          {snapshot.pending_events.map((p, i) => (
            <div key={i} className="debug-item">
              [{p.status}] {p.title} {p.remind_date ? `(${p.remind_date.split('T')[0]})` : ''}
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
    </div>
  );
}
