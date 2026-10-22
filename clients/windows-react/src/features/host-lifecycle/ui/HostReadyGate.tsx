import type {
  HostDiagnosticEntry,
  HostLifecycleSnapshot,
} from "../domain/hostLifecycle";
import { useGeneralSettingsStore } from "../../general-settings/application/generalSettingsStore";
import { interfaceText } from "../../../shared/interfaceLocale";

interface HostReadyGateProps {
  snapshot: HostLifecycleSnapshot;
  diagnostics?: HostDiagnosticEntry[];
  onRetry(): void;
}

const workingPhases = new Set([
  "launching",
  "handshaking",
  "synchronizing",
  "restarting",
]);

export function HostReadyGate({
  snapshot,
  diagnostics = [],
  onRetry,
}: HostReadyGateProps) {
  const language = useGeneralSettingsStore((state) => state.settings.interfaceLanguage);
  const text = (key: Parameters<typeof interfaceText>[1]) => interfaceText(language, key);
  if (snapshot.ready) {
    return (
      <div className="ready-gate">
        <section className="gate-card">
          <div className="gate-icon" aria-hidden="true">✓</div>
          <h2>{text("localServiceReady")}</h2>
          <p>{text("localServiceReadyDetail")}</p>
        </section>
      </div>
    );
  }

  const working = workingPhases.has(snapshot.phase);
  return (
    <div className="ready-gate">
      <section className="gate-card" aria-live="polite">
        <div className="gate-icon" aria-hidden="true">
          {working ? <span className="spinner" /> : "⌁"}
        </div>
        <h2>{working ? text("preparingLocalService") : text("localServiceUnavailable")}</h2>
        <p>{snapshot.detail}</p>
        {snapshot.retryable && !working ? (
          <div className="gate-actions">
            <button className="primary-button" type="button" onClick={onRetry}>{text("checkAgain")}</button>
          </div>
        ) : null}
        {diagnostics.length > 0 && !working ? (
          <details className="diagnostics-panel">
            <summary>{text("diagnostics")}</summary>
            <ol>
              {diagnostics.slice(-5).map((entry) => (
                <li key={entry.sequence}>
                  <code>{entry.event}</code>
                  <span>{entry.outcome}</span>
                </li>
              ))}
            </ol>
          </details>
        ) : null}
      </section>
    </div>
  );
}
