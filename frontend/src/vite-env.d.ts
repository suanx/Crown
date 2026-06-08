/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly DEV: boolean;
  readonly PROD: boolean;
  readonly MODE: string;
  readonly VITE_API_MODE?: "mock" | "hybrid" | "tauri";
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
