# OpenAPI Gate Emergency Waivers

This folder stores temporary exceptions for OpenAPI required gate failures.

## Rules

1. Waivers are for emergency-only changes.
2. Waivers must include an explicit expiration timestamp (UTC).
3. Waiver expiry must stay within policy max lifetime (default: 24h).
4. Waivers must include approver and reason for auditability.
5. Waivers should be removed immediately after contracts are brought back to policy.

## How To Use

1. Copy `openapi_gate_waiver.example.env` to `openapi_gate_waiver.env`.
2. Fill all required fields with real values.
3. Keep expiration short (for example: <= 24h, enforced by gate policy).
4. Open a follow-up task to remove the waiver.

When `openapi_gate_waiver.env` is absent, disabled, expired, or malformed, CI gate fallback is strict.
