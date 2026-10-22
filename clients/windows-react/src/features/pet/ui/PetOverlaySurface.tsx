import { useEffect, useMemo, useState } from "react";
import "./pet.css";

const columns = 8;
const idleFrameCount = 7;

export function PetOverlaySurface() {
  const [frame, setFrame] = useState(0);
  const [assetFailed, setAssetFailed] = useState(false);
  const assetUrl = useMemo(() => (
    navigator.userAgent.includes("Windows")
      ? "http://chatos-pet.localhost/fengtuan-atlas"
      : "chatos-pet://localhost/fengtuan-atlas"
  ), []);

  useEffect(() => {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return undefined;
    const timer = window.setInterval(() => setFrame((current) => (current + 1) % idleFrameCount), 1_000);
    return () => window.clearInterval(timer);
  }, []);

  const x = `${(frame / (columns - 1)) * 100}%`;
  return (
    <main aria-label="ChatOS pet is idle" className="pet-overlay" data-tauri-drag-region>
      {assetFailed ? (
        <div aria-hidden="true" className="pet-overlay__fallback" data-tauri-drag-region>🦊</div>
      ) : (
        <img
          alt=""
          className="pet-overlay__preload"
          onError={() => setAssetFailed(true)}
          src={assetUrl}
        />
      )}
      {!assetFailed && <div aria-hidden="true" className="pet-overlay__sprite" data-tauri-drag-region style={{ backgroundImage: `url(${assetUrl})`, backgroundPosition: `${x} 0%` }} />}
    </main>
  );
}
