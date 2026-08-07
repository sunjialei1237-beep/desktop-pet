import { defineConfig } from "vitest/config";

// Pure-logic unit tests for the frontend (circadian / sleep / microbehavior).
// Node environment — these modules have no DOM dependencies, so we avoid
// pulling in jsdom. Vitest prefers this file over vite.config.ts (whose async
// server config is dev-only).
export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
