"""
Node 3 FastAPI Service
======================
Exposes /health, /sentiment, /transcribe, and /learn endpoints for manual testing,
local diagnostics, and integration verification.
"""

import os
import sys
import time
from typing import Any, Dict, List, Optional

from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field

# Ensure local imports resolve
curr_dir = os.path.dirname(os.path.abspath(__file__))
if curr_dir not in sys.path:
    sys.path.insert(0, curr_dir)

from finbert_service import FinBERTService
from moonshine_service import MoonshineService
from rill_learner import OnlineLearner
from ws_client import get_rss_mb

app = FastAPI(
    title="XAUUSD Node 3 AI Service",
    description="Moonshine Streaming ASR + FinBERT Sentiment + Online Learner",
    version="1.0.0",
)

# Shared service singletons
start_time = time.time()
finbert_svc = FinBERTService()
moonshine_svc = MoonshineService()
learner_svc = OnlineLearner()


class SentimentRequest(BaseModel):
    text: str = Field(
        ...,
        description="Fed statement, speech transcript, or financial commentary",
        example="The Federal Reserve raised rates by 75 basis points.",
    )


class LearnRequest(BaseModel):
    features: List[float] = Field(
        ...,
        description="Feature vector from closed trade (delta, VP distance, sentiment, etc.)",
        example=[1.5, -0.4, 0.8],
    )
    target: float = Field(
        ...,
        description="Trade outcome: 1.0 (win/profit) or 0.0 (loss)",
        example=1.0,
    )


@app.get("/")
def root():
    return {
        "service": "XAUUSD Node 3 AI Service",
        "status": "online",
        "models": {
            "moonshine_onnx": moonshine_svc.is_onnx_loaded,
            "finbert_onnx": finbert_svc.is_onnx_loaded,
        },
        "uptime_sec": round(time.time() - start_time, 1),
    }


@app.get("/health")
def health():
    """Health endpoint returning RSS memory, loaded models, and learner status."""
    models_loaded = []
    if moonshine_svc.is_onnx_loaded:
        models_loaded.append("moonshine")
    else:
        models_loaded.append("moonshine_fallback")

    if finbert_svc.is_onnx_loaded:
        models_loaded.append("finbert")
    else:
        models_loaded.append("finbert_fallback")

    return {
        "status": "ok",
        "rss_mb": get_rss_mb(),
        "models_loaded": models_loaded,
        "uptime": round(time.time() - start_time, 1),
        "learner_samples": learner_svc.total_samples,
    }


@app.post("/sentiment")
def score_sentiment(payload: SentimentRequest):
    """
    Scores Fed speak text for Hawkish / Dovish / Neutral sentiment.
    Hawkish -> bearish gold
    Dovish  -> bullish gold
    """
    if not payload.text or not payload.text.strip():
        raise HTTPException(status_code=400, detail="Text field cannot be empty.")
    result = finbert_svc.predict(payload.text)
    return result


@app.post("/learn")
def learn_trade(payload: LearnRequest):
    """Updates the online logistic regression learner with a closed trade."""
    if payload.target not in (0.0, 1.0):
        # Allow continuous targets between 0 and 1 as well
        if not (0.0 <= payload.target <= 1.0):
            raise HTTPException(
                status_code=400, detail="Target must be between 0.0 and 1.0."
            )
    result = learner_svc.update(payload.features, payload.target)
    return result


@app.get("/stats")
def stats():
    """Returns learner parameters and running training stats."""
    return {
        "learner": learner_svc.stats(),
        "rss_mb": get_rss_mb(),
        "uptime_sec": round(time.time() - start_time, 1),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)
