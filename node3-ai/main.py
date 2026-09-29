"""
Node 3 FastAPI Service — UPGRADED for 16GB / 4vCPU / 400GB
==========================================================
Exposes /health, /sentiment, /transcribe, and /learn endpoints for manual testing,
local diagnostics, and integration verification. Upgraded tier reports FOMC-RoBERTa
medium-tier status and learner v2 stats.
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
    title="XAUUSD Node 3 AI Service — UPGRADED 16GB",
    description="Moonshine Streaming (medium/small) + FOMC-RoBERTa 355M / FinBERT + Online Adam Learner v2",
    version="2.0.0",
)

# Shared service singletons — lazy init but keep global for reuse
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


class TranscribeRequest(BaseModel):
    pcm_base64: Optional[str] = Field(None, description="Base64 float32 16kHz PCM")
    text_fallback: Optional[str] = Field(None, description="If no audio, just echo")


@app.get("/")
def root():
    return {
        "service": "XAUUSD Node 3 AI Service — UPGRADED",
        "status": "online",
        "tier": "16gb_4vcpu_400gb",
        "models": {
            "moonshine_onnx": moonshine_svc.is_onnx_loaded,
            "moonshine_tier": getattr(moonshine_svc, 'model_tier', 'unknown'),
            "moonshine_dir": getattr(moonshine_svc, 'model_dir', ''),
            "vad_loaded": bool(getattr(moonshine_svc, 'vad_session', None)),
            "finbert_onnx": finbert_svc.is_onnx_loaded,
            "finbert_tier": getattr(finbert_svc, 'label_mode', 'unknown'),
            "finbert_model": getattr(finbert_svc, 'model_name', ''),
            "finbert_dir": getattr(finbert_svc, 'model_dir', ''),
            "learner_version": getattr(learner_svc, 'version', 1),
        },
        "uptime_sec": round(time.time() - start_time, 1),
        "rss_mb": get_rss_mb(),
    }


@app.get("/health")
def health():
    models_loaded = []
    if moonshine_svc.is_onnx_loaded:
        models_loaded.append(f"moonshine_{getattr(moonshine_svc,'model_tier','')}")
    else:
        models_loaded.append("moonshine_fallback")
    if finbert_svc.is_onnx_loaded:
        models_loaded.append(f"{finbert_svc.label_mode}:{finbert_svc.model_name}")
    else:
        models_loaded.append("finbert_heuristic")
    models_loaded.append(f"learner_v{getattr(learner_svc,'version',1)}")

    return {
        "status": "ok",
        "tier": "upgraded_16gb",
        "rss_mb": get_rss_mb(),
        "models_loaded": models_loaded,
        "uptime": round(time.time() - start_time, 1),
        "learner_samples": learner_svc.total_samples,
        "learner_acc": round(getattr(learner_svc,'running_acc',0),4),
        "vad": bool(getattr(moonshine_svc,'vad_session',None)),
    }


@app.post("/sentiment")
def score_sentiment(payload: SentimentRequest):
    if not payload.text or not payload.text.strip():
        raise HTTPException(status_code=400, detail="Text field cannot be empty.")
    result = finbert_svc.predict(payload.text)
    return result


@app.post("/transcribe")
def transcribe(payload: Dict[str, Any]):
    """
    Accepts {"audio_base64": "..."} or raw {"data": base64} or plain debug.
    Decodes float32 PCM and runs Moonshine.
    """
    import base64
    import numpy as np
    b64 = payload.get("audio_base64") or payload.get("data") or payload.get("pcm_base64")
    if not b64:
        return {"transcript": "", "note": "no audio provided"}
    try:
        raw = base64.b64decode(b64)
        arr = np.frombuffer(raw, dtype=np.float32)
        if len(arr) == 0:
            arr = np.frombuffer(raw, dtype=np.int16).astype(np.float32)/32768.0
        txt = moonshine_svc.transcribe(arr)
        return {"transcript": txt, "tier": getattr(moonshine_svc,'model_tier','')}
    except Exception as e:
        raise HTTPException(status_code=400, detail=str(e))


@app.post("/learn")
def learn_trade(payload: LearnRequest):
    if not (0.0 <= payload.target <= 1.0):
        raise HTTPException(status_code=400, detail="Target must be between 0.0 and 1.0.")
    result = learner_svc.update(payload.features, payload.target)
    return result


@app.post("/predict")
def predict_proba(payload: LearnRequest):
    """Inference only — no learning step."""
    p = learner_svc.predict_proba(payload.features)
    return {"proba": round(p, 4), "threshold": learner_svc.suggested_threshold(), "pred": 1 if p >= 0.5 else 0}


@app.get("/stats")
def stats():
    return {
        "learner": learner_svc.stats(),
        "rss_mb": get_rss_mb(),
        "uptime_sec": round(time.time() - start_time, 1),
        "models": {
            "moonshine": getattr(moonshine_svc,'model_tier',''),
            "finbert": getattr(finbert_svc,'model_name',''),
        }
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
