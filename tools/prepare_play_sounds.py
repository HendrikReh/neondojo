#!/usr/bin/env python3
"""Prepare browser WAVs from the supplied sounds/ recordings; requires FFmpeg."""
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "web/src/assets/play-sounds"
RECORDINGS = {
    "move.au": "checker-move.wav",
    "rollingDice.au": "rolling-dice.wav",
    "hit22.wav": "checker-hit.wav",
    "double.au": "cube-double.wav",
}


def main():
    ffmpeg = shutil.which("ffmpeg")
    if not ffmpeg:
        raise SystemExit("Install FFmpeg to regenerate the recorded sound assets.")
    OUTPUT.mkdir(parents=True, exist_ok=True)
    for source, destination in RECORDINGS.items():
        subprocess.run([
            ffmpeg, "-hide_banner", "-loglevel", "error", "-y", "-i",
            str(ROOT / "sounds" / source), "-map_metadata", "-1",
            "-fflags", "+bitexact", "-flags:a", "+bitexact", "-c:a", "pcm_s16le",
            str(OUTPUT / destination),
        ], check=True)
        print(f"{source} -> {destination}")


if __name__ == "__main__":
    main()
