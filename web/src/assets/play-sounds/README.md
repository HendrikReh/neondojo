# Recorded game sounds

These PCM WAV files are decoded from the user-supplied project recordings in `/sounds`, following that directory's `sounds.xml` mapping:

- `rolling-dice.wav` ← `sounds/rollingDice.au` (`rollDice`)
- `checker-move.wav` ← `sounds/move.au` (`move`), also used for bear-off because no dedicated recording was supplied
- `cube-double.wav` ← `sounds/double.au` (`double`), used for a new bot cube offer
- `checker-hit.wav` ← `sounds/hit22.wav` (`hitChecker`)

Regenerate with `python3 tools/prepare_play_sounds.py` (FFmpeg required only for regeneration). Original recordings remain untouched. Sample rates, channels, duration and decoded samples are preserved; no synthesized sound, filtering, pitch change, normalization or time stretching is added. The frontend bundles these small assets, decodes them once per audio context and applies a fixed playback gain.
