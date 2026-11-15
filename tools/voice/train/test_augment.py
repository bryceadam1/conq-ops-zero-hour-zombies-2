"""Tests for augment.py on synthetic recordings (no downloads needed):

    venv/bin/python -m unittest tools/voice/train/test_augment.py
"""

import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np
import soundfile as sf

sys.path.insert(0, str(Path(__file__).parent))
import augment as A  # noqa: E402

SR = A.SR


def tone(seconds, hz=220, amp=0.3):
    t = np.arange(int(seconds * SR)) / SR
    return (amp * np.sin(2 * np.pi * hz * t)).astype(np.float32)


def utterance():
    """0.3 s of silence, 1.4 s of 'speech', 0.3 s of silence."""
    rng = np.random.default_rng(1)
    speech = tone(1.4) * (0.5 + 0.5 * rng.random(int(1.4 * SR))).astype(np.float32)
    return np.concatenate([np.zeros(int(0.3 * SR), np.float32), speech, np.zeros(int(0.3 * SR), np.float32)])


class Fixture(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        root = Path(cls.tmp.name)
        rng = np.random.default_rng(0)
        files = {}
        for i in range(10):
            files[f"musan/noise/free-sound/n{i}.wav"] = rng.normal(0, 0.1, 3 * SR)
            files[f"musan/music/fma/m{i}.wav"] = tone(4, 330 + i)
            files[f"musan/speech/librivox/s{i}.wav"] = tone(2, 150 + 10 * i) * rng.random(2 * SR)
            for room in ("smallroom", "mediumroom", "largeroom"):
                decay = np.exp(-np.arange(SR // 4) / (SR * 0.03 * (1 + i % 3)))
                rir = rng.normal(0, 1, SR // 4) * decay
                rir[: 40 + i] = 0
                rir[40 + i] = 3.0
                files[f"RIRS_NOISES/simulated_rirs/{room}/Room{i:03d}/r.wav"] = rir / np.abs(rir).max()
        for rel, x in files.items():
            (root / rel).parent.mkdir(parents=True, exist_ok=True)
            sf.write(root / rel, np.asarray(x, np.float32), SR)
        cls.root = root
        cls.pools = A.Pools(root, A.build_pools(root))

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()


class TestAugment(Fixture):
    def test_the_same_clip_and_epoch_augment_the_same_way(self):
        x = utterance()
        a, ra = A.augment(x, A.rng_for("c1", 3), self.pools, force=True)
        b, rb = A.augment(x, A.rng_for("c1", 3), self.pools, force=True)
        self.assertEqual(ra, rb)
        np.testing.assert_array_equal(a, b)

    def test_epochs_draw_different_takes(self):
        x = utterance()
        recipes = {str(A.augment(x, A.rng_for("c1", e), self.pools, force=True)[1]) for e in range(8)}
        self.assertGreater(len(recipes), 4)

    def test_a_fifth_stay_clean(self):
        x = utterance()[: SR // 2]
        clean = sum(A.augment(x, A.rng_for(f"c{i}", 0), self.pools)[1] == {"clean": True} for i in range(400))
        self.assertAlmostEqual(clean / 400, A.CLEAN, delta=0.06)

    def test_every_take_is_finite_and_within_full_scale(self):
        x = utterance()
        for i in range(40):
            y, recipe = A.augment(x, A.rng_for(f"c{i}", 0), self.pools, force=True)
            self.assertTrue(np.all(np.isfinite(y)), recipe)
            self.assertLessEqual(float(np.max(np.abs(y))), 1.0 + 1e-6, recipe)
            self.assertGreater(len(y), SR // 2, recipe)

    def test_noise_lands_at_its_snr_over_the_speech(self):
        x = utterance()
        noise = np.random.default_rng(5).normal(0, 0.2, len(x)).astype(np.float32)
        for snr in (0, 10, 20):
            y = A.add_noise(x, noise, snr)
            got = A.db(A.speech_power(x) / np.mean((y - x) ** 2))
            self.assertAlmostEqual(got, snr, delta=0.01)

    def test_a_room_keeps_the_voice_aligned_and_as_loud(self):
        x = utterance()
        rir = np.zeros(SR // 4, np.float32)
        rir[100] = 1.0
        rir[101:] = 0.05 * np.random.default_rng(2).normal(size=SR // 4 - 101) * np.exp(-np.arange(SR // 4 - 101) / 800)
        y = A.reverberate(x, rir)
        self.assertEqual(len(y), len(x))
        self.assertEqual(A.speech_span(y)[0], A.speech_span(x)[0])
        self.assertAlmostEqual(A.db(A.speech_power(y)), A.db(A.speech_power(x)), delta=0.01)

    def test_push_to_talk_cuts_to_the_speech_then_pads(self):
        x = utterance()
        a, b = A.speech_span(x)
        for i in range(20):
            y, r = A.push_to_talk(x, A.rng_for(f"p{i}", 0))
            cut_a, cut_b, lead, tail = r["ptt"]
            self.assertEqual(len(y), lead + (len(x) - cut_a - cut_b) + tail)
            self.assertGreaterEqual(cut_a, a)
            self.assertLessEqual(cut_a - a, int(0.05 * SR))
            self.assertLessEqual(cut_b - (len(x) - b), int(0.05 * SR))

    def test_a_microphone_cuts_the_band_edges(self):
        rng = np.random.default_rng(3)
        gain, r = A.mic_response(4097, rng)
        f = np.fft.rfftfreq(8192, 1 / SR)
        self.assertLess(gain[np.searchsorted(f, 30)], 0.5)
        self.assertLess(gain[-1], 0.5)
        self.assertGreater(gain[np.searchsorted(f, 1000)], 0.5)

    def test_opus_keeps_the_length_and_the_signal(self):
        x = utterance()
        y = A.opus(x, 16)
        self.assertEqual(len(y), len(x))
        # Opus has its own delay; the voice should still be there, near where it was.
        corr = np.correlate(y[:SR], x[:SR], "full")
        self.assertLess(abs(int(np.argmax(corr)) - (SR - 1)), 400)
        self.assertGreater(A.speech_power(y), 0.1 * A.speech_power(x))

    def test_the_test_pools_hold_back_a_fifth_and_share_nothing(self):
        for kind in ("noise", "music", "babble", "smallroom"):
            train = {r[0] for r in self.pools.by[(kind, "train")]}
            test = {r[0] for r in self.pools.by[(kind, "test")]}
            self.assertFalse(train & test)
        rooms = {r[0].rsplit("/", 2)[1]: r[2] for split in ("train", "test") for r in self.pools.by[("smallroom", split)]}
        for rows in (self.pools.by[("smallroom", "train")], self.pools.by[("smallroom", "test")]):
            for r in rows:
                self.assertEqual(r[2], rooms[r[0].rsplit("/", 2)[1]])
        names = [f"x/{i}.wav" for i in range(2000)]
        self.assertAlmostEqual(sum(A.split_of(n) == "test" for n in names) / 2000, 0.2, delta=0.03)

    def test_spec_augment_never_masks_the_padded_silence(self):
        mel = np.random.default_rng(4).normal(size=(80, 3000)).astype(np.float32)
        for i in range(20):
            out = A.spec_augment(mel, np.random.default_rng(i), frames=200)
            # Past the speech, only frequency masks change anything, and they cover whole rows.
            past = out[:, 200:] != mel[:, 200:]
            touched = past.any(1)
            self.assertTrue(np.all(past[touched].all(1)), i)

    def test_word_error(self):
        self.assertEqual(A.wer("Rangers, move to the bridge.", "rangers move to the bridge"), (0, 5))
        self.assertEqual(A.wer("attack the torvel", "attack the travel"), (1, 3))


if __name__ == "__main__":
    unittest.main()
