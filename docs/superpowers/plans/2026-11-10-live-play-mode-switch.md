# Live play modes and startup restoration

User requested fixing startup422 and changing mode during an ongoing match.

- [x] Reproduce automatic analysis restoration of a live Competition source and cover startup routing plus cancelled restoration with browser regressions.
- [x] Restore restricted selections into their linked play session using accessible source metadata, preserving the existing newer-navigation fence and genuine error handling.
- [x] Add versioned `/play/sessions/{id}/mode` mutation preserving native checkpoint, pending attempt history, frozen engine/depth/threshold and explicit commit semantics. Persist sticky learning use; mixed first-attempt ratings use actual native totals plus original-attempt corrections.
- [x] Add a prominent current mode badge and accessible in-game mode selector. Keep partial checker steps; freeze switching during requests/animations. Pending Competition choice offers neutral explicit confirmation.
- [x] Verify stale mode writes, feedback changes, restart, concurrency, mixed statistics, normal startup/home navigation, native GNUbg flows, type generation, builds and review. Restart local app after verification.

Verified: 27 native backend play tests, 317 frontend unit tests, 28 focused browser cases and 5 real GNUbg browser lifecycles passed. Production build, formatting, type checks, clippy and diff checks passed. Reviewed frontend navigation/cache races. Restarted and smoke-checked the local app at 127.0.0.1:8787; removed the disposable native-test service/database.
