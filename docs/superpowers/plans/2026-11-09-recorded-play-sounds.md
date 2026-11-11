# Recorded game sounds

Use project recordings according to sounds/sounds.xml: rollingDice.au for roll, move.au for checker movement and bear-off (no dedicated bear-off source exists), hit22.wav for hits. Decode AU mu-law to browser-supported PCM WAV without synthesis, time/pitch changes, or altering originals. Include small converted assets in the frontend and document a reproducible conversion command. Preserve enabled/mute preferences, gesture unlock, per-frame synchronization and silent load/reload behavior.

- [x] Prove the current browser plays synthetic durations instead of the provided recordings with an updated audio regression.
- [x] Convert the three selected recordings, use bundled asset URLs with cached asynchronous Web Audio decoding, preserve cancellation after fetch/decode, and remove synthesis.
- [x] Verify actual decoded files/durations, recording provenance, mute/cancel/failure handling, existing audio/animation paths, units and production build.
- [x] Publish tested frontend, verify live playback/files and unchanged saved data, clean owned test resources; keep ongoing changes uncommitted.


Observed: supplied three recordings were probe-played using afplay. Decoded PCM bytes from originals and converted WAVs compare identical via FFmpeg; Python WAV checks cover original channels, rates, frame counts and non-silence. Regression initially detected old synthetic0.14s movement versus recorded0.41s; replacement uses imported audio assets and no synthesis. 312 frontend unit tests, 18 existing animation/audio browser cases, 11 expanded audio cases (including missing asset retry and mute during decode) and two native GNUbg1.08.003 color/direction/profile cases passed. Scoped asynchronous cache/cancellation review approved with no findings. Production build, format/diff and Python compile checks passed. Tested frontend published; live verification follows. Backend unchanged.


Live verification: all 11 audio cases passed against the activated8787 bundle, including actual decoded recording durations and mute/retry behavior. Served WAV bytes match staged files; hit WAV is bundled inline by Vite. Saved settings and position/review counts are unchanged. During verification, a real existing user match advanced natively from revision13 to17 and game_over; its existing history prefix is preserved, with no matches removed. This is live gameplay, not a test write (browser test session IDs are isolated/mocked). No rollback of user moves was performed. Backend remains PID82754. Cleaned only owned5194/8991 services and the exact disposable recordedsounds database. Source remains uncommitted. Evidence: /Users/hendrik/Library/Caches/gnubg-update/bgtutor-recorded-sounds-verification/.
