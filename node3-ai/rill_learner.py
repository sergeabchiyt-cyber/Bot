"""
Online Logistic Regression Learner (rill-ml Python Fallback)
============================================================
Provides online learning from closed trade events. Updates after every trade
outcome received from Node 1 over the persistent WebSocket connection.

Contracts:
- Inbound frame: {"type": "learn", "features": [...], "target": 0.0 | 1.0}
- Feature vector: floats (e.g., orderflow delta, sentiment score, proximity to PoC)
- Target: 1.0 (profitable trade / win), 0.0 (loss)
"""

import json
import logging
import math
import os
from typing import Dict, List, Optional, Union

import numpy as np

logger = logging.getLogger("node3.learner")
if not logger.handlers:
    logging.basicConfig(level=logging.INFO)


class OnlineLearner:
    def __init__(
        self,
        learning_rate: float = 0.05,
        l2_reg: float = 0.001,
        state_file: Optional[str] = None,
    ):
        self.learning_rate_0 = learning_rate
        self.l2_reg = l2_reg
        self.weights: np.ndarray = np.array([], dtype=np.float64)
        self.bias: float = 0.0
        self.total_samples: int = 0
        self.win_samples: int = 0
        self.loss_samples: int = 0
        self.running_loss: float = 0.0

        if state_file is None:
            curr_dir = os.path.dirname(os.path.abspath(__file__))
            state_file = os.path.join(curr_dir, "learner_state.json")
        self.state_file = state_file

        self.load()

    def _ensure_dimension(self, n_features: int):
        """Dynamically initializes or extends weights to match feature dimension."""
        curr_dim = len(self.weights)
        if curr_dim < n_features:
            new_weights = np.zeros(n_features, dtype=np.float64)
            if curr_dim > 0:
                new_weights[:curr_dim] = self.weights
            self.weights = new_weights

    @staticmethod
    def _sigmoid(z: float) -> float:
        """Numerically stable sigmoid function."""
        if z >= 0:
            ez = math.exp(-z)
            return 1.0 / (1.0 + ez)
        else:
            ez = math.exp(z)
            return ez / (1.0 + ez)

    def predict_proba(self, features: Union[List[float], np.ndarray]) -> float:
        """
        Predicts win probability P(win=1 | features).
        """
        x = np.asarray(features, dtype=np.float64)
        if len(self.weights) == 0:
            return 0.5

        if len(x) > len(self.weights):
            self._ensure_dimension(len(x))
        elif len(x) < len(self.weights):
            x = np.pad(x, (0, len(self.weights) - len(x)), mode="constant")

        z = float(np.dot(self.weights, x) + self.bias)
        return self._sigmoid(z)

    def predict(
        self, features: Union[List[float], np.ndarray], threshold: float = 0.5
    ) -> int:
        """Returns binary prediction 1 (win) or 0 (loss)."""
        return 1 if self.predict_proba(features) >= threshold else 0

    def update(
        self, features: Union[List[float], np.ndarray], target: float
    ) -> Dict[str, float]:
        """
        Updates logistic regression model via online Stochastic Gradient Descent.

        Args:
            features: list or array of trade features
            target: 0.0 (loss) or 1.0 (win)

        Returns:
            dict with updated metrics: loss, prediction, target, sample count
        """
        x = np.asarray(features, dtype=np.float64)
        n = len(x)
        self._ensure_dimension(n)

        p = self.predict_proba(x)
        y = float(target)
        error = p - y

        # Learning rate schedule: eta_t = eta_0 / sqrt(1 + t)
        self.total_samples += 1
        eta = self.learning_rate_0 / math.sqrt(1.0 + 0.01 * self.total_samples)

        # SGD gradient step with L2 weight decay:
        # grad_w = error * x + lambda * w
        # grad_b = error
        grad_w = error * x + self.l2_reg * self.weights
        self.weights -= eta * grad_w
        self.bias -= eta * error

        # Track metrics
        eps = 1e-12
        p_clipped = min(max(p, eps), 1.0 - eps)
        loss = - (y * math.log(p_clipped) + (1.0 - y) * math.log(1.0 - p_clipped))

        if y >= 0.5:
            self.win_samples += 1
        else:
            self.loss_samples += 1

        # Exponential moving average of loss
        alpha = 0.05
        if self.running_loss == 0.0:
            self.running_loss = loss
        else:
            self.running_loss = (1.0 - alpha) * self.running_loss + alpha * loss

        logger.info(
            "Trade learnt: target=%.1f, pred=%.3f, loss=%.4f, total=%d",
            y,
            p,
            loss,
            self.total_samples,
        )

        # Periodically persist state
        if self.total_samples % 5 == 0 or self.total_samples <= 10:
            self.save()

        return {
            "loss": round(loss, 4),
            "prediction": round(p, 4),
            "target": y,
            "total_samples": self.total_samples,
            "win_rate": round(self.win_samples / max(1, self.total_samples), 4),
        }

    def stats(self) -> dict:
        """Returns summary stats of the online learner."""
        return {
            "total_samples": self.total_samples,
            "win_samples": self.win_samples,
            "loss_samples": self.loss_samples,
            "win_rate": round(self.win_samples / max(1, self.total_samples), 4),
            "running_loss": round(self.running_loss, 4),
            "feature_dim": len(self.weights),
            "bias": round(float(self.bias), 4),
        }

    def save(self, filepath: Optional[str] = None):
        """Persists model parameters and statistics to disk."""
        target_path = filepath or self.state_file
        try:
            data = {
                "weights": self.weights.tolist(),
                "bias": float(self.bias),
                "total_samples": self.total_samples,
                "win_samples": self.win_samples,
                "loss_samples": self.loss_samples,
                "running_loss": float(self.running_loss),
            }
            tmp_path = f"{target_path}.tmp"
            with open(tmp_path, "w") as f:
                json.dump(data, f, indent=2)
            os.replace(tmp_path, target_path)
            logger.debug("Learner state saved to %s", target_path)
        except Exception as e:
            logger.error("Failed to save learner state: %s", e)

    def load(self, filepath: Optional[str] = None):
        """Loads model parameters and statistics from disk if available."""
        target_path = filepath or self.state_file
        if not os.path.isfile(target_path) or os.path.getsize(target_path) == 0:
            return
        try:
            with open(target_path, "r") as f:
                data = json.load(f)
            self.weights = np.array(data.get("weights", []), dtype=np.float64)
            self.bias = float(data.get("bias", 0.0))
            self.total_samples = int(data.get("total_samples", 0))
            self.win_samples = int(data.get("win_samples", 0))
            self.loss_samples = int(data.get("loss_samples", 0))
            self.running_loss = float(data.get("running_loss", 0.0))
            logger.info(
                "Loaded learner state from %s (samples=%d, dim=%d)",
                target_path,
                self.total_samples,
                len(self.weights),
            )
        except Exception as e:
            logger.warning("Failed to load learner state: %s", e)
