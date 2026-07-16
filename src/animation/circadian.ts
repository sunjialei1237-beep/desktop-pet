// Circadian Rhythm: biological clock, not emotion.
// Design doc 6.7: Body-layer independent state source.
// Outputs to Emotion (mood/energy influence) and FSM (idle weights, speed).

export enum TimeOfDay {
  Morning = "morning",       // 6-11: energetic, bouncy
  Afternoon = "afternoon",   // 11-17: normal
  Evening = "evening",       // 17-22: relaxed
  LateNight = "late_night",  // 22-2: sleepy, slow, yawns
  DeepNight = "deep_night",  // 2-6: nearly inactive, urges sleep
}

export interface CircadianState {
  period: TimeOfDay;
  energyModifier: number;  // multiplier for energy consumption
  speedModifier: number;   // multiplier for animation speed
  sleepiness: number;      // 0..1, affects idle weights toward sleep
}

export function getCircadianState(hour?: number): CircadianState {
  const h = hour ?? new Date().getHours();
  if (h >= 6 && h < 11) {
    return { period: TimeOfDay.Morning, energyModifier: 1.3, speedModifier: 1.2, sleepiness: 0.1 };
  }
  if (h >= 11 && h < 17) {
    return { period: TimeOfDay.Afternoon, energyModifier: 1.0, speedModifier: 1.0, sleepiness: 0.1 };
  }
  if (h >= 17 && h < 22) {
    return { period: TimeOfDay.Evening, energyModifier: 0.9, speedModifier: 0.9, sleepiness: 0.2 };
  }
  if (h >= 22 || h < 2) {
    return { period: TimeOfDay.LateNight, energyModifier: 0.5, speedModifier: 0.6, sleepiness: 0.6 };
  }
  return { period: TimeOfDay.DeepNight, energyModifier: 0.3, speedModifier: 0.4, sleepiness: 0.9 };
}

// DeepNight special: proactive nudge messages
export function deepNightMessages(): string[] {
  return [
    "\u8fd9\u4e48\u665a\u4e86\u8fd8\u4e0d\u7761\u5440\u2026",
    "\u522b\u71ac\u591c\u4e86\uff0c\u8eab\u4f53\u8981\u7d27\u2026",
    "\u65e9\u70b9\u7761\u5427\uff0c\u660e\u5929\u8fd8\u6709\u7cbe\u529b\u5417\uff1f",
  ];
}
