import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: [".tooling/**", ".npm-cache/**", "dist/**", "src-tauri/**"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
);
