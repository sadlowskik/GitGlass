import js from "@eslint/js";
import tseslint from "@typescript-eslint/eslint-plugin";
import tsparser from "@typescript-eslint/parser";
import reactHooks from "eslint-plugin-react-hooks";

const browserGlobals = {
  window: "readonly",
  document: "readonly",
  console: "readonly",
  localStorage: "readonly",
  setTimeout: "readonly",
  clearTimeout: "readonly",
  // DOM lib types referenced in annotations.
  HTMLElement: "readonly",
  SVGSVGElement: "readonly",
  React: "readonly",
};

const nodeGlobals = {
  process: "readonly",
  Buffer: "readonly",
  console: "readonly",
  __dirname: "readonly",
};

export default [
  { ignores: ["dist/", "src-tauri/", "node_modules/"] },
  js.configs.recommended,

  // Application source (TypeScript). tsc handles undefined-symbol checks, so
  // no-undef is disabled here per typescript-eslint guidance.
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: {
      parser: tsparser,
      parserOptions: { ecmaFeatures: { jsx: true }, sourceType: "module" },
      globals: browserGlobals,
    },
    plugins: {
      "@typescript-eslint": tseslint,
      "react-hooks": reactHooks,
    },
    rules: {
      ...tseslint.configs.recommended.rules,
      ...reactHooks.configs.recommended.rules,
      "no-undef": "off",
      "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_" }],
    },
  },

  // Build scripts and Vite config run in Node.
  {
    files: ["scripts/**/*.{js,mjs}", "*.config.{js,ts}", "vite.config.ts"],
    languageOptions: {
      parser: tsparser,
      parserOptions: { sourceType: "module" },
      globals: nodeGlobals,
    },
  },
];
