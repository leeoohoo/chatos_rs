import type { ConnectionTestResult } from "../../types/models";

interface Props {
  testResult: ConnectionTestResult;
}

export function ConnectionTestResultCard({ testResult }: Props) {
  return (
    <div className="test-result">
      <p className="test-result__summary">
        Last test: {testResult.ok ? "OK" : "Failed"} · {testResult.latency_ms}ms
      </p>

      {testResult.server_version ? (
        <p className="test-result__line">version: {testResult.server_version}</p>
      ) : null}

      {!testResult.ok ? (
        <>
          {testResult.error_code ? (
            <p className="test-result__line">
              code: <code>{testResult.error_code}</code>
            </p>
          ) : null}
          {testResult.stage ? <p className="test-result__line">stage: {testResult.stage}</p> : null}
          {testResult.message ? (
            <p className="test-result__line test-result__error">{testResult.message}</p>
          ) : null}
        </>
      ) : null}

      {testResult.checks.length > 0 ? (
        <ul className="test-result__checks">
          {testResult.checks.map((check) => (
            <li key={check.stage}>
              <span>{check.stage}</span>
              <strong>{check.ok ? "ok" : "failed"}</strong>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
