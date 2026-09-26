import os
import sys
import unittest

# Ensure node3-ai is in python path
curr_dir = os.path.dirname(os.path.abspath(__file__))
parent_dir = os.path.dirname(curr_dir)
if parent_dir not in sys.path:
    sys.path.insert(0, parent_dir)

from finbert_service import FinBERTService


class TestFinBERTService(unittest.TestCase):
    def setUp(self):
        self.fb = FinBERTService()

    def test_hawkish_statement(self):
        text = "The Federal Reserve raised rates by 75 basis points."
        res = self.fb.predict(text)
        self.assertIn("hawkish", res)
        self.assertIn("dovish", res)
        self.assertIn("neutral", res)
        self.assertIn("confidence", res)
        self.assertGreater(res["hawkish"], res["dovish"])
        self.assertAlmostEqual(res["hawkish"], 0.72, delta=0.05)

    def test_dovish_statement(self):
        text = "The Fed signaled it would slow the pace of rate hikes."
        res = self.fb.predict(text)
        self.assertGreater(res["dovish"], res["hawkish"])

    def test_tight_labor_statement(self):
        text = "The labor market remains tight and inflation is elevated."
        res = self.fb.predict(text)
        self.assertGreater(res["hawkish"], res["dovish"])

    def test_empty_string(self):
        res = self.fb.predict("")
        self.assertEqual(res["neutral"], 1.0)
        self.assertEqual(res["hawkish"], 0.0)
        self.assertEqual(res["dovish"], 0.0)

    def test_probabilities_sum(self):
        text = "Economic conditions remain uncertain with mixed employment signals."
        res = self.fb.predict(text)
        total = res["hawkish"] + res["dovish"] + res["neutral"]
        self.assertAlmostEqual(total, 1.0, places=2)


if __name__ == "__main__":
    unittest.main()
