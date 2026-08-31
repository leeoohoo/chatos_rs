# ChatOS NetworkGuard Driver

This is the privileged WDK component for Controlled networking. It exposes only a SYSTEM/Administrators control device and owns the ChatOS WFP provider, sublayer, callouts and per-AppContainer filters.

Build requirements: Visual Studio 2022, Windows 11 SDK and the matching Windows Driver Kit. Production packages must be attestation/WHQL signed; test-signing is accepted only by the isolated CI VM.

The wire structs in `NetworkGuardProtocol.h` must stay byte-for-byte aligned with `WindowsNetworkGuardNativeController`. The driver fails closed: an expired or unknown lease blocks the classified connection; it never falls back to permit.
