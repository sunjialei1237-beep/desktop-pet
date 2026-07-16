import { memo } from "react";
import { BehaviorState } from "./animation/fsm";
import { AttentionState } from "./animation/attention";

type MoodLabel = string;

interface PetCharacterProps {
 moodLabel: MoodLabel;
 isThinking: boolean;
 behavior: BehaviorState;
  attention: AttentionState;
  headAngleX: number;
  headAngleY: number;
  onHeadClick: () => void;
  onBodyClick: () => void;
}

function renderEyes(mood: string, thinking: boolean) {
  if (thinking) {
    return (
      <>
        <circle cx="135" cy="195" r="8" fill="#4a4a6a" opacity="0.6" />
        <circle cx="185" cy="195" r="8" fill="#4a4a6a" opacity="0.6" />
      </>
    );
  }
  switch (mood) {
    case "开心":
      return (
        <>
          <path d="M125 200 Q135 188 145 200" stroke="#4a4a6a" strokeWidth="4" fill="none" strokeLinecap="round" />
          <path d="M175 200 Q185 188 195 200" stroke="#4a4a6a" strokeWidth="4" fill="none" strokeLinecap="round" />
        </>
      );
    case "调皮":
      return (
        <>
          <path d="M125 198 L145 192 M125 192 L145 198" stroke="#4a4a6a" strokeWidth="3" fill="none" strokeLinecap="round" />
          <path d="M175 198 L195 192 M175 192 L195 198" stroke="#4a4a6a" strokeWidth="3" fill="none" strokeLinecap="round" />
        </>
      );
    case "平静":
      return (
        <>
          <line x1="128" y1="195" x2="142" y2="195" stroke="#4a4a6a" strokeWidth="3" strokeLinecap="round" />
          <line x1="178" y1="195" x2="192" y2="195" stroke="#4a4a6a" strokeWidth="3" strokeLinecap="round" />
        </>
      );
    case "难过":
      return (
        <>
          <path d="M125 190 Q135 200 145 190" stroke="#4a4a6a" strokeWidth="3" fill="none" strokeLinecap="round" />
          <path d="M175 190 Q185 200 195 190" stroke="#4a4a6a" strokeWidth="3" fill="none" strokeLinecap="round" />
        </>
      );
    case "疲惫":
      return (
        <>
          <line x1="128" y1="193" x2="142" y2="197" stroke="#4a4a6a" strokeWidth="3" strokeLinecap="round" />
          <line x1="178" y1="193" x2="192" y2="197" stroke="#4a4a6a" strokeWidth="3" strokeLinecap="round" />
        </>
      );
    case "担心":
      return (
        <>
          <circle cx="135" cy="195" r="6" fill="#4a4a6a" />
          <circle cx="185" cy="195" r="6" fill="#4a4a6a" />
        </>
      );
    default:
      return (
        <>
          <circle cx="135" cy="195" r="7" fill="#4a4a6a" />
          <circle cx="185" cy="195" r="7" fill="#4a4a6a" />
        </>
      );
  }
}

function renderMouth(mood: string, thinking: boolean) {
  if (thinking) {
    return <circle cx="160" cy="235" r="5" fill="#4a4a6a" opacity="0.5" />;
  }
  switch (mood) {
    case "开心":
      return <path d="M140 225 Q160 245 180 225" stroke="#4a4a6a" strokeWidth="3.5" fill="none" strokeLinecap="round" />;
    case "调皮":
      return <path d="M140 230 Q155 240 180 228" stroke="#4a4a6a" strokeWidth="3" fill="none" strokeLinecap="round" />;
    case "平静":
      return <path d="M148 232 Q160 236 172 232" stroke="#4a4a6a" strokeWidth="3" fill="none" strokeLinecap="round" />;
    case "难过":
      return <path d="M140 238 Q160 225 180 238" stroke="#4a4a6a" strokeWidth="3" fill="none" strokeLinecap="round" />;
    case "疲惫":
      return <line x1="150" y1="234" x2="170" y2="234" stroke="#4a4a6a" strokeWidth="3" strokeLinecap="round" />;
    case "担心":
      return <path d="M145 235 Q160 230 175 235" stroke="#4a4a6a" strokeWidth="3" fill="none" strokeLinecap="round" />;
    default:
      return <path d="M145 230 Q160 238 175 230" stroke="#4a4a6a" strokeWidth="3" fill="none" strokeLinecap="round" />;
  }
}

function PetCharacterComponent({
  moodLabel, isThinking, behavior,
  attention, headAngleX, headAngleY,
  onHeadClick, onBodyClick,
}: PetCharacterProps) {
 // Map behavior states to CSS classes for animation
 const behaviorClass = behavior === BehaviorState.LookAround ? " look-around"
   : behavior === BehaviorState.Yawn ? " yawn"
   : behavior === BehaviorState.Stretch ? " stretch"
   : behavior === BehaviorState.Sway ? " sway"
   : behavior === BehaviorState.Peek ? " peek"
   : behavior === BehaviorState.Sleeping ? " sleeping"
   : behavior === BehaviorState.TiltHead ? " tilt-head"
   : "";

  // Peripheral attention: eyes/face shift slightly toward cursor
  const eyeOffsetX = attention === AttentionState.Peripheral ? headAngleX * 4 : 0;
  const eyeOffsetY = attention === AttentionState.Peripheral ? headAngleY * 3 : 0;
  const focusedClass = attention === AttentionState.Focused ? " attention-focused" : "";

 return (
   <svg
     viewBox="0 0 320 400"
      className={`pet-svg ${isThinking ? "thinking" : ""}${behaviorClass}${focusedClass}`}
     style={{ width: "200px", height: "250px", overflow: "visible" }}
   >
      {/* Head click region: upper portion of the SVG */}
      <ellipse
        cx="160" cy="170" rx="110" ry="90"
        fill="transparent"
        style={{ cursor: "pointer" }}
        onClick={onHeadClick}
      />
      {/* Body click region: lower portion */}
      <ellipse
        cx="160" cy="320" rx="110" ry="80"
        fill="transparent"
        style={{ cursor: "pointer" }}
        onClick={onBodyClick}
    />
      <defs>
        <radialGradient id="bodyGrad" cx="40%" cy="35%">
          <stop offset="0%" stopColor="#c5a3f0" />
          <stop offset="60%" stopColor="#9d7ee0" />
          <stop offset="100%" stopColor="#7b5fc7" />
        </radialGradient>
        <radialGradient id="cheekGrad" cx="50%" cy="50%">
          <stop offset="0%" stopColor="rgba(255,150,180,0.5)" />
          <stop offset="100%" stopColor="rgba(255,150,180,0)" />
        </radialGradient>
      </defs>

      <ellipse cx="160" cy="370" rx="80" ry="12" fill="rgba(0,0,0,0.08)" />

      <path
        d="M160 80
           C 100 80, 60 130, 60 200
           C 60 290, 100 360, 160 360
           C 220 360, 260 290, 260 200
           C 260 130, 220 80, 160 80 Z"
        fill="url(#bodyGrad)"
        className="pet-body"
      />

      <ellipse cx="115" cy="225" rx="18" ry="12" fill="url(#cheekGrad)" />
      <ellipse cx="205" cy="225" rx="18" ry="12" fill="url(#cheekGrad)" />

      <g
        transform={`translate(${eyeOffsetX}, ${eyeOffsetY})`}
        style={{ transition: "transform 0.2s ease-out" }}
      >
        {renderEyes(moodLabel, isThinking)}
        {renderMouth(moodLabel, isThinking)}
      </g>

      {behavior === BehaviorState.Sleeping && (
        <text x="245" y="120" fontSize="20" fill="#aaa" opacity="0.6">Z z</text>
      )}

      <path
        d="M120 110 Q130 100 140 108"
        stroke="rgba(255,255,255,0.3)"
        strokeWidth="6"
        fill="none"
        strokeLinecap="round"
      />

      <ellipse cx="95" cy="300" rx="12" ry="8" fill="#7b5fc7" opacity="0.7" />
      <ellipse cx="225" cy="300" rx="12" ry="8" fill="#7b5fc7" opacity="0.7" />
    </svg>
  );
}

export const PetCharacter = memo(PetCharacterComponent);
