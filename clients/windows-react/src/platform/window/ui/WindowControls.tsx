import type { WindowChromePort } from "../ports/WindowChromePort";

interface WindowControlsProps {
  windowChrome: WindowChromePort;
}

export function WindowControls({ windowChrome }: WindowControlsProps) {
  return (
    <div className="window-controls" onDoubleClick={(event) => event.stopPropagation()}>
      <button
        aria-label="关闭窗口"
        className="window-control window-control--close"
        type="button"
        onClick={() => void windowChrome.close()}
      />
      <button
        aria-label="最小化窗口"
        className="window-control window-control--minimize"
        type="button"
        onClick={() => void windowChrome.minimize()}
      />
      <button
        aria-label="最大化或还原窗口"
        className="window-control window-control--zoom"
        type="button"
        onClick={() => void windowChrome.toggleMaximize()}
      />
    </div>
  );
}
