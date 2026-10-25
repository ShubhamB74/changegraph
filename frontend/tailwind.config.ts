import type { Config } from "tailwindcss";

// Palette: a blue-black ink for the canvas, with colour reserved for impact states
// (see src/lib/theme.ts). Nothing else on screen is allowed to be saturated.
const config: Config = {
  content: ["./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        ink: {
          DEFAULT: "#0E131A",
          panel: "#141B25",
          raised: "#1B2431",
          line: "#283345",
          text: "#DDE5F1",
          dim: "#8C9AB0",
          faint: "#5E6D84",
        },
        signal: {
          changed: "#F5B942",
          direct: "#F2668B",
          transitive: "#A08BFF",
          uncertain: "#C5CEDC",
          test: "#4FD1C5",
          limit: "#FF8A5B",
          ok: "#5BD6A0",
        },
      },
      fontFamily: {
        sans: ["var(--font-ui)", "system-ui", "sans-serif"],
        mono: ["var(--font-mono)", "ui-monospace", "monospace"],
      },
    },
  },
  plugins: [],
};
export default config;
