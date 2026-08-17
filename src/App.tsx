import { useState, useEffect, useCallback, useRef } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow, currentMonitor } from "@tauri-apps/api/window";
import { LogicalPosition } from "@tauri-apps/api/dpi";
import { SpineCanvas } from "./SpineCanvas";
import { SettingsPanel } from "./SettingsPanel";
import { ContextMenu } from "./ContextMenu";
import { AnimationFSM, BehaviorState } from "./animation/fsm";
import { pickNextBehavior } from "./animation/microBehavior";
import { type PetPosition } from "./animation/physics";
import { createGravity, stepGravity, type GravityState } from "./animation/gravity";
import { getCircadianState, deepNightMessages, TimeOfDay } from "./animation/circadian";
import { typewriterPacing, inferPacingMood } from "./animation/bubblePacing";
import { shouldAutoSleep } from "./animation/sleepLogic";
import { sound, INTIMATE_THRESHOLD } from "./audio/soundManager";
import { PetBubble } from "./components/PetBubble";
import { ThinkingOrb } from "thinking-orbs";
import type { BubbleEmotion, GlyphKind } from "./animation/bubbleVariants";
import { pickGreeting } from "./greetings";

interface EmotionData {
  mood: number;
  mood_label: string;
  physical_energy: number;
  social_battery: number;
  stress: number;
  loneliness: number;
  rest_need: number;
}

interface ProactiveAction {
  event_id: string | null;
  action_type: string;
  message_hint: string;
}

// F2 修改提案确认卡（plan §3.6：回复即提案，Rust 在用户确认后才写文件）。
interface EditProposalInfo {
  id: string;
  path: string;
  diff_preview: string;
  search_len: number;
}

interface EditApplyOutcome {
  status: string; // saved | declined | failed | undone
  message: string;
  path: string | null;
}

// 首次见面访谈问题（顺序即提问顺序）。答案存入 app_config，注入 system prompt 的 [Persona]。
const ONBOARD_QUESTIONS = [
  { key: "user_nickname", ask: "初次见面！我该怎么称呼你呀？" },
  { key: "personality_style", ask: "你希望我是什么性格呢？（温柔治愈 / 活泼调皮 / 知性冷静 / 毒舌……）" },
  { key: "relationship_style", ask: "我们是什么关系好呢？（伙伴 / 恋人 / 妹妹 / 助手……）" },
  { key: "pet_name", ask: "最后，给我起个名字吧？（想让我自己起，就回“你来想”）" },
] as const;

// ── 思考球（thinking-orbs）──
// 样式 state 九选一：working 粒子轨道 / searching 扫描子午线 / solving 色带还原 /
//   listening 波形 / connecting 星座连线 / weaving 三股辫 / composing 波纹彩带 /
//   breathing 呼吸环（Thinking…，LLM 等待默认）/ shaping 圆→三角→方
// 尺寸 size 两种预设（非缩放，独立调参）：64 头像级 / 20 行内级
const THINKING_ORB_STATE = "working";
const THINKING_ORB_SIZE = 20;

// Map mood label to bubble CSS class for emotion-driven styling (Design doc 6.3)
function bubbleClassForMood(label: string): BubbleEmotion {
  if (label === "开心") return "bubble-happy";
  if (label === "调皮") return "bubble-playful";
  if (label === "难过") return "bubble-sad";
  if (label === "担心") return "bubble-worried";
  if (label === "疲惫") return "bubble-sad";
  if (label === "害羞") return "bubble-shy";
  return "bubble-calm";
}

export default function App() {
// 方案 B: window dimensions match tauri.conf.json. petPos = window top-left.
const WINDOW_W = 400;
const WINDOW_H = 760;
// Sleeping: in DeepNight (2-6) the pet drifts off after this long with no
// interaction. Any interaction refreshes it -> natural awake cooldown (#10).
const SLEEP_AFTER_IDLE_MS = 10 * 60 * 1000;

  const [bubbleText, setBubbleText] = useState("");
  const [bubbleVisible, setBubbleVisible] = useState(false);
  // bubbleId = identity for ONE speech act (design review v3 #2). A new bubbleId
  // passed as <Bubble key={bubbleId}> makes AnimatePresence exit the old bubble
  // and enter the new one. Streaming token growth does NOT change bubbleId, so
  // the enter animation never replays mid-stream. Driven by beginBubble() below
  // (single identity generator), not ad-hoc setBubbleId(k=>k+1) at call sites.
  const [bubbleId, setBubbleId] = useState(0);
  // Glyph sub-kind (only meaningful when bubbleStyle === "bubble-glyph"). The dot
  // is one variant, not a universal prefix — see glyphText() in bubbleVariants.
  const [glyphKind, setGlyphKind] = useState<GlyphKind | undefined>(undefined);
  const [bubbleStyle, setBubbleStyle] = useState<BubbleEmotion>("bubble-calm");
  const [bubblePos, setBubblePos] = useState("");
  const [inputVisible, setInputVisible] = useState(false);
  const [inputText, setInputText] = useState("");
  // F2 edit proposal confirm card (plan §3.6) and its apply/undo outcome.
  const [editProposal, setEditProposal] = useState<EditProposalInfo | null>(null);
  const [editOutcome, setEditOutcome] = useState<EditApplyOutcome | null>(null);
  const [isThinking, setIsThinking] = useState(false);
  const [moodLabel, setMoodLabel] = useState("平静");
  const [showSettings, setShowSettings] = useState(false);
 const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);
const [awayMode, setAwayMode] = useState(false);
  const [onboarding, setOnboarding] = useState<{
    active: boolean;
    step: number;
    answers: Record<string, string>;
  } | null>(null);
  // Ref mirror so bubble listeners (captured once) can read the latest value
  // and suppress welcome bubbles during the interview.
  const onboardingActiveRef = useRef(false);
const bubbleTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Identity generator for bubble "speech acts" (design review v3 #2). Each new
  // bubble (whether from showBubble or streaming firstChunk) calls beginBubble()
  // → new id → AnimatePresence exits old + enters new. Streaming tokens do NOT
  // call this, so they don't replay enter. Centralizing here prevents a new
  // entry point from forgetting to mint a new id.
  const bubbleIdRef = useRef(0);
  const beginBubble = useCallback(() => {
    bubbleIdRef.current += 1;
    return bubbleIdRef.current;
  }, []);
  const [behavior, setBehavior] = useState<BehaviorState>(BehaviorState.Idle);
  const fsmRef = useRef<AnimationFSM | null>(null);
  const petRef = useRef<HTMLDivElement>(null);
  const pokeCountRef = useRef(0);
 const pokeResetTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingPokeRef = useRef<ReturnType<typeof setTimeout> | null>(null);
 const lastPetTimeRef = useRef(0);
  const lastInteractionRef = useRef(Date.now()); // drives DeepNight auto-sleep
  const closenessRef = useRef(0);
  const moodRef = useRef(0.5);
  const energyRef = useRef(0.7);
  // B2 (P12.1): free-fall + taskbar bounce. gravityRef drives the rAF loop
  // after a release mid-air; floorYRef/winSizeRef are refreshed on every
  // release so monitor changes are picked up.
  const gravityRef = useRef<GravityState>(createGravity());
  const floorYRef = useRef(0);
  // Logical-px monitor bounds (currentMonitor at init). The "walls" that keep
  // the pet's BODY fully on-screen via clampModelToScreen — without them the
  // OS drag could park her half-off-screen (上半身出屏 → 头不可见、无法再抓).
  const screenSizeRef = useRef({ w: 1920, h: 1080 });
  const winSizeRef = useRef({ w: 0, h: 0 });
  // B2 (P12.1): free-fall stops after a THIRD of the drop distance (user
  // preference 08-01) — she floats to a hover instead of hitting the floor.
  // Set when a fall starts, cleared when she stops (hover or land).
  const fallLimitBottomRef = useRef(0);
  // Native OS dragging (startDragging) swallows all webview mouse events, so
  // drag-end is detected via window "moved" events + a quiet-period check in
  // the rAF loop: after the window stops moving for ~300ms, if she was left
  // mid-air, free-fall starts.
  const wasDraggedRef = useRef(false);
  const lastMovedRef = useRef(0);
  // Timestamp (performance.now) of the last received global-cursor event.
  // The fall only arms when this is FRESH (<1.5s): if the event pipeline ever
  // goes silent, the lbutton state is unverifiable and arming on a guess can
  // fight the OS drag (shake) or teleport from a stale petPos (回原位 bug).
  const lastCursorEventAtRef = useRef(0);
  // One-shot guard so the async (IPC-calibrated) fall-arm can't double-fire
  // across rAF frames while the outerPosition roundtrip is in flight.
  const armPendingRef = useRef(false);
  // Telemetry: petPos-vs-real-window drift seen at the last fall-arm (>2px
  // means the onMoved sync missed drag moves — the stale-position teleport
  // the user reported on 2026-08-15). Surfaced via __dragDiag + console.
  const lastArmDriftRef = useRef<{
    drift: number; stale: PetPosition | null; real: PetPosition; at: number;
  } | null>(null);
  // Window position at the LAST onMoved while the left button was HELD — the
  // position the user actually released at. Windows snaps a window whose top
  // edge is above the screen (y<0) back to y=0 when a drag ends, so the
  // post-release position ≠ the release point; this ref lets us undo that
  // snap ("上移后仍回原位" root cause, 2026-08-15) and park the pet high.
  const lastHeldPosRef = useRef<PetPosition | null>(null);
  // Cursor-based release-point capture (续³⁹·2, more robust than onMoved-based:
  // the top-clamp snap's move event can arrive before the lbutton=false cursor
  // event and overwrite lastHeldPosRef with the snapped y=0, killing the undo).
  // The OS drag keeps the cursor↔window offset fixed at the threshold-crossing
  // point, so releasePos = lastHeldCursor − grabOffset, immune to that race.
  const grabOffsetRef = useRef<{ x: number; y: number } | null>(null); // physical px
  const lastHeldCursorRef = useRef<{ x: number; y: number } | null>(null); // physical px
  // Telemetry: the last OS top-clamp snap we undid (__dragDiag.lastSnapRestore).
  const lastSnapRestoreRef = useRef<{
    held: PetPosition; snapped: PetPosition; at: number;
  } | null>(null);
  // Manual-drag in progress: threshold crossed, until the left button
  // releases. The OS native drag (startDragging) is NOT used anymore — it
  // lets the window slide past the screen boundary freely (head off-screen =
  // 穿模) and cannot be clamped. Instead the global-cursor pipeline drives
  // the window to a clamped target (see the lbutton branch below), so the
  // pet's body can never leave the monitor: screen edges act as walls.
  const draggingRef = useRef(false);
  // Logical release point where the user let go. Cursor-based capture
  // (lastHeldCursor − grabOffset) is authoritative — it cannot be clobbered by
  // the top-clamp snap's move event — with the onMoved-based capture as
  // fallback. Shared by the onMoved fast path and the arm fallback.
  const releasePosRef = (): PetPosition | null => {
    if (lastHeldCursorRef.current && grabOffsetRef.current) {
      const f = scaleFactorRef.current || 1;
      return {
        x: (lastHeldCursorRef.current.x - grabOffsetRef.current.x) / f,
        y: (lastHeldCursorRef.current.y - grabOffsetRef.current.y) / f,
      };
    }
    return lastHeldPosRef.current;
  };
  // Keep the pet's BODY on-screen: screen edges act as walls, and they hug the
  // VISUAL body (visualBoundsRef, no padding) so the head can touch the screen
  // top and the feet the taskbar top. The Tauri window (400×760) is taller and
  // wider than the model, so its edges may sit off-screen — up to the point
  // where the body itself would leave the monitor (body fully visible, head/
  // feet/arms all grabbable). The TOP wall lets the window's top edge (bubble
  // zone + canvas slack above her head) go off-screen so her HEAD reaches the
  // screen top; the manual drag pipeline (draggingRef — no OS startDragging,
  // hence no system move loop) makes this safe, and if a Windows release-time
  // top-clamp ever does fire, the snap-undo paths restore immediately. The
  // BOTTOM wall is the work-area floor (taskbar top), so her feet rest on the
  // taskbar instead of sliding behind it.
  // Without all of this she can be parked half-off-screen where the head is
  // unreachable and the pet can't be dragged (用户: "上半身出去了,没法拖动,穿模").
  // Returns the input unchanged when geometry isn't ready yet.
  const clampModelToScreen = (pos: { x: number; y: number }): { x: number; y: number } => {
    const canvas = canvasRectRef.current;
    // Prefer the visual rect; the padded click-through rect (fallback before
    // SpineCanvas reports) keeps an air gap at every wall.
    const mb = visualBoundsRef.current ?? modelBoundsRef.current;
    if (!canvas || !mb) return pos; // geometry not reported yet — no clamp
    const screen = screenSizeRef.current;
    // Body rect inside the window (viewport coords, logical px).
    const mLeft = canvas.left + mb.x;
    const mTop = canvas.top + mb.y;
    const mRight = mLeft + mb.width;
    const mBottom = mTop + mb.height;
    // Window top-left (logical) that keeps the body within the monitor.
    const minX = -mLeft;
    const maxX = screen.w - mRight;
    const minY = -mTop; // top wall: HEAD at screen top (window top may be off-screen)
    const floor = floorYRef.current > 0 ? floorYRef.current : screen.h;
    const maxY = floor - mBottom; // bottom wall: FEET on the taskbar top
    return {
      x: maxX >= minX ? Math.min(Math.max(pos.x, minX), maxX) : pos.x,
      y: maxY >= minY ? Math.min(Math.max(pos.y, minY), maxY) : pos.y,
    };
  };
  // ALL programmatic window moves go through here. It syncs windowOriginRef
  // (physical px, used by the global-cursor click-through test) to the target
  // BEFORE the async onMoved echo arrives. Without this, a move at drag end
  // (snap-undo / visible-clamp / wall clamp) leaves the origin stale, so the
  // inside/outside test is shifted: the body is treated as click-through
  // (no grab cursor on the pet) while blank canvas beside her is treated as
  // interactive (grab cursor outside the pet). The origin then only heals
  // once some later onMoved fires — the "recovers after a while" symptom.
  const moveWindowTo = useCallback((x: number, y: number) => {
    const f = scaleFactorRef.current || 1;
    windowOriginRef.current = { x: Math.round(x * f), y: Math.round(y * f) };
    return getCurrentWindow().setPosition(new LogicalPosition(x, y));
  }, []);
  // Physical left-button state from the backend's global-cursor events (OS
  // truth via GetAsyncKeyState). Native drags swallow webview mouseup, so the
  // page alone can't tell "user paused mid-drag" from "user released" — this
  // ref can. The fall physics freezes while it's true (the user is holding
  // her), which stops the rAF setPosition loop from fighting the OS drag loop
  // (the "violent shake while dragging" bug).
  const lbuttonRef = useRef(false);
  // The physics loop reads/writes position via refs (not state) so moved events
  // and setPosition calls never re-create the loop mid-motion — the previous
  // state-driven deps re-created it every frame, resetting `lastTime` and
  // making dt jitter (visible as stutter while falling/bouncing).
  const petPosRef = useRef<PetPosition | null>(null);
  const isBeingDraggedRef = useRef(false);
  const lastOriginRefreshRef = useRef(0);
 const [isBeingDragged, setIsBeingDragged] = useState(false);
 const [soundMuted, setSoundMuted] = useState(false);
 const circadianRef = useRef(getCircadianState());
 const pointerRef = useRef({ x: window.innerWidth / 2, y: window.innerHeight / 2 });
 const thinkStartRef = useRef(0);
 // Streaming-typewriter state: chunks arrive faster than the eye can follow
 // and React batches same-tick setState, so buffer in refs and reveal on an
 // interval for a smooth typewriter effect (#10).
 const streamBufRef = useRef("");
 const shownLenRef = useRef(0);
 const streamEndedRef = useRef(false);
 const typewriterRef = useRef<number | null>(null);
 // Ref mirrors of bubble/thinking visibility, so the long-lived emotionTimer
 // setInterval reads fresh values instead of a stale mount-time closure
 // (used by the idle-sigh guard below).
  const bubbleVisibleRef = useRef(false);
  const isThinkingRef = useRef(false);
  // Timestamp of the last bubble shown via showBubble (Date.now). Lets
  // low-priority bubble sources (Soul startup thought) wait for a quiet
  // moment instead of stacking on top of a greeting (2026-08-15: three
  // bubbles in a row after relaunch).
  const lastBubbleShownAtRef = useRef(0);
 // Idle-sigh cooldown (2026-08-14): a "呼…" at most every 5 minutes — it was
 // 8% per 5s tick with no cooldown, i.e. potentially several sighs a minute.
 const lastSighRef = useRef(0);
 useEffect(() => { bubbleVisibleRef.current = bubbleVisible; }, [bubbleVisible]);
 useEffect(() => { isThinkingRef.current = isThinking; }, [isThinking]);
 // Foley: preload all buffers + startup greeting on mount (greeting defers to
 // the first pointerdown if AudioContext is suspended — autoplay policy). Mute
 // mirrors the singleton so the menu toggle and play() agree (#6/#11).
 useEffect(() => {
   sound.preload();
   sound.greet();
 }, []);
 useEffect(() => { sound.setMuted(soundMuted); }, [soundMuted]);
  // Click-through state (ADR Phase 2).
  const ignoreRef = useRef(false);
  const windowOriginRef = useRef<{ x: number; y: number } | null>(null); // physical px
  const scaleFactorRef = useRef<number | null>(null);
  const modelBoundsRef = useRef<{ x: number; y: number; width: number; height: number } | null>(null);
  const modelHitBoundsRef = useRef<{ x: number; y: number; width: number; height: number } | null>(null);
  const canvasRectRef = useRef<{ left: number; top: number } | null>(null);
  // Click-through diagnostics (Architecture #11 observability). Mirrored to
  // the backend (set_clickthrough_diag, throttled ~200ms) so the Debug Panel's
  // separate OS window can read them via get_clickthrough_diag. DebugStandalone
  // can't share React state with App — the backend Mutex is the bridge (续¹⁸
  // Face State "backend-relay" pattern).
  const clickthroughDiagRef = useRef<{
    has_origin: boolean; has_scale: boolean; has_canvas: boolean; has_bounds: boolean;
    sx: number; sy: number;
    left: number; top: number; right: number; bottom: number;
    inside: boolean; ignore: boolean;
    bounds_x: number; bounds_y: number; bounds_w: number; bounds_h: number;
    origin_x: number; origin_y: number; scale: number;
  } | null>(null);
  const lastDiagPushRef = useRef(0); // performance.now() of last backend push (throttle)
  // Click-through boundary visualization (AIRI-style). A colored border drawn
  // around the model's hit rect that appears only when the cursor is near the
  // border line (±BAND px, inside or outside), and fades out 250ms after the
  // cursor leaves that band. Purely visual — pointer-events:none so it never
  // interferes with click-through or interaction. The overlay div is positioned
  // in canvas-local coords (same as modelBoundsRef), so it lives inside
  // pet-char-wrapper alongside the canvas.
  const boundsOverlayRef = useRef<HTMLDivElement | null>(null);
  const boundsShownRef = useRef(false); // current shown state (for edge-trigger)
  const boundsHideTimerRef = useRef<number | null>(null); // 250ms debounce hide
  // Trigger cooldown (user 2026-08-15): after the border SHOWS once, the cursor
  // re-approaching the band within this window will NOT re-trigger it. Only an
  // approach after the window expires shows it again, and so on. Timed from
  // the show moment (not the hide moment).
  const boundsCooldownUntilRef = useRef(0); // performance.now() timestamp

  if (!fsmRef.current) {
    fsmRef.current = new AnimationFSM();
    fsmRef.current.onStateChange((s) => setBehavior(s));
  }

 const showBubble = useCallback((text: string, duration = 8000, style: BubbleEmotion = "bubble-calm", pos = "", glyph?: GlyphKind) => {
   // Duty sequence (design review v3 #3): cancel any prior timer FIRST so a
   // stale timer from bubble A can't fire and hide bubble B while B is still
   // showing. Then mint a new identity, set state, start the new timer.
   if (bubbleTimerRef.current) clearTimeout(bubbleTimerRef.current);
   lastBubbleShownAtRef.current = Date.now();
   const id = beginBubble();
   setBubbleText(text);
   setBubbleStyle(style);
   setBubblePos(pos);
   setGlyphKind(glyph);
   setBubbleId(id);
   setBubbleVisible(true);
  bubbleTimerRef.current = setTimeout(() => setBubbleVisible(false), duration);
}, [beginBubble]);

  // 启动首次见面访谈：问第一题 + 显示输入框。访谈期间屏蔽其它气泡，避免 welcome 覆盖第一题。
  const startOnboarding = useCallback(() => {
    onboardingActiveRef.current = true;
    setOnboarding({ active: true, step: 0, answers: {} });
    showBubble(ONBOARD_QUESTIONS[0].ask, 120000, "bubble-calm");
    setInputVisible(true);
  }, [showBubble]);

  // Receive model bounds (canvas-local CSS px) from SpineCanvas for click-through.
  const handleModelBounds = useCallback((b: { x: number; y: number; width: number; height: number }) => {
    modelBoundsRef.current = b;
    // Capture the CANVAS rect (viewport-relative CSS px) — not the wrapper's.
    // The wrapper is display:flex; align-items:flex-end, so its rect differs
    // from the canvas inside it (canvas sits at the wrapper's bottom-center).
    // Using the wrapper rect here would offset both the click-through rect and
    // the boundary overlay (the "bottom not framed" bug). Query the actual
    // <canvas> element; there's exactly one inside the wrapper.
    const el = petRef.current;
    if (el) {
      const canvas = el.querySelector("canvas");
      if (canvas) {
        const r = canvas.getBoundingClientRect();
        canvasRectRef.current = { left: r.left, top: r.top };
      } else {
        // Fallback: wrapper rect (safe default keeps window interactive).
        const r = el.getBoundingClientRect();
        canvasRectRef.current = { left: r.left, top: r.top };
      }
    }
    // Diagnostic: confirms the canvas reported bounds (if this never logs,
    // modelBoundsRef stays null → the listener's safe default keeps the window
    // fully interactive forever = "blank area never click-through"). Covers
    // the SpineCanvas getBounds(true) throw case.
    console.log("[clickthrough] modelBounds reported", b);
  }, []);

  // Receive the tight model bounds (10% inset) from SpineCanvas; stored for
  // click hit testing (kept separate from the loose gaze/through rect).
  const handleModelHitBounds = useCallback((b: { x: number; y: number; width: number; height: number }) => {
    modelHitBoundsRef.current = b;
  }, []);

  // Visual body rect (NO padding), canvas-local CSS px — drives the drag
  // screen walls. The padded click-through rect (modelBoundsRef) would keep an
  // air gap: PAD+TOP_BIAS above the head and PAD below the feet, so the head
  // could never touch the screen top nor the feet the taskbar (用户 2026-08-16:
  // "头顶/脚底像有空气墙").
  const visualBoundsRef = useRef<{ x: number; y: number; width: number; height: number } | null>(null);
  const handleVisualBounds = useCallback((b: { x: number; y: number; width: number; height: number }) => {
    visualBoundsRef.current = b;
  }, []);

  // PetBubble reports its viewport rect here (CSS px) so the global-cursor
  // listener can treat the bubble region as non-click-through. Under OS-level
  // ignore_cursor_events, CSS pointer-events can't make the bubble scrollable;
  // the window must stop ignoring the cursor over the bubble. Null when hidden.
  const bubbleBoundsRef = useRef<{ left: number; top: number; width: number; height: number } | null>(null);
  const handleBubbleBounds = useCallback((rect: { left: number; top: number; width: number; height: number } | null) => {
    bubbleBoundsRef.current = rect;
  }, []);

  // Click-through: toggle whether transparent regions forward clicks to the desktop.
  // Safe default: never flip to ignore unless we have all the geometry we need.
  const applyIgnore = useCallback((desired: boolean) => {
    if (ignoreRef.current === desired) return;
    ignoreRef.current = desired;
    getCurrentWindow()
      .setIgnoreCursorEvents(desired)
      .catch((e) => console.warn("[clickthrough] setIgnore failed", e));
  }, []);

  // FSM tick timer: drives microbehaviors independently of LLM
  useEffect(() => {
    const timer = setInterval(() => {
      const fsm = fsmRef.current;
      if (!fsm || awayMode) return;
      // DeepNight (2-6) auto-sleep: left alone long enough + nothing happening ->
      // drift off. markInteraction() refreshes lastInteractionRef on any poke/pet/
      // drag/chat, so this won't re-fire until SLEEP_AFTER_IDLE_MS elapses again.
      // Condition extracted to shouldAutoSleep() so it is unit-testable.
      if (
        shouldAutoSleep({
          period: circadianRef.current.period,
          state: fsm.state,
          isThinking,
          isTalking: behavior === BehaviorState.Talking,
          idleMs: Date.now() - lastInteractionRef.current,
          thresholdMs: SLEEP_AFTER_IDLE_MS,
        })
      ) {
        fsm.forceState(BehaviorState.Sleeping);
        sound.sleep(); // drifting-off cue (plays once — guard above gates entry)
        return;
      }
      if (isThinking || behavior === BehaviorState.Talking) return;
      fsm.tick(moodRef.current, energyRef.current, closenessRef.current, circadianRef.current.sleepiness, Date.now(), pickNextBehavior);
    }, 2500);
    return () => clearInterval(timer);
 }, [isThinking, behavior, awayMode]);

 // Attention tracking: mouse proximity to pet (Design doc 6.6)
  useEffect(() => {
   const onMouseMove = (e: MouseEvent) => {
     const el = petRef.current;
     if (!el || awayMode) return;
      pointerRef.current = { x: e.clientX, y: e.clientY };
    };
    window.addEventListener("mousemove", onMouseMove);
    return () => window.removeEventListener("mousemove", onMouseMove);
  }, [awayMode]);

  // Update emotion + closeness refs for FSM
  useEffect(() => {
    const emoTimer = setInterval(async () => {
      try {
        const emo = await invoke<EmotionData>("get_emotion_state");
        moodRef.current = emo.mood;
        energyRef.current = emo.physical_energy;
      } catch { /* ignore */ }
    }, 10000);
    return () => clearInterval(emoTimer);
  }, []);

  useEffect(() => {
    const snapTimer = setInterval(async () => {
      try {
        const snap = await invoke<Record<string, number>>("get_debug_snapshot");
        closenessRef.current = snap.closeness || 0;
      } catch { /* ignore */ }
    }, 15000);
    return () => clearInterval(snapTimer);
  }, []);

  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    // StrictMode (dev) mounts effects twice (mount -> cleanup -> remount). Since
    // listen() is async, the first mount's listener can resolve *after* cleanup
    // ran — leaking a duplicate listener that fires on every event. `cancelled`
    // lets a late-resolving listener unlisten itself instead of leaking.
    let cancelled = false;

   listen<string>("bubble-show", (event) => {
      // 访谈进行中：只允许访谈气泡，忽略后端 welcome 等其它气泡以免覆盖当前问题。
      if (onboardingActiveRef.current) return;
      showBubble(event.payload, 8000, "bubble-calm");
   }).then((un) => { if (!cancelled) unlisteners.push(un); else un(); });

    listen("bubble-hide", () => {
      setBubbleVisible(false);
    }).then((un) => { if (!cancelled) unlisteners.push(un); else un(); });

    listen<EmotionData>("emotion-update", (event) => {
      setMoodLabel(event.payload.mood_label);
    }).then((un) => { if (!cancelled) unlisteners.push(un); else un(); });

    listen<{ title: string; event_id: string }>("proactive-prompt", (event) => {
      // Backend emitted a due pending event; route through the memory-grounded
      // generator instead of a canned "怎么样啦？" string.
      invoke<string | null>("proactive_bubble")
        .then((reply) => {
          if (reply) showBubble(reply, 15000, "bubble-calm");
        })
        .catch((e) => console.warn("[proactive-prompt] proactive_bubble failed", e));
      void event;
    }).then((un) => { if (!cancelled) unlisteners.push(un); else un(); });

    // User returned after >5min away (presence LongAway -> Active). Generate a
    // memory-grounded welcome via the LLM (falls back to a rule line backend-side).
    // Suppressed during the onboarding interview and while the pet is resting.
    listen<{ away_secs: number }>("welcome-back", (event) => {
      if (onboardingActiveRef.current) return;
      if (awayMode) return;
      invoke<string | null>("welcome_back_bubble", { awaySecs: event.payload.away_secs })
        .then((reply) => {
          if (reply) showBubble(reply, 10000, bubbleClassForMood(moodLabel));
        })
        .catch((e) => console.warn("[welcome-back] welcome_back_bubble failed", e));
    }).then((un) => { if (!cancelled) unlisteners.push(un); else un(); });

    // Loneliness-driven nudge: homeostasis let loneliness climb while the user
    // was idle at the desk, and the backend (gated by closeness + presence +
    // 30-min cooldown) decided she should reach out. Voice it via the
    // memory-grounded LLM path (falls back to a rule line backend-side).
    // Suppressed during onboarding / away, like welcome-back.
    listen<{ loneliness: number }>("lonely-nudge", () => {
      if (onboardingActiveRef.current) return;
      if (awayMode) return;
      // She's asleep — don't wake her to say "想你了". Mirrors the "go to bed"
      // nudge guard: a sleepy 璃 stays sleepy (Architecture #12 silence).
      if (fsmRef.current?.state === BehaviorState.Sleeping) return;
      invoke<string | null>("lonely_bubble")
        .then((reply) => {
          if (reply) showBubble(reply, 10000, bubbleClassForMood(moodLabel));
        })
        .catch((e) => console.warn("[lonely-nudge] lonely_bubble failed", e));
    }).then((un) => { if (!cancelled) unlisteners.push(un); else un(); });

    // Ritual greeting (早安 first iteration): the backend fires this once per
    // day on the first meeting during Morning/Afternoon. Mirrors the
    // lonely-nudge guard set (onboarding / away / sleeping). #12: a sleeping
    // 璃 isn't woken to say 早安.
    listen<{ kind: string }>("ritual-bubble", (event) => {
      if (onboardingActiveRef.current) return;
      if (awayMode) return;
      if (fsmRef.current?.state === BehaviorState.Sleeping) return;
      invoke<string | null>("ritual_bubble", { kind: event.payload.kind })
        .then((reply) => {
          if (reply) showBubble(reply, 10000, bubbleClassForMood(moodLabel));
        })
        .catch((e) => console.warn("[ritual-bubble] ritual_bubble failed", e));
    }).then((un) => { if (!cancelled) unlisteners.push(un); else un(); });

    listen<{ status: string; elapsed_secs: number }>("app-status", (event) => {
      if (event.payload.status === "resumed") {
        // 与其它问候监听器同一套守卫（onboarding / away / sleeping）：重启
        // 问候不得覆盖访谈第一问，也不打扰睡着的璃（单问候原则 2026-08-15/17）。
        if (onboardingActiveRef.current) return;
        if (awayMode) return;
        if (fsmRef.current?.state === BehaviorState.Sleeping) return;
        // Diversified local greeting pool (zero LLM, cost control — replaces
        // the old single hardcoded "我睡了N个小时" template). Bucketed by
        // away duration + time-of-day flavor, no immediate repeats.
        const line = pickGreeting({
          awayHours: event.payload.elapsed_secs / 3600,
          hourOfDay: new Date().getHours(),
        });
        showBubble(line, 8000, "bubble-calm");
      }
    }).then((un) => { if (!cancelled) unlisteners.push(un); else un(); });

    const emotionTimer = setInterval(async () => {
      try {
        const emo = await invoke<EmotionData>("get_emotion_state");
        setMoodLabel(emo.mood_label);
        // Idle sigh (architecture #12 silence-is-expression, #10 liveliness):
        // when she's worn down and not busy, occasionally let out a wordless
        // "呼…" glyph bubble. Guards keep it from interrupting an active
        // conversation / interview / away mode. Uses the just-fetched label
        // (fresh) and ref mirrors (fresh), not the stale closure state.
        if (
          (emo.mood_label === "疲惫" || emo.mood_label === "难过") &&
          !bubbleVisibleRef.current &&
          !isThinkingRef.current &&
          !onboardingActiveRef.current &&
          !awayMode &&
          Date.now() - lastSighRef.current > 5 * 60 * 1000 &&
          Math.random() < 0.03
        ) {
          lastSighRef.current = Date.now();
          showBubble("呼…", 2500, "bubble-glyph", "", "sigh");
        }
      } catch { /* ignore */ }
    }, 5000);

    invoke<EmotionData>("get_emotion_state")
      .then((emo) => {
        setMoodLabel(emo.mood_label);
      })
      .catch(() => {});

    // 重启问候统一走后端协调的 app-status / ritual-bubble（首 tick ~5s 到达，
    // 单问候原则 2026-08-15/17）。曾经的后端 2s 硬编码欢迎已删除（续⁴¹·5），
    // 此处的 FIX-J 2s 兜底随之失去触发方——留着会让每次启动都比后端问候先
    // 抢发一条无协调的随机问候（"重启三连"的元凶之一），故一并移除。

    const proactiveTimer = setInterval(async () => {
      if (awayMode) return;
      try {
        // Cold-start interview check (bypasses closeness gate).
        const interview = await invoke<string | null>("check_cold_start");
        if (interview) {
          showBubble(interview, 15000, "bubble-calm");
          return;
        }

        const action = await invoke<ProactiveAction | null>("check_proactive");
        if (action) {
          // Gate passed (closeness/cooldown/focus): generate a memory-grounded
          // proactive bubble via the LLM. Never use canned greetings.
          const reply = await invoke<string | null>("proactive_bubble");
          if (reply) showBubble(reply, 12000, "bubble-calm");
        }
      } catch { /* ignore */ }
    }, 5 * 60 * 1000);

    return () => {
      cancelled = true;
      unlisteners.forEach((un) => un());
      clearInterval(emotionTimer);
      clearInterval(proactiveTimer);
      if (bubbleTimerRef.current) clearTimeout(bubbleTimerRef.current);
    };
  }, [showBubble, awayMode]);

 useEffect(() => {
   const onKey = (e: KeyboardEvent) => {
     // F12 is the default, but some laptops hijack it (e.g. sleep key), so also
     // accept Ctrl+Shift+D as a reliable alternate to toggle the Debug Panel.
     const k = e.key.toLowerCase();
     // F12 / Ctrl+Shift+D open the Debug Panel as a separate OS window
     // (open_debug_window) so it never covers the pet. Idempotent — pressing
     // again while open just focuses the existing window.
     if (e.key === "F12" || (e.ctrlKey && e.shiftKey && k === "d")) {
       e.preventDefault();
       invoke("open_debug_window");
     }
   };
   window.addEventListener("keydown", onKey);
   return () => window.removeEventListener("keydown", onKey);
 }, []);

  // FIX-0 guard: detect browser mode (no Tauri backend) and warn once.
  useEffect(() => {
    if (!(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__) {
      const t = setTimeout(() => {
        showBubble("请在桌面模式下运行 (npm run tauri dev)", 10000, "bubble-worried");
      }, 1500);
      return () => clearTimeout(t);
    }
  }, [showBubble]);

 // P12: Initialize nest position on first mount
 useEffect(() => {
    // 方案 B: petPos = window top-left in screen coordinates. Place the window at
    // the bottom-right corner above the taskbar on first launch.
    (async () => {
    const win = getCurrentWindow();
    let screenW = 1920;
    let screenH = 1080;
    let factor = 1;
    try {
      // Prefer the Tauri current-monitor size for accuracy across DPI.
       factor = await win.scaleFactor();
       let monitor = await currentMonitor();
       if (!monitor) {
         const { primaryMonitor } = await import("@tauri-apps/api/window");
         monitor = await primaryMonitor();
       }
       if (monitor) {
         screenW = monitor.size.width / factor;
         screenH = monitor.size.height / factor;
       }
       screenSizeRef.current = { w: screenW, h: screenH };
     } catch { /* keep fallback 1920x1080 */ }
      const x = Math.max(20, Math.round(screenW - WINDOW_W - 20));
      const y = Math.max(20, Math.round(screenH - WINDOW_H - 48)); // above taskbar
      const initPos = { x, y };
      try { await win.setPosition(new LogicalPosition(x, y)); } catch (e) { console.warn("[Init] setPosition failed", e); }
      console.log("[Init] window placed at", x, y, "screen:", screenW, screenH);
     petPosRef.current = initPos;
     // B2 (P12.1): cache window size + work-area bottom (taskbar top) once —
     // the fall/land physics collide against this floor. Refreshed again on
     // each drag release via currentMonitor.
     try {
       const size = await win.outerSize();
       winSizeRef.current = { w: size.width / factor, h: size.height / factor };
       const mon = await currentMonitor();
       if (mon) {
         floorYRef.current = mon.workArea.position.y / factor + mon.workArea.size.height / factor;
       }
     } catch { /* keep defaults; drag release re-measures */ }
    })();
}, []);

  // 首次见面访谈：启动时查后端 needs_onboarding，未完成则开始访谈。
  useEffect(() => {
    (async () => {
      try {
        if (await invoke<boolean>("needs_onboarding")) startOnboarding();
      } catch (e) {
        console.warn("onboarding check", e);
      }
    })();
  }, [startOnboarding]);

  // Soul layer: on startup, trigger reflection if due (>20h) then surface
  // any pending internal thoughts as a bubble. Skipped during onboarding
  // (no conversations to reflect on yet). Runs once.
  useEffect(() => {
    (async () => {
      try {
        // Don't run if onboarding is active (user hasn't had real conversations yet).
        if (await invoke<boolean>("needs_onboarding")) return;

        // Trigger reflection if >20h since last (returns bool).
        const reflected = await invoke<boolean>("trigger_reflection_if_due");
        if (reflected) console.log("[Soul] reflection completed");

        // Surface pending thoughts (marks them surfaced, returns contents).
        const thoughts = await invoke<string[]>("get_pending_thoughts");
        if (thoughts.length > 0) {
          // Single-greeting coordination (2026-08-15): the thought is the
          // LOWEST-priority startup voice — 早安/welcome-back canned own the
          // first moment. Wait for a quiet window (no bubble visible, none in
          // the last 45s) with a few retries; if it never gets quiet, drop
          // (reflections regenerate; spam is worse than silence — Arch #12).
          setTimeout(() => {
            const tryShow = (attempt: number) => {
              if (onboardingActiveRef.current) return;
              const busy =
                bubbleVisibleRef.current ||
                Date.now() - lastBubbleShownAtRef.current < 45_000;
              if (busy) {
                if (attempt < 3) window.setTimeout(() => tryShow(attempt + 1), 30_000);
                return;
              }
              showBubble(thoughts[0], 12000, "bubble-calm");
            };
            tryShow(0);
          }, 6000);
        }
      } catch (e) {
        console.warn("[Soul] reflection/thought check failed", e);
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Click-through (ADR Phase 2): transparent regions forward clicks to the desktop.
  // Listens to the backend global-cursor event, compares against the model's screen
  // rect (window origin + canvas-local model bounds * scaleFactor), and toggles
  // setIgnoreCursorEvents. Forces capture (ignore=false) when input/settings/drag are active.
  useEffect(() => {
    const win = getCurrentWindow();
    // cancelled guards the async listen()/onMoved() promises against StrictMode
    // (dev) double-mount and dep-array rebuilds: the first mount's listen()
    // Promise can resolve AFTER cleanup ran, leaking a duplicate listener that
    // fires on every cursor event. Mirrors the bubble listener's pattern
    // (App.tsx ~302-306). Late-resolving unlisten self-cancels instead.
    let cancelled = false;
    const refreshOrigin = async () => {
      try {
        const p = await win.outerPosition();
        windowOriginRef.current = { x: p.x, y: p.y };
        const f = await win.scaleFactor();
        scaleFactorRef.current = f;
      } catch { /* leave nulls; safe default keeps window interactive */ }
    };
    refreshOrigin();
    // Click-through geometry diagnostics for CDP debugging (dev aid).
    (window as any).__ctDiag = () => ({
      origin: windowOriginRef.current,
      scale: scaleFactorRef.current,
    });
    // Pin the window's ignore state to a known value on mount. Tauri's default
    // is false, but applyIgnore's dedup (`if (ignoreRef.current === desired)
    // return`) means the first desired=false would skip the IPC entirely —
    // leaving the window state relying on the (undocumented) Tauri default.
    // An explicit set pins it to known-good and removes that implicit coupling.
    win.setIgnoreCursorEvents(false).catch(() => {});
    let unlistenMoved: UnlistenFn | undefined;
    // B2 (P12.1): native startDragging swallows webview mouse events entirely
    // (verified: no mouseup ever reaches the page), so drag-end is detected
    // via movement quiescence: onMoved keeps lastMovedRef fresh; the rAF loop
    // starts free-fall after ~300ms of stillness. onMoved also keeps petPos in
    // sync with the real window position (previously stale after every drag).
    win.onMoved(({ payload: pos }) => {
      const f = scaleFactorRef.current || 1;
      const logical = { x: pos.x / f, y: pos.y / f };
      // While free-falling the rAF loop owns the position (it already writes
      // the rounded value it setPosition'd), so skip the echo to avoid
      // fighting the loop; when grounded, keep petPosRef in sync. A held
      // left button ALSO syncs: a re-grab mid-fall freezes the fall (see
      // physics loop), and without this sync the resumed fall would start
      // from the stale pre-drag position and teleport her.
      if (gravityRef.current.grounded || lbuttonRef.current) {
        petPosRef.current = logical;
      }
      lastMovedRef.current = performance.now();
      if (isBeingDraggedRef.current) {
        wasDraggedRef.current = true;
        isBeingDraggedRef.current = false;
        setIsBeingDragged(false);
      }
      // 2026-08-15 (续³⁹·2): OS top-clamp snap-undo. Capture the release point
      // while the button is held; Windows clamps a window released with its
      // top edge off-screen (y<0) back to y=0 (measured: T=-504 → T=0 within
      // 2ms of release) — the "上移后仍回原位" the user still saw after the
      // fall was disabled. When a post-release move shows the window snapped
      // DOWN from an off-screen release point, undo it immediately so the pet
      // can actually be parked high (拖到哪停哪 includes 上面).
      if (lbuttonRef.current) {
        lastHeldPosRef.current = logical;
      }
      const held = releasePosRef();
      if (
        !lbuttonRef.current &&
        wasDraggedRef.current &&
        held &&
        held.y < -SNAP_GAP &&
        logical.y < SNAP_GAP &&
        logical.y > held.y + SNAP_GAP
      ) {
        const restore = clampModelToScreen(held);
        // Only act if it actually moves the window — otherwise this is the
        // wall-clamp false positive (cursor kept going past the wall while she
        // stayed pinned at the clamped target), not an OS snap. y<0 is a
        // NORMAL parked position now (top wall = head at screen top), so this
        // guard keeps the path quiet for ordinary top-edge releases.
        if (Math.abs(restore.x - logical.x) > 0.5 || Math.abs(restore.y - logical.y) > 0.5) {
          lastSnapRestoreRef.current = { held: restore, snapped: logical, at: Date.now() };
          console.warn("[drag] OS top-clamp snap undone", { held: restore, snapped: logical });
          petPosRef.current = restore;
          moveWindowTo(restore.x, restore.y).catch((err) =>
            console.warn("[drag] snap-undo setPosition failed", err),
          );
        }
      }
      // Throttle the click-through origin refresh: it does an async
      // outerPosition IPC per move, which floods during fast motion
      // (drag/fall/walk) and causes visible stutter.
      const now = performance.now();
      if (now - lastOriginRefreshRef.current > 100) {
        lastOriginRefreshRef.current = now;
        void refreshOrigin();
      }
    }).then((u) => { if (!cancelled) unlistenMoved = u; else u(); }).catch(() => {});

    let unlisten: UnlistenFn | undefined;
    // Watchdog: if the global-cursor polling thread stalls (rare but would
    // leave the window permanently transparent to mouse events — pet can't
    // be dragged nor right-clicked), force back to interactive after 10 s.
    let cursorWatchdog: ReturnType<typeof setTimeout> | undefined;
    const resetWatchdog = () => {
      if (cursorWatchdog) clearTimeout(cursorWatchdog);
      cursorWatchdog = setTimeout(() => {
        if (ignoreRef.current) {
          console.warn("[clickthrough] cursor polling silent for 10s — resetting to interactive");
          applyIgnore(false);
        }
      }, 10_000);
    };
    listen<{ x: number; y: number; lbutton: boolean }>("global-cursor", (e) => {
      resetWatchdog();
      // Pipeline liveness timestamp (see lastCursorEventAtRef) — refreshed
      // before anything else so every consumer below reads fresh state.
      lastCursorEventAtRef.current = performance.now();
      // OS-level button truth (see lbuttonRef declaration) — refresh it before
      // anything below so the physics loop always reads the freshest state.
      const prevLbutton = lbuttonRef.current;
      lbuttonRef.current = e.payload.lbutton === true;
      const { x: sx, y: sy } = e.payload; // physical screen px
      // Last cursor position while the button was held — release-point capture
      // (see grabOffsetRef); immune to the top-clamp snap's move race.
      if (lbuttonRef.current) {
        lastHeldCursorRef.current = { x: sx, y: sy };
        // Manual drag driver (see draggingRef): move the window to a clamped
        // target so the pet's body stays fully on-screen while dragging —
        // screen edges act as walls (drag her up and the head pins at the top
        // edge instead of 穿模-ing off-screen). The cursor pipeline keeps
        // emitting even while the window is click-through over transparent
        // areas (cursor above the pinned model), so the drag never stalls.
        if (draggingRef.current) {
          const off = grabOffsetRef.current;
          if (off) {
            const f = scaleFactorRef.current || 1;
            const target = clampModelToScreen({
              x: (sx - off.x) / f,
              y: (sy - off.y) / f,
            });
            moveWindowTo(target.x, target.y).catch(() => {});
          }
        }
      } else if (prevLbutton) {
        // Released: end the manual drag (movement gate off; isBeingDragged is
        // cleared by the first onMoved after the drag, and force-capture no
        // longer needs to hold once the button is up).
        draggingRef.current = false;
        // If she was parked with her top edge off-screen (the clamped top park
        // position is y<0), Windows snaps the top edge back to y=0 — undo it a
        // tick later so the clamp has landed (release fast path; the arm's
        // 300ms quiet would read as a bounce).
        const rel = releasePosRef();
        if (rel && rel.y < -SNAP_GAP) {
          setTimeout(() => {
            if (lbuttonRef.current || isBeingDraggedRef.current) return; // re-grabbed
            // Guards against false positives: wasDragged proves a real OS drag
            // just happened (grabOffset is fresh); snapped.y≈0 proves the
            // top-clamp actually pulled her to the top edge (startDragging can
            // fail without moving the window — petPosRef would stay put).
            const snapped = petPosRef.current; // synced by the snap's onMoved
            if (
              wasDraggedRef.current &&
              snapped &&
              rel.y < -SNAP_GAP &&
              snapped.y < SNAP_GAP &&
              snapped.y > rel.y + SNAP_GAP
            ) {
              const restore = clampModelToScreen(rel);
              // Only act if it actually moves the window — otherwise this is a
              // false positive: she was clamped at a wall while the cursor kept
              // going (release point beyond the wall), not an OS snap. The
              // restore target equals her current (clamped) position then.
              if (Math.abs(restore.x - snapped.x) > 0.5 || Math.abs(restore.y - snapped.y) > 0.5) {
                lastSnapRestoreRef.current = { held: restore, snapped, at: Date.now() };
                console.warn("[drag] OS top-clamp snap undone (release fast path)", { held: restore, snapped });
                petPosRef.current = restore;
                moveWindowTo(restore.x, restore.y).catch((err) =>
                  console.warn("[drag] snap-undo setPosition failed", err),
                );
              }
            }
          }, 80);
        }
      }
      const origin = windowOriginRef.current;
      const scale = scaleFactorRef.current;
      const canvas = canvasRectRef.current;
      const mb = modelBoundsRef.current;
      // Force-capture: never ignore when the user needs to interact with the whole window.
      const forceCapture = inputVisible || showSettings || isBeingDragged;
      // Geometry for the diagnostics snapshot (computed in every branch so the
      // Debug Panel sees what the listener sees, even when forceCapture / null
      // geometry short-circuit before the rect math).
      let left = 0, top = 0, right = 0, bottom = 0, inside = false;
      if (origin && scale && canvas && mb) {
        left = origin.x + (canvas.left + mb.x) * scale;
        top = origin.y + (canvas.top + mb.y) * scale;
        right = left + mb.width * scale;
        bottom = top + mb.height * scale;
        inside = sx >= left && sx <= right && sy >= top && sy <= bottom;
      }
      // Bubble region also counts as inside: the bubble sits above the model
      // (not in modelBounds), so without this the window stays click-through
      // over it and scrolling is impossible (CSS pointer-events is useless
      // under OS-level ignore_cursor_events). bubbleBoundsRef is set by
      // PetBubble (null when hidden), so no stale-state concern.
      const bb = bubbleBoundsRef.current;
      if (origin && scale && bb) {
        const bl = origin.x + bb.left * scale;
        const bt = origin.y + bb.top * scale;
        if (sx >= bl && sx <= bl + bb.width * scale && sy >= bt && sy <= bt + bb.height * scale) {
          inside = true;
        }
      }
      // Boundary visualization (AIRI-style). Show the colored border only when
      // the cursor is near the rect's outline — in the outer band (just outside
      // the rect) OR the inner band (just inside), within BAND px of the edge.
      // Cursor deep inside the rect, or far outside, keeps it hidden. 250ms
      // debounce on hide so edge jitter doesn't flicker. Geometry in canvas-
      // local CSS px (modelBoundsRef is already in that space) for the overlay
      // div, which is a sibling of the canvas inside pet-char-wrapper.
      if (mb && boundsOverlayRef.current) {
        const BAND = 12; // px (screen) band around the outline that triggers show
        const TRIGGER_COOLDOWN_MS = 5000; // re-approach must wait this long after a show
        const nearBorder =
          sx >= left - BAND && sx <= right + BAND &&
          sy >= top - BAND && sy <= bottom + BAND &&
          !(sx >= left + BAND && sx <= right - BAND &&
            sy >= top + BAND && sy <= bottom - BAND);
        if (nearBorder) {
          // (Re)position the overlay each show. position:fixed (viewport coords)
          // so the frame escapes body{overflow:hidden}. The model bounds rect
          // is canvas-local; translate to viewport via the canvas's own rect.
          //
          // The PAD-expanded rect can extend past the canvas (e.g. mb.y=-51,
          // mb.y+mb.h=624 > canvas 600). Since the Tauri window is only 760px
          // tall and the canvas sits in its lower portion, a rect bottom beyond
          // the canvas falls outside the window entirely and is invisible —
          // which is why the bottom edge was missing ("下边缺一条边"). Clamp the
          // frame to the canvas's viewport rect so all four edges stay on-
          // screen. The frame still visually encloses the model (the model
          // lives inside the canvas).
          const o = boundsOverlayRef.current;
          const wrapperEl = petRef.current;
          const canvasEl = wrapperEl?.querySelector("canvas") as HTMLCanvasElement | null;
          let cx = 0, cy = 0, cw = 0, ch = 0;
          if (canvasEl) {
            const cr = canvasEl.getBoundingClientRect();
            cx = cr.left; cy = cr.top; cw = cr.width; ch = cr.height;
          }
          // Canvas-local rect -> viewport, then clamp to canvas bounds.
          const rawLeft = cx + mb.x;
          const rawTop = cy + mb.y;
          const rawRight = rawLeft + mb.width;
          const rawBottom = rawTop + mb.height;
          const clampedLeft = Math.max(rawLeft, cx);
          const clampedTop = Math.max(rawTop, cy);
          const clampedRight = Math.min(rawRight, cx + cw);
          const clampedBottom = Math.min(rawBottom, cy + ch);
          o.style.left = `${clampedLeft}px`;
          o.style.top = `${clampedTop}px`;
          o.style.width = `${Math.max(0, clampedRight - clampedLeft)}px`;
          o.style.height = `${Math.max(0, clampedBottom - clampedTop)}px`;
          if (
            !boundsShownRef.current &&
            performance.now() >= boundsCooldownUntilRef.current
          ) {
            boundsShownRef.current = true;
            // Cooldown starts at the show moment: approaches inside the window
            // are ignored, the next approach after it shows again (循环).
            boundsCooldownUntilRef.current =
              performance.now() + TRIGGER_COOLDOWN_MS;
            o.classList.add("bounds-visible");
          }
          if (boundsHideTimerRef.current) {
            clearTimeout(boundsHideTimerRef.current);
            boundsHideTimerRef.current = null;
          }
        } else if (boundsShownRef.current && !boundsHideTimerRef.current) {
          boundsHideTimerRef.current = window.setTimeout(() => {
            boundsShownRef.current = false;
            if (boundsOverlayRef.current) {
              boundsOverlayRef.current.classList.remove("bounds-visible");
            }
            boundsHideTimerRef.current = null;
          }, 250);
        }
      }
      // Push diagnostics to the backend (throttled ~200ms) so the Debug Panel's
      // separate OS window can render them. Best-effort — a failed invoke must
      // never break click-through itself.
      const now = performance.now();
      if (now - lastDiagPushRef.current > 200) {
        lastDiagPushRef.current = now;
        clickthroughDiagRef.current = {
          has_origin: !!origin, has_scale: !!scale, has_canvas: !!canvas, has_bounds: !!mb,
          sx, sy, left, top, right, bottom, inside,
          ignore: forceCapture ? false : !inside,
          bounds_x: mb?.x ?? 0, bounds_y: mb?.y ?? 0, bounds_w: mb?.width ?? 0, bounds_h: mb?.height ?? 0,
          origin_x: origin?.x ?? 0, origin_y: origin?.y ?? 0, scale: scale ?? 0,
        };
        invoke("set_clickthrough_diag", { diag: clickthroughDiagRef.current }).catch(() => {});
      }
      if (forceCapture) {
        applyIgnore(false);
        return;
      }
      // Missing geometry -> stay fully interactive (safe default).
      if (!origin || !scale || !canvas || !mb) {
        applyIgnore(false);
        return;
      }
      // global-cursor 是穿透期间的唯一权威指针来源。即使后续 ignore=true，
      // pointerRef 仍持续更新它（client 坐标口径），供 gaze/视线使用。
      const clientX = (sx - origin.x) / scale;
      const clientY = (sy - origin.y) / scale;
      pointerRef.current = { x: clientX, y: clientY };
     applyIgnore(!inside);
    }).then((u) => { if (!cancelled) unlisten = u; else u(); }).catch(() => {});

    return () => {
      cancelled = true;
      unlisten?.();
      unlistenMoved?.();
      if (cursorWatchdog) clearTimeout(cursorWatchdog);
    };
  }, [applyIgnore, inputVisible, showSettings, isBeingDragged]);

  // P12: Physics + circadian loop (Body layer, independent of LLM)
  useEffect(() => {
    let raf = 0;
    let lastTime = performance.now();

   const loop = (now: number) => {
     const dt = Math.min(0.05, (now - lastTime) / 1000); // cap at 50ms
     lastTime = now;

      const gravity = gravityRef.current;
      const pos = petPosRef.current;

      // lbuttonRef gates BOTH phases of the drag/fall handshake: while the
      // button is held the user owns the window (OS drag loop), so a running
      // fall must not integrate and a new fall must not arm — otherwise the
      // rAF setPosition here fights the OS drag every frame (violent shake).
      if (pos && !isBeingDraggedRef.current && !lbuttonRef.current && !awayMode) {
        // B2 (P12.1): free-fall toward a hover point (1/3 of the way to the
        // taskbar). Runs until grounded.
        if (!gravity.grounded) {
          const bottom = pos.y + winSizeRef.current.h;
          const newBottom = stepGravity(gravity, dt, bottom);
          // 1/3-arc rule: she falls only a third of the way to the floor, then
          // floats to a stop (hover). Reaching the hover point plays the
          // settling sound.
          let finalBottom = newBottom;
          const limit = fallLimitBottomRef.current;
          if (limit > 0 && newBottom >= limit) {
            finalBottom = limit;
            gravity.grounded = true;
            gravity.vy = 0;
            fallLimitBottomRef.current = 0;
            sound.play("land"); // reached the hover point — soft settling sound
          }
          const newY = Math.round(finalBottom - winSizeRef.current.h);
          petPosRef.current = { x: pos.x, y: newY };
          moveWindowTo(pos.x, newY).catch(() => {});
        } else {
          // B2 (P12.1): drag-end detection. After a native drag the window
          // goes still once the user releases; if she was left above the
          // work-area bottom, start free-fall. Gates:
          // - !lbuttonRef: OS button truth — a mid-drag pause (button held)
          //   must not arm the fall (the original shake bug).
          // - fresh lastCursorEventAtRef: if the global-cursor pipeline went
          //   silent, lbutton is unverifiable — defer (keep wasDragged) until
          //   events flow again rather than arm on a guess.
          // - arm-time outerPosition() calibration: the arm uses the window's
          //   REAL position, not petPosRef. If the onMoved sync ever misses
          //   drag moves (the 松手回原位 bug reported 2026-08-15), a stale
          //   petPos made the fall teleport her back to where she was before
          //   the drag. Calibrating from the source of truth makes that
          //   physically impossible, and the >2px drift is logged as
          //   telemetry (lastArmDriftRef / __dragDiag).
          if (
            wasDraggedRef.current &&
            !lbuttonRef.current &&
            !armPendingRef.current &&
            now - lastCursorEventAtRef.current < 1500 &&
            now - lastMovedRef.current > 300
          ) {
            wasDraggedRef.current = false;
            armPendingRef.current = true;
            const f = scaleFactorRef.current || 1;
            getCurrentWindow()
              .outerPosition()
              .then((phys) => {
                armPendingRef.current = false;
                // The user may have re-grabbed her during the IPC roundtrip —
                // arming now would start the fall mid-drag (fight).
                if (lbuttonRef.current || isBeingDraggedRef.current) return;
                const real = { x: phys.x / f, y: phys.y / f };
                // Snap-undo fallback: the onMoved fast path (above) handles
                // the normal case; this covers the rare one where the snap's
                // move event never reached the webview. Same condition — a
                // release off-screen at the top that ended up snapped to y≥0.
                // Idempotent: after a restore, outerPosition() == held.
                const held = releasePosRef();
                let pos = real;
                if (held && held.y < -SNAP_GAP && real.y < SNAP_GAP && real.y > held.y + SNAP_GAP) {
                  const restore = clampModelToScreen(held);
                  // Only act when it actually moves the window — otherwise this
                  // is the wall-clamp false positive (cursor past the wall, she
                  // already sits at the clamped target), not an OS snap.
                  if (Math.abs(restore.x - real.x) > 0.5 || Math.abs(restore.y - real.y) > 0.5) {
                    lastSnapRestoreRef.current = { held: restore, snapped: real, at: Date.now() };
                    console.warn("[drag] OS top-clamp snap undone (arm fallback)", { held: restore, real });
                    pos = restore;
                    moveWindowTo(restore.x, restore.y).catch((err) =>
                      console.warn("[drag] snap-undo setPosition failed", err),
                    );
                  }
                } else {
                  // No snap to undo — but the release might still be off-screen
                  // (e.g. parked against the left/right/bottom edge). Pull the
                  // model back fully on-screen so she's always grabbable.
                  const clamped = clampModelToScreen(pos);
                  if (Math.abs(clamped.x - pos.x) > 0.5 || Math.abs(clamped.y - pos.y) > 0.5) {
                    console.warn("[drag] release off-screen — clamped to visible", { real, clamped });
                    pos = clamped;
                    moveWindowTo(clamped.x, clamped.y).catch((err) =>
                      console.warn("[drag] visible-clamp setPosition failed", err),
                    );
                  }
                }
                const stale = petPosRef.current;
                const drift = stale ? Math.hypot(pos.x - stale.x, pos.y - stale.y) : Infinity;
                if (drift > 2) {
                  lastArmDriftRef.current = {
                    drift: Math.round(drift), stale, real: pos, at: Date.now(),
                  };
                  console.warn("[drag] stale petPos at fall-arm — calibrated instead", {
                    drift: Math.round(drift), stale, real: pos,
                  });
                }
                petPosRef.current = pos;
                // 2026-08-15 (user, after three escalating reports): she must
                // STAY where she is released — the post-release 1/3 hover-fall
                // kept moving her off the drop point ("回原位/触底触顶反弹/
                // 无法停在上面和下面"), and its size scales with altitude.
                // Flip this constant to resurrect the fall (1/3 arc, g=1200/9).
                const atFloor = pos.y + winSizeRef.current.h >= floorYRef.current - 2;
                if (ENABLE_POST_DRAG_FALL && !atFloor) {
                  gravityRef.current.grounded = false;
                  gravityRef.current.vy = 0;
                  // Only fall a third of the way to the floor (old preference).
                  fallLimitBottomRef.current =
                    pos.y + winSizeRef.current.h + (floorYRef.current - (pos.y + winSizeRef.current.h)) / 3;
                } else if (atFloor) {
                  sound.play("land"); // dropped right on the floor: thud now
                }
                // Mid-air + fall disabled → stays exactly where released.
              })
              .catch(() => { armPendingRef.current = false; });
          }
          // (2026-08-08) Walk-back-to-nest removed — 桌宠驻留，拖到哪停哪，
          // 不再自主走回角落窝。Body 层只剩自由落体 + 落地弹跳 + 昼夜节律。
        }
      }

     // Circadian update (every 30s is enough, but cheap to check each frame)
     circadianRef.current = getCircadianState();

     raf = requestAnimationFrame(loop);
   };

   raf = requestAnimationFrame(loop);
   return () => cancelAnimationFrame(raf);
  }, [awayMode]);

  // Drag/fall diagnostics (same dev-aid pattern as __ctDiag / __pet): reads
  // refs only, never mutates — lets CDP observe the handshake gates live
  // while a real OS drag is in progress (webview mouse events are swallowed
  // then, so this is the only window into the physics state).
  useEffect(() => {
    (window as any).__dragDiag = () => ({
      lbutton: lbuttonRef.current,
      wasDragged: wasDraggedRef.current,
      isBeingDragged: isBeingDraggedRef.current,
      armPending: armPendingRef.current,
      grounded: gravityRef.current.grounded,
      vy: Math.round(gravityRef.current.vy),
      petPos: petPosRef.current,
      lastMovedAgoMs: Math.round(performance.now() - lastMovedRef.current),
      lastCursorEventAgoMs: Math.round(performance.now() - lastCursorEventAtRef.current),
      lastArmDrift: lastArmDriftRef.current,
      lastSnapRestore: lastSnapRestoreRef.current,
      lastHeldPos: lastHeldPosRef.current,
      lastHeldCursor: lastHeldCursorRef.current,
      grabOffset: grabOffsetRef.current,
      releasePos: releasePosRef(),
      dragging: draggingRef.current,
      canvas: canvasRectRef.current,
      modelBounds: modelBoundsRef.current,
      visualBounds: visualBoundsRef.current,
      screenSize: screenSizeRef.current,
      fallLimitBottom: Math.round(fallLimitBottomRef.current),
      floorY: Math.round(floorYRef.current),
      winH: Math.round(winSizeRef.current.h),
    });
    return () => { delete (window as any).__dragDiag; };
  }, []);

  // P12: DeepNight/LateNight proactive nudge. Extracted into a callback so the
  // dev verify hook (window.__pet.probeNudge) can fire one on demand instead of
  // waiting the full 10-min interval. Yields for the rest of the day once the
  // 晚安 ritual has been said — one bedtime voice per day, not two.
  const runNudge = useCallback(() => {
    if (awayMode) return;
    // She's asleep — don't sleep-talk the "go to bed" nudge (#10).
    if (fsmRef.current?.state === BehaviorState.Sleeping) return;
    invoke<boolean>("ritual_done_today", { kind: "goodnight" })
      .then((done) => {
        if (done) return;
        const circ = getCircadianState();
        if (circ.period === TimeOfDay.DeepNight && Math.random() < 0.4) {
          const msgs = deepNightMessages();
          showBubble(msgs[Math.floor(Math.random() * msgs.length)], 8000, "bubble-worried");
        } else if (circ.period === TimeOfDay.LateNight && Math.random() < 0.2) {
          showBubble("还不睡呀…", 6000, "bubble-sad");
        }
      })
      .catch(() => {/* backend unavailable → keep the legacy behavior */});
  }, [showBubble, awayMode]);

  // Nudge check every 10 min (fires once per period, probabilistically).
  useEffect(() => {
    const timer = setInterval(runNudge, 10 * 60 * 1000);
    return () => clearInterval(timer);
  }, [runNudge]);

// P12: Drag handling
  // 方案 B: drag the pet = drag the OS window. NATIVE startDragging() for zero
  // jitter, but DEFERRED until the pointer actually moves past a threshold — so a
  // pure click (press+release without moving) never enters the OS drag loop and
  // Spine hit-test clicks (head/body bubbles) still fire normally.
  const DRAG_THRESHOLD = 5;
  // 2026-08-15: post-drag free-fall DISABLED by user decision — she stays
  // exactly where released ("无法停在上面和下面/回原位" across three reports;
  // the fall's 1/3-arc scales with altitude so it read as bouncing back).
  // Set true to bring the 1/3 hover-fall back (arm path is kept intact).
  const ENABLE_POST_DRAG_FALL = false;
  // Minimum off-screen release distance (logical px) that triggers the OS
  // top-clamp snap-undo (see onMoved + arm fallback). Windows forces a window
  // released with its top edge above the screen back to y=0; releases at
  // least this far off-screen get restored to the actual release point.
  const SNAP_GAP = 10;

  // Any user interaction refreshes the DeepNight sleep-idle timer and wakes the
  // pet if asleep. Stable (deps []): only touches refs, so callers may omit it
  // from their own deps without stale-closure risk. forceState bypasses the
  // transition priority lock so Sleeping can exit.
  const markInteraction = useCallback(() => {
    lastInteractionRef.current = Date.now();
    if (fsmRef.current?.state === BehaviorState.Sleeping) {
      fsmRef.current.forceState(BehaviorState.Idle);
    }
  }, []);

  // DEV-ONLY verify hook (北极星 #7: invisible in release — Vite replaces
  // import.meta.env.DEV with false and dead-code-eliminates this effect).
  // Lets us验收 circadian / Sleeping / B3 WITHOUT touching the OS clock or
  // waiting 10 min. Open browser DevTools (right-click pet → DevTools, dev
  // only — calls open_devtools; NOTE F12 is the in-app Debug Panel, different)
  // → Console → window.__pet. See docs/verify-checklist.md.
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    const origGetHours = Date.prototype.getHours;
    const api = {
      // Pretend it's hour h. circadian.ts is the only getHours caller; Date.now()
      // is untouched, so idle/sleep timers still use the real clock.
      setHour: (h: number): void => { Date.prototype.getHours = (): number => h; },
      // Restore the real clock.
      resetHour: (): void => { Date.prototype.getHours = origGetHours; },
      // Back-date last interaction so the DeepNight auto-sleep guard fires on
      // the next 2.5s tick — skips the 10-min idle wait, real code path.
      forceIdle: (mins: number): void => {
        lastInteractionRef.current = Date.now() - mins * 60_000;
      },
      // Enter/exit Sleeping directly (sleeping pose + sleep sound).
      sleep: (): void => {
        fsmRef.current?.forceState(BehaviorState.Sleeping);
        sound.sleep();
      },
      wake: (): void => { markInteraction(); },
      // Fire one nudge check now instead of waiting 10 min. No-op while asleep
      // (verifies B3① suppression); 0.4/0.2 random, so call a few times.
      probeNudge: (): void => { runNudge(); },
      state: (): Record<string, unknown> => ({
        behavior: fsmRef.current?.state,
        period: circadianRef.current.period,
        sleepiness: circadianRef.current.sleepiness,
        idleSecs: Math.round((Date.now() - lastInteractionRef.current) / 1000),
      }),
    };
    const w = window as unknown as { __pet?: typeof api };
    w.__pet = api;
    console.log("[dev] window.__pet ready: setHour/forceIdle/sleep/wake/probeNudge/state");
    return () => {
      Date.prototype.getHours = origGetHours; // never leak a fake hour
      delete w.__pet;
    };
  }, [runNudge, markInteraction]);

  const handleDragStart = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return; // left click only
    markInteraction();
    const startClientX = e.clientX;
    const startClientY = e.clientY;
    let dragStarted = false;
    let onMove: ((ev: MouseEvent) => void) | null = null;
    let onUp: ((ev: MouseEvent) => void) | null = null;

    const cleanup = () => {
      if (onMove) window.removeEventListener("mousemove", onMove);
      if (onUp) window.removeEventListener("mouseup", onUp);
    };

    // Pure click (press+release under the threshold): mouseup DOES reach the
    // page (no OS drag engaged), so detach here. Without this the mousemove
    // watcher stayed armed with the stale press point and a later hover-move
    // past it could spuriously call startDragging() with no button held —
    // stray drag sound + isBeingDragged stuck true (never click-through again
    // until the next real drag).
    onUp = () => cleanup();

    onMove = (_ev: MouseEvent) => {
      if (dragStarted) return; // OS now owns the drag
      const dx = _ev.clientX - startClientX;
      const dy = _ev.clientY - startClientY;
      if (Math.sqrt(dx * dx + dy * dy) < DRAG_THRESHOLD) return;
      // Real drag: engage manual drag mode. The OS native drag
      // (win.startDragging) is deliberately NOT used — it moves the window in
      // lockstep with the pointer past the screen boundary (head off-screen =
      // 穿模) and cannot be clamped. Instead the global-cursor pipeline
      // (125Hz while the button is held, GetAsyncKeyState) drives the window
      // to a clamped target below,
      // so the pet's body can never leave the monitor.
      dragStarted = true;
      wasDraggedRef.current = true;
      isBeingDraggedRef.current = true;
      setIsBeingDragged(true);
      draggingRef.current = true;
      sound.play("drag");
      // The cursor↔window offset is FIXED from here on, so the release
      // position can be recovered as lastHeldCursor − grabOffset (also immune
      // to the top-clamp snap's move event racing ahead of lbutton=false).
      grabOffsetRef.current = {
        x: _ev.clientX * (scaleFactorRef.current || 1),
        y: _ev.clientY * (scaleFactorRef.current || 1),
      };
      cleanup(); // stop watching; the cursor pipeline drives movement now.
      // NOTE: no mouseup path here — drag-end is detected via the pipeline's
      // lbutton=false transition (see the gravity loop / global-cursor).
    };

    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }, []);

  // overrideText lets Escape submit an empty answer during onboarding (see onKeyDown).
  const handleApplyEdit = useCallback(async (approve: boolean) => {
    const proposal = editProposal;
    if (!proposal) return;
    setEditProposal(null);
    setEditOutcome(null);
    try {
      const outcome = await invoke<EditApplyOutcome>("apply_edit_proposal", {
        id: proposal.id,
        approve,
      });
      setEditOutcome(outcome);
      showBubble(
        outcome.status === "saved"
          ? "已经改好啦，就动了你说的那一处~"
          : outcome.message,
        12000,
        outcome.status === "failed" ? "bubble-worried" : "bubble-calm",
      );
    } catch (e) {
      console.error("[edit_file] apply_edit_proposal failed:", e);
      showBubble("……这个确认卡没能处理，先算了吧", 5000, "bubble-worried");
    }
  }, [editProposal]);

  const handleUndoEdit = useCallback(async () => {
    setEditOutcome(null);
    try {
      const outcome = await invoke<EditApplyOutcome>("undo_last_edit");
      setEditOutcome(outcome);
      showBubble(outcome.message, 12000, outcome.status === "failed" ? "bubble-worried" : "bubble-calm");
    } catch (e) {
      console.error("[edit_file] undo_last_edit failed:", e);
      showBubble("……撤销没成功", 5000, "bubble-worried");
    }
  }, []);

  const handleSend = useCallback(async (overrideText?: string) => {
    markInteraction();
    const text = (overrideText !== undefined ? overrideText : inputText).trim();
    // 访谈分流：存答案→推进→收尾，不走正常对话
    if (onboarding?.active) {
      const { step, answers } = onboarding;
      const key = ONBOARD_QUESTIONS[step].key;
      try { await invoke("save_onboarding_answer", { key, value: text }); }
      catch (e) { console.warn("save_onboarding_answer", e); }
      setInputText("");
      const nextStep = step + 1;
      if (nextStep < ONBOARD_QUESTIONS.length) {
        setOnboarding({ active: true, step: nextStep, answers: { ...answers, [key]: text } });
        showBubble(ONBOARD_QUESTIONS[nextStep].ask, 120000, "bubble-calm");
        requestAnimationFrame(() => {
          document.querySelector<HTMLInputElement>(".input-bubble input")?.focus();
        });
      } else {
        try { await invoke("complete_onboarding"); }
        catch (e) { console.warn("complete_onboarding", e); }
        onboardingActiveRef.current = false;
        setOnboarding(null);
        showBubble("认识你真高兴！以后就这么陪着你啦~", 10000, "bubble-happy");
      }
      return;
    }
    if (!text) { setInputVisible(false); return; }
    setInputText("");
    setInputVisible(false); // 轮次制：发送后立即收起输入框，让桌宠回复气泡独占显示
    // A new speech act supersedes any unanswered edit card / apply result.
    setEditProposal(null);
    setEditOutcome(null);
    sound.play("send");
   setIsThinking(true);
   fsmRef.current?.forceState(BehaviorState.Thinking);
    thinkStartRef.current = Date.now();

   try {
     // Stream tokens live: the first content chunk means she starts speaking
     // (drop dots, switch to Talking, open the bubble); later chunks accumulate
     // into the bubble text. DeepSeek emits reasoning before content, so dots
     // stay up while she "thinks" (architecture #10, 踩坑#3).
     // Stream tokens via a Tauri event. Chunks arrive faster than the eye can
     // follow (DeepSeek emits ~80 tokens/s) and React batches same-tick
     // setState into one render — so direct accumulation visibly "jumps". We
     // buffer into a ref and reveal on a 30ms interval => smooth typewriter
     // (architecture #10).
     streamBufRef.current = "";
     shownLenRef.current = 0;
     streamEndedRef.current = false;
     if (typewriterRef.current) { window.clearInterval(typewriterRef.current); typewriterRef.current = null; }

     let firstChunk = true;
     const onChunk = new Channel<string>();
     onChunk.onmessage = (delta: string) => {
       streamBufRef.current += delta; // buffer only — no setState here (defeats batching)
       if (firstChunk) {
         firstChunk = false;
         // Typewriter cadence follows THIS turn's emotion (#10): happy input
         // flows fast, sad/worried drags with pauses. The backend moodLabel is
         // a slow variable (only re-derived after the reply), so we infer the
         // pacing mood from the user's own words for immediacy; moodLabel is
         // just the fallback when no emotion keyword is present.
         const pacing = typewriterPacing(inferPacingMood(text, moodLabel));
         setIsThinking(false);
         fsmRef.current?.forceState(BehaviorState.Talking);
         // Mint a new bubble identity for this speech act — AnimatePresence will
         // exit the prior bubble and enter this one. Streaming tokens below do
         // NOT call beginBubble, so they grow the text without replaying enter.
         const id = beginBubble();
         setBubbleStyle(bubbleClassForMood(moodLabel));
         setBubblePos("");
         setGlyphKind(undefined);
         setBubbleText("");
         setBubbleId(id);
         setBubbleVisible(true);
         if (bubbleTimerRef.current) clearTimeout(bubbleTimerRef.current); // no auto-hide mid-stream
         typewriterRef.current = window.setInterval(() => {
           // Hesitate: while still streaming, skip a tick now and then = a
           // pause (hesitation / breath). Never hesitate once ended — the
           // final reveal must complete.
           if (!streamEndedRef.current && pacing.hesitate > 0 && Math.random() < pacing.hesitate) return;
           const buf = streamBufRef.current;
           if (shownLenRef.current < buf.length) {
             const step = Math.max(1, Math.ceil((buf.length - shownLenRef.current) / pacing.catchDiv));
             shownLenRef.current += step;
             setBubbleText(buf.slice(0, shownLenRef.current));
           } else if (streamEndedRef.current) {
             if (typewriterRef.current) { window.clearInterval(typewriterRef.current); typewriterRef.current = null; }
             bubbleTimerRef.current = setTimeout(() => setBubbleVisible(false), 10000);
             setTimeout(() => fsmRef.current?.forceState(BehaviorState.Idle), 2000);
           }
         }, pacing.intervalMs);
       }
     };
     const res = await invoke<{ reply: string; transient_expression: string | null; edit_proposal: EditProposalInfo | null }>(
       "send_message",
       { text, onChunk },
     );
     // Stream done: correct the buffer to the authoritative full reply (in
     // case the last chunks raced the unlisten), then let the typewriter
     // finish revealing it.
     streamBufRef.current = res.reply;
     streamEndedRef.current = true;

     if (firstChunk) {
       // No content tokens arrived (silence / empty reply): the typewriter
       // never started — fall back to the original thinking→reply path.
       const elapsed = Date.now() - thinkStartRef.current;
       if (elapsed < 500) await new Promise((r) => setTimeout(r, 500 - elapsed));
       setIsThinking(false);
       fsmRef.current?.forceState(BehaviorState.Talking);
       if (res.reply) showBubble(res.reply, 10000, bubbleClassForMood(moodLabel));
       else showBubble("…", 2500, "bubble-glyph", "", "surprise");
       setTimeout(() => fsmRef.current?.forceState(BehaviorState.Idle), 2000);
     }
      // F2: surface the confirm card when the reply carried a valid proposal.
      if (res.edit_proposal) {
        setEditProposal(res.edit_proposal);
        setEditOutcome(null);
      }
      // Refresh emotion immediately so the expression changes right after the
      // reply, instead of waiting up to 5s for the next poll.
      invoke<EmotionData>("get_emotion_state")
        .then((emo) => {
          setMoodLabel(emo.mood_label);
        })
        .catch(() => {});
   } catch (e) {
      // FIX-G: keep the dots up for >= 500ms even when it fails fast.
      const elapsed = Date.now() - thinkStartRef.current;
      if (elapsed < 500) await new Promise((r) => setTimeout(r, 500 - elapsed));
     setIsThinking(false);
     fsmRef.current?.forceState(BehaviorState.Idle);
      // FIX-F: surface a readable error instead of a bare "...", and log full detail.
      console.error("[Conversation] send_message failed:", e);
     const errMsg = String(e);
     if (errMsg.includes("not configured") || errMsg.includes("NotConfigured")) {
        showBubble("（还没有配置好连接……）", 5000, "bubble-worried");
     } else if (errMsg.includes("Timeout") || errMsg.includes("timeout")) {
        showBubble("我……刚刚有点走神……", 5000, "bubble-worried");
     } else if (errMsg.includes("Network") || errMsg.includes("network")) {
        showBubble("信号不太好呢……", 5000, "bubble-worried");
     } else if (errMsg.includes("RateLimit") || errMsg.includes("429")) {
        showBubble("说了好多话，让我喘口气吧~", 5000, "bubble-calm");
     } else {
        showBubble("（连接出了点问题…）", 5000, "bubble-worried");
    }
  }
}, [inputText, showBubble, moodLabel, onboarding]);

 // Head pet: closeness + mood up, 3s cooldown
 const handleHeadClick = useCallback(() => {
   markInteraction();
   const now = Date.now();
   if (now - lastPetTimeRef.current < 3000) return;
   lastPetTimeRef.current = now;
    // 单/双击消歧：与 body 一致，延迟 280ms，双击取消
    if (pendingPokeRef.current) clearTimeout(pendingPokeRef.current);
    pendingPokeRef.current = setTimeout(() => {
      pendingPokeRef.current = null;
      sound.play(closenessRef.current >= INTIMATE_THRESHOLD ? "pet-intimate" : "pet-stranger");
      invoke("pet_head").catch(() => {});
      fsmRef.current?.transition(BehaviorState.Embarrassed);
      // 摸头反应按亲密度分档（与音效一致）：熟络=撒娇开心，陌生=拘谨害羞
      // （害羞用 bubble-shy 慢浮现，与 续³ 低亲密度→害羞 的情绪设计对齐）
      const intimate = closenessRef.current >= INTIMATE_THRESHOLD;
      const pool = intimate
        ? ["嘿嘿…", "谢谢你～", "抹抹～", "最喜欢你摸头啦～"]
        : ["呜…", "啊…", "怎、怎么了…？"];
      const variant = intimate ? "bubble-happy" : "bubble-shy";
      showBubble(pool[Math.floor(Math.random() * pool.length)], 3000, variant, "bubble-pet");
      setTimeout(() => fsmRef.current?.forceState(BehaviorState.Idle), 1500);
    }, 280);
 }, [showBubble]);

// Body poke: mood down after 3 pokes
const handleBodyClick = useCallback(() => {
  if (inputVisible) return; // 输入框打开时不弹身体气泡
  markInteraction();
  // 单/双击消歧：戳反应延迟 280ms，双击会在 dblclick 里取消它
  if (pendingPokeRef.current) clearTimeout(pendingPokeRef.current);
  pendingPokeRef.current = setTimeout(() => {
    pendingPokeRef.current = null;
    pokeCountRef.current++;
    if (pokeResetTimerRef.current) clearTimeout(pokeResetTimerRef.current);
    pokeResetTimerRef.current = setTimeout(() => { pokeCountRef.current = 0; }, 5000);
    fsmRef.current?.transition(BehaviorState.Embarrassed);
    setTimeout(() => fsmRef.current?.forceState(BehaviorState.Idle), 1500);
    const n = pokeCountRef.current;
    sound.play(n >= 3 ? "poke3" : n === 2 ? "poke2" : "poke1");
    if (n === 1) {
      showBubble("怎么啦？", 2500, "bubble-calm");
    } else if (n === 2) {
      showBubble("别戳啦～痒痒的…", 2500, "bubble-playful");
    } else {
      showBubble("再戳我要生气啦！", 3000, "bubble-worried");
    }
    invoke<boolean>("poke", { count: pokeCountRef.current }).catch(() => {});
  }, 280);
}, [showBubble, inputVisible]);

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    sound.play("menu");
    setContextMenu({ x: e.clientX, y: e.clientY });
  }, []);

  // Memory export: pops a native save dialog, then hands the chosen path to a
  // Rust command that writes the file (the webview can't write arbitrary
  // paths directly). Shows the full saved path in the bubble so the user
  // knows where the file landed — the old version silently downloaded a
  // truncated debug snapshot, which is why "导出成功" appeared to do nothing.
  const handleExportJson = useCallback(async () => {
    try {
      const path = await save({
        defaultPath: "pet-memory-backup.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path) return; // user cancelled
      showBubble("记忆导出中…", 3000, "bubble-calm");
      await invoke("export_memory_json", { path });
      showBubble("已备份到：" + path, 6000, "bubble-happy");
    } catch (e) {
      console.error("[Export JSON]", e);
      showBubble("导出失败了…", 3000, "bubble-worried");
    }
  }, [showBubble]);

  const handleExportMarkdown = useCallback(async () => {
    try {
      const path = await save({
        defaultPath: "pet-memory-export.md",
        filters: [{ name: "Markdown", extensions: ["md"] }],
      });
      if (!path) return;
      showBubble("记忆导出中…", 3000, "bubble-calm");
      await invoke("export_memory_markdown", { path });
      showBubble("已保存到：" + path, 6000, "bubble-happy");
    } catch (e) {
      console.error("[Export MD]", e);
      showBubble("导出失败了…", 3000, "bubble-worried");
    }
  }, [showBubble]);

  const handleExportBoth = useCallback(async () => {
    try {
      const jsonPath = await save({
        defaultPath: "pet-memory-backup.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!jsonPath) return;
      const mdPath = await save({
        defaultPath: "pet-memory-export.md",
        filters: [{ name: "Markdown", extensions: ["md"] }],
      });
      if (!mdPath) return;
      showBubble("记忆导出中…", 3000, "bubble-calm");
      await invoke("export_memory_both", { jsonPath, mdPath });
      showBubble("两份都存好了：" + jsonPath, 6000, "bubble-happy");
    } catch (e) {
      console.error("[Export both]", e);
      showBubble("导出失败了…", 3000, "bubble-worried");
    }
  }, [showBubble]);

  const handleAwayMode = useCallback(() => {
    setAwayMode(true);
    showBubble("我先休息一下哦~", 4000, "bubble-calm");
    // Hide the window to the system tray shortly after the bubble shows.
    // Clicking the tray icon restores it (clears awayMode via the
    // restore-from-tray listener below).
    setTimeout(() => { void invoke("hide_to_tray"); }, 600);
  }, [showBubble]);

  // Restore from tray: clicking the tray icon re-shows the window (Rust) and
  // emits this event. Clear awayMode and greet the user back.
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;
    (async () => {
      unlisten = await listen("restore-from-tray", () => {
        setAwayMode(false);
        showBubble("回来啦~", 3000, "bubble-happy");
      });
      if (cancelled) unlisten();
    })();
    return () => { cancelled = true; unlisten?.(); };
  }, [showBubble]);

  // Alt+Space global shortcut (P11.4): backend registered Alt+Space system-wide;
  // pressing it anywhere shows+focuses the window (Rust) and emits this event.
  // Summon the pet to talk — clear away mode, open the chat input, focus it.
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;
    (async () => {
      unlisten = await listen("show-input", () => {
        setAwayMode(false);
        setInputVisible(true);
        // setInputVisible is async; wait a frame for the input to mount, then focus.
        requestAnimationFrame(() => {
          document.querySelector<HTMLInputElement>(".input-bubble input")?.focus();
        });
      });
      if (cancelled) unlisten();
    })();
    return () => { cancelled = true; unlisten?.(); };
  }, []);

  const handleQuit = useCallback(() => {
    showBubble("再见…", 3000, "bubble-sad");
    // FIX: previously only showed the bubble and never closed. We now call a
    // Rust-side quit_app (app.exit(0)) which terminates the process
    // deterministically — window.destroy() alone did not reliably exit under
    // Tauri 2. 400ms lets the goodbye render before the process goes away.
    setTimeout(() => { void invoke("quit_app"); }, 400);
  }, [showBubble]);

  // Bubble/orb placement mode: when the window is parked with its top edge
  // off-screen (head at the screen top — the top wall), the default above-head
  // spots render entirely above the visible screen, so both flip below her
  // head. Evaluated at render time from petPosRef — every bubble show and both
  // drag boundaries (drag start/end setState) trigger renders, so it's fresh
  // whenever placement can change. -6px grace: at borderline parks the tallest
  // bubble loses ≤6 off-screen px, not worth flipping for.
  const bubbleBelow = (petPosRef.current?.y ?? 0) < -6;

  return (
    <div className="pet-container" onContextMenu={handleContextMenu}>
      {isThinking && (
        <div className={`thinking-orb${bubbleBelow ? " thinking-orb--below" : ""}`}>
          <ThinkingOrb state={THINKING_ORB_STATE} size={THINKING_ORB_SIZE} theme="auto" />
        </div>
      )}

      <div className="input-bubble">
          <input
            type="text"
            value={inputText}
            autoFocus
            placeholder={"想和我说什么？"}
            onChange={(e) => setInputText(e.target.value)}
            onKeyDown={(e) => {
             if (e.key === "Enter") handleSend();
              if (e.key === "Escape") {
                // 访谈中 Escape 当作空答案，推进到下一题
                if (onboarding?.active) handleSend("");
                else setInputVisible(false);
              }
            }}
            // Input stays open during conversation; closed only by Esc or empty-message Enter.
            // 轮次制：失焦即收起输入框；访谈期间保持打开
            onBlur={() => {}}
          />
      </div>

      {editProposal && (
        <div className="edit-confirm-card" role="dialog" aria-label="修改文件确认">
          <div className="edit-confirm-title">要按这样改这段吗？</div>
          <div className="edit-confirm-path" title={editProposal.path}>{editProposal.path}</div>
          <pre className="edit-confirm-diff">{editProposal.diff_preview}</pre>
          <div className="edit-confirm-actions">
            <button className="edit-confirm-btn edit-confirm-no" onClick={() => handleApplyEdit(false)}>先不改</button>
            <button className="edit-confirm-btn edit-confirm-yes" onClick={() => handleApplyEdit(true)}>就这样改</button>
          </div>
        </div>
      )}
      {!editProposal && editOutcome && (
        <div className="edit-confirm-card edit-confirm-card--done" role="status">
          <div className="edit-confirm-title">文件修改</div>
          <div className="edit-confirm-message">{editOutcome.message}</div>
          {editOutcome.status === "saved" && (
            <div className="edit-confirm-actions">
              <button className="edit-confirm-btn edit-confirm-no" onClick={() => setEditOutcome(null)}>知道了</button>
              <button className="edit-confirm-btn edit-confirm-yes" onClick={handleUndoEdit}>撤销刚才修改</button>
            </div>
          )}
          {editOutcome.status !== "saved" && (
            <div className="edit-confirm-actions">
              <button className="edit-confirm-btn edit-confirm-no" onClick={() => setEditOutcome(null)}>知道了</button>
            </div>
          )}
        </div>
      )}

      {/* PetBubble: the pet's speech surface (Motion-animated). bubbleId = one
          speech act identity; a new id exits the old bubble and enters the new
          one. PetBubble wraps its own AnimatePresence internally, so we render
          it unconditionally and let `visible` drive enter/exit. mode="glyph"
          for wordless signals (呼… / 嗯？) renders shell-less. */}
      <PetBubble
        visible={bubbleVisible}
        text={bubbleText}
        bubbleId={bubbleId}
        variant={bubbleStyle}
        mode={glyphKind !== undefined || bubbleStyle === "bubble-glyph" ? "glyph" : "speech"}
        below={bubbleBelow}
        className={bubblePos}
        onBubbleBounds={handleBubbleBounds}
      />

     <div
       ref={petRef}
      className={`pet-char-wrapper ${isBeingDragged ? "dragging" : ""}`}
      data-behavior={behavior}
      onDoubleClick={() => {
        markInteraction();
        sound.play("dblclick");
        if (pendingPokeRef.current) { clearTimeout(pendingPokeRef.current); pendingPokeRef.current = null; }
        setBubbleVisible(false);
        if (bubbleTimerRef.current) clearTimeout(bubbleTimerRef.current);
        setInputVisible(true);
      }}
      onMouseDown={handleDragStart}
   >
    <SpineCanvas
      speedModifier={circadianRef.current.speedModifier}
      behavior={behavior}
      pointerRef={pointerRef}
      onHeadClick={handleHeadClick}
      onBodyClick={handleBodyClick}
      onModelBounds={handleModelBounds}
      onModelHitBounds={handleModelHitBounds}
      onVisualBounds={handleVisualBounds}
    />
    {/* Click-through boundary visualization (AIRI-style). Hidden by default;
        gains .bounds-visible when the cursor is near the model rect's outline
        (±BAND px). pointer-events:none so it never blocks clicks. Position is
        written imperatively from the global-cursor listener (canvas-local CSS
        px, matching modelBoundsRef) to avoid per-frame React re-renders. */}
    <div ref={boundsOverlayRef} className="bounds-overlay" />
   </div>

      {/* Settings lives in the context menu (2026-08-15): the corner button sat
          outside the model's hit rect, so the click-through window let every
          click fall through to the desktop — it could never be pressed.
          Right-click → 模型与设置 opens the same panel (showSettings forces
          capture, so the panel itself is fully interactive). */}
      {contextMenu && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={() => setContextMenu(null)}
          onExportJson={handleExportJson}
          onExportMarkdown={handleExportMarkdown}
          onExportBoth={handleExportBoth}
          onAwayMode={handleAwayMode}
          soundMuted={soundMuted}
          onToggleSound={() => setSoundMuted(sound.toggleMuted())}
          onOpenSettings={() => setShowSettings(true)}
         onQuit={handleQuit}
          onDevTools={() => invoke("open_devtools")}
       />
      )}

      {showSettings && <SettingsPanel onClose={() => setShowSettings(false)} />}
    </div>
  );
}
