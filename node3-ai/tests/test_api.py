import os
import sys
import unittest

curr_dir = os.path.dirname(os.path.abspath(__file__))
parent_dir = os.path.dirname(curr_dir)
if parent_dir not in sys.path:
    sys.path.insert(0, parent_dir)

from main import (
    health,
    score_sentiment,
    learn_trade,
    stats,
    SentimentRequest,
    LearnRequest,
)


class TestFastAPIEndpoints(unittest.TestCase):
    def test_health_endpoint(self):
        h = health()
        self.assertEqual(h["status"], "ok")
        self.assertIn("rss_mb", h)
        self.assertIn("models_loaded", h)
        self.assertIn("uptime", h)

    def test_sentiment_endpoint(self):
        req = SentimentRequest(
            text="The Federal Reserve raised rates by 75 basis points."
        )
        res = score_sentiment(req)
        self.assertEqual(res["hawkish"], 0.72)
        self.assertIn("confidence", res)

    def test_learn_endpoint(self):
        req = LearnRequest(features=[1.2, -0.5, 0.3], target=1.0)
        res = learn_trade(req)
        self.assertIn("loss", res)
        self.assertIn("prediction", res)
        self.assertEqual(res["target"], 1.0)

    def test_stats_endpoint(self):
        st = stats()
        self.assertIn("learner", st)
        self.assertIn("rss_mb", st)
        self.assertIn("uptime_sec", st)


if __name__ == "__main__":
    unittest.main()
