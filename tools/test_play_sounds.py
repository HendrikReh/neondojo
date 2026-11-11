import unittest
import wave
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "web/src/assets/play-sounds"


class RecordedSoundsTest(unittest.TestCase):
    def test_recorded_pcm_format_duration_and_non_silent_samples(self):
        for name, rate, frames in [
            ("checker-move", 8012, 3247),
            ("rolling-dice", 8012, 2204),
            ("checker-hit", 22000, 1123),
            ("cube-double", 8012, 1238),
        ]:
            with self.subTest(recording=name), wave.open(str(ROOT / (name + ".wav")), "rb") as audio:
                self.assertEqual((audio.getnchannels(), audio.getsampwidth()), (1, 2))
                self.assertEqual((audio.getframerate(), audio.getnframes()), (rate, frames))
                samples = struct.unpack("<" + "h" * frames, audio.readframes(frames))
                self.assertGreater(sum((n / 32768) ** 2 for n in samples) / frames, .000025)


if __name__ == "__main__":
    unittest.main()
