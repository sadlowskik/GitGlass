/** @type {import('tailwindcss').Config} */
export default {
  darkMode: "class",
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // Neutral surface ramp — used for panels, backgrounds, borders.
        surface: {
          0: "rgb(var(--surface-0) / <alpha-value>)",
          1: "rgb(var(--surface-1) / <alpha-value>)",
          2: "rgb(var(--surface-2) / <alpha-value>)",
          3: "rgb(var(--surface-3) / <alpha-value>)",
        },
        content: {
          strong: "rgb(var(--content-strong) / <alpha-value>)",
          DEFAULT: "rgb(var(--content) / <alpha-value>)",
          muted: "rgb(var(--content-muted) / <alpha-value>)",
          faint: "rgb(var(--content-faint) / <alpha-value>)",
        },
        // Git status semantic colors (single source of truth for badges).
        git: {
          staged: "rgb(var(--git-staged) / <alpha-value>)",
          modified: "rgb(var(--git-modified) / <alpha-value>)",
          untracked: "rgb(var(--git-untracked) / <alpha-value>)",
          ignored: "rgb(var(--git-ignored) / <alpha-value>)",
          conflict: "rgb(var(--git-conflict) / <alpha-value>)",
        },
        accent: {
          DEFAULT: "rgb(var(--accent) / <alpha-value>)",
          soft: "rgb(var(--accent-soft) / <alpha-value>)",
        },
        github: "rgb(var(--github) / <alpha-value>)",
      },
      borderRadius: {
        xl: "0.875rem",
        "2xl": "1.125rem",
      },
      boxShadow: {
        panel: "0 1px 0 0 rgb(255 255 255 / 0.04) inset, 0 8px 30px -12px rgb(0 0 0 / 0.5)",
        glass: "0 1px 0 0 rgb(255 255 255 / 0.06) inset, 0 20px 40px -20px rgb(0 0 0 / 0.55)",
      },
      transitionTimingFunction: {
        spring: "cubic-bezier(0.22, 1, 0.36, 1)",
      },
      keyframes: {
        "slide-up": {
          from: { transform: "translateY(100%)", opacity: "0" },
          to: { transform: "translateY(0)", opacity: "1" },
        },
        "fade-in": {
          from: { opacity: "0" },
          to: { opacity: "1" },
        },
      },
      animation: {
        "slide-up": "slide-up 260ms cubic-bezier(0.22, 1, 0.36, 1)",
        "fade-in": "fade-in 180ms ease-out",
      },
    },
  },
  plugins: [],
};
