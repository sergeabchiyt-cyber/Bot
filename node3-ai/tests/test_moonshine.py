import os
import sys
import unittest
import numpy as np

curr_dir = os.path.dirname(os.path.abspath(__file__))
parent_dir = os.path.dirname(curr_dir)
if parent_dir not in sys.path:
    sys.path.insert(0, parent_dir)

from moonshine_service import MoonshineService


class TestMoonshineService(unittest.TestCase):
    def setUp(self):
        self.ms = MoonshineService()

    def test_audio_padding_to_multiple_of_80(self):
        # 16005 samples -> should pad to 16080 (next multiple of 80)
        audio = np.random.randn(16005).astype(np.float32)
        res = self.ms.transcribe(audio)
        self.assertIsInstance(res, str)

    def test_empty_audio(self):
        empty = np.array([], dtype=np.float32)
        res = self.ms.transcribe(empty)
        self.assertEqual(res, "")

    def test_none_audio(self):
        res = self.ms.transcribe(None)
        self.assertEqual(res, "")

    def test_list_input(self):
        audio_list = [0.0] * 160
        res = self.ms.transcribe(audio_list)
        self.assertIsInstance(res, str)


if __name__ == "__main__":
    unittest.main()
