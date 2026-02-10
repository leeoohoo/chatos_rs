/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_ADMIN_API?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
