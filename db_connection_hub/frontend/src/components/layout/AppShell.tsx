// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { PropsWithChildren } from "react";

export function AppShell({ children }: PropsWithChildren) {
  return (
    <div className="shell">
      <header className="shell__header">
        <h1>DB Connection Hub</h1>
        <p>Rust backend + React frontend, modular metadata explorer.</p>
      </header>
      <main className="shell__main">{children}</main>
    </div>
  );
}
