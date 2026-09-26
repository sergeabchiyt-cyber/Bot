import os
import sys
import tempfile
import unittest
import numpy as np

curr_dir = os.path.dirname(os.path.abspath(__file__))
parent_dir = os.path.dirname(curr_dir)
if parent_dir not in sys.path:
    sys.path.insert(0, parent_dir)

from rill_learner import OnlineLearner


class TestOnlineLearner(unittest.TestCase):
    def setUp(self):
        self.temp_file = tempfile.NamedTemporaryFile(delete=False, suffix=".json")
        self.temp_file.close()
        self.learner = OnlineLearner(learning_rate=0.1, state_file=self.temp_file.name)

    def tearDown(self):
        if os.path.exists(self.temp_file.name):
            os.remove(self.temp_file.name)

    def test_single_update(self):
        features = [1.0, 0.5, -0.2]
        res = self.learner.update(features, 1.0)
        self.assertIn("loss", res)
        self.assertIn("prediction", res)
        self.assertEqual(res["target"], 1.0)
        self.assertEqual(res["total_samples"], 1)

    def test_learning_convergence(self):
        # A positive feature reliably associated with win (1.0)
        features = [2.0, 1.5]
        for _ in range(40):
            self.learner.update(features, 1.0)
        prob = self.learner.predict_proba(features)
        self.assertGreater(prob, 0.75)
        self.assertEqual(self.learner.predict(features), 1)

    def test_dimension_expansion(self):
        self.learner.update([1.0, 2.0], 1.0)
        self.assertEqual(len(self.learner.weights), 2)
        # Next trade has 4 features
        self.learner.update([1.0, 2.0, 3.0, 4.0], 0.0)
        self.assertEqual(len(self.learner.weights), 4)

    def test_persistence_save_load(self):
        features = [0.8, -1.2]
        for _ in range(10):
            self.learner.update(features, 1.0)
        prob1 = self.learner.predict_proba(features)
        self.learner.save()

        # Create new instance pointing to same file
        reloaded = OnlineLearner(state_file=self.temp_file.name)
        prob2 = reloaded.predict_proba(features)
        self.assertAlmostEqual(prob1, prob2, places=5)
        self.assertEqual(reloaded.total_samples, 10)


if __name__ == "__main__":
    unittest.main()
