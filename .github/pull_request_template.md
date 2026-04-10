## Summary

<!-- Briefly describe what changed and why. -->

## Risk & Validation

- [ ] I evaluated regression risk and listed key affected modules.
- [ ] I ran relevant local verification (tests/build/type-check/scripts) and confirmed results.

## API Contract Checklist

- [ ] This PR does not change API behavior.
- [ ] If API behavior changed, I updated OpenAPI contract files under `.github/api-contract/`.
- [ ] If endpoint topology changed, I updated and committed path/surface baselines.
- [ ] I checked API owner mapping in `.github/api-contract/OWNERSHIP_MAP.md`.
- [ ] I reviewed OpenAPI owner report / change-summary artifact when fragment files changed.
- [ ] If emergency-only waiver is used, I added expiry + approver + reason in waiver file and created follow-up removal task.
