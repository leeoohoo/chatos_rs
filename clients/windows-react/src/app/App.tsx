import { Route, Routes } from "react-router";
import { AppShell } from "./AppShell";

export function App() {
  return (
    <Routes>
      <Route path="*" element={<AppShell />} />
    </Routes>
  );
}
