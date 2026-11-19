import { useEffect, useState, type RefObject } from "react";
import type { TerminalSurfaceHandle } from "./TerminalSurface";

export function TerminalSearchControls({
  className,
  onClear,
  resetKey,
  surfaceRef,
}: {
  className: string;
  onClear(): void;
  resetKey?: unknown;
  surfaceRef: RefObject<TerminalSurfaceHandle | null>;
}) {
  const [searchText, setSearchText] = useState("");
  useEffect(() => setSearchText(""), [resetKey]);

  const search = (direction: "next" | "previous") => {
    const query = searchText.trim();
    if (!query) return;
    surfaceRef.current?.[direction === "next" ? "findNext" : "findPrevious"](query);
  };
  const clear = () => {
    surfaceRef.current?.clear();
    onClear();
  };

  return (
    <>
      <label className={className}>
        <span aria-hidden="true">⌕</span>
        <input
          aria-label="搜索终端输出"
          onChange={(event) => setSearchText(event.target.value)}
          onKeyDown={(event) => { if (event.key === "Enter") search("next"); }}
          placeholder="搜索终端输出"
          value={searchText}
        />
      </label>
      <button aria-label="上一个匹配项" disabled={!searchText.trim()} onClick={() => search("previous")} type="button">↑</button>
      <button aria-label="下一个匹配项" disabled={!searchText.trim()} onClick={() => search("next")} type="button">↓</button>
      <button onClick={clear} type="button">清屏</button>
    </>
  );
}
