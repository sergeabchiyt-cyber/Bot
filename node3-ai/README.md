# XAUUSD Node 3 — AI Speech & Sentiment Services

Node 3 is the dedicated Python AI service designed to deploy on the **Lightning AI Always-On VM**. It operates in real-time alongside Node 1 (Rust Execution Engine) and Node 2 (Web Dashboard).

## Mission & Architecture

Node 3 performs four core functions:
1. **ASR (Speech-to-Text)**: Transcribes live Federal Reserve audio streams via **Moonshine Small Streaming** (123M, ONNX INT8).
2. **Sentiment Scoring**: Scores transcribed Fed speech for Hawkish / Dovish / Neutral sentiment via **FinBERT INT8**.
3. **Continuous Online Learning**: Learns trade outcome patterns from every closed trade using an online logistic regression model (`OnlineLearner` in `rill_learner.py`).
4. **WebSocket Transport**: Communicates with Node 1 exclusively over a persistent WebSocket connection with exponential backoff and automatic reconnect recovery.

```
       +----------------------------------------------------+
       |                   Node 1 (Engine)                  |
       |  Rust WebSocket Server (onrender.com / 0.0.0.0)    |
       +-------------------------+--------------------------+
                                 |
              WebSocket (audio_chunk, learn, sentiment)
                                 |
       +-------------------------v--------------------------+
       |               Node 3 (Lightning AI VM)             |
       |                                                    |
       |   +--------------------------------------------+   |
       |   | ws_client.py (Persistent WebSocket Client)|   |
       |   +---------------------+----------------------+   |
       |                         |                          |
       |         +---------------+---------------+          |
       |         |               |               |          |
       |   +-----v-----+   +-----v-----+   +-----v------+   |
       |   | Moonshine |   |  FinBERT  |   |   Online   |   |
       |   |    ASR    |   | Sentiment |   |  Learner   |   |
       |   | (ONNX INT8|   | (ONNX INT8|   |  (rill-ml  |   |
       |   |  123M)    |   |   BERT)   |   |  fallback) |   |
       |   +-----------+   +-----------+   +------------+   |
       |                                                    |
       |   +--------------------------------------------+   |
       |   | keep_alive() CPU tick (Every 60s)          |   |
       |   +--------------------------------------------+   |
       +----------------------------------------------------+
```

---

## Hard Constraints (User-Verified)

| Parameter | Specification | Notes |
| :--- | :--- | :--- |
| **Host** | Lightning AI Always-On VM (Free tier) | Provisioned as Always-On VM |
| **RAM** | 16 GB | ~1.73 GB peak footprint, ~14 GB headroom |
| **vCPU** | 4 | CPU execution provider (no GPU needed) |
| **Disk** | 400 GB persistent | `/teamspace/studios/this_studio/` |
| **Restart policy**| None | User-verified: no 4-hour restart on Always-On VM |
| **Idle sleep** | Prevented by 60s CPU tick | Built-in `keep_alive()` background coroutine |
| **Torch Dependency**| **Prohibited** | Both models run in ONNX Runtime; saves 2 GB RAM/disk |

---

## Model Specifications

### 1. Moonshine Small Streaming (ONNX INT8)
- **HuggingFace Repository**: `Mer0vin8ian/moonshine-streaming-small-onnx`
- **Base Architecture**: `UsefulSensors/moonshine-streaming-small` (123M parameter encoder-decoder transformer)
- **Quantization**: Dynamic INT8 (weight-only, MatMul/Gemm ops)
- **Total Model Size**: ~341 MB (vs ~1.3 GB FP32)
- **Input Contract**: 16 kHz mono float32 PCM audio, padded to a multiple of 80.
- **Latency**: Sub-300ms time-to-first-token on CPU (10–100x faster than Whisper).

### 2. FinBERT INT8
- **HuggingFace Repository**: `sekarkrishna/finbert-int8`
- **Quantization**: Dynamic INT8 via ONNX Runtime (~2–3x faster, ~4x smaller)
- **Gold Sentiment Mapping**:
  - `Hawkish` (bearish gold) $\rightarrow$ FinBERT negative (`probs[1]`)
  - `Dovish` (bullish gold) $\rightarrow$ FinBERT positive (`probs[0]`)
  - `Neutral` $\rightarrow$ FinBERT neutral (`probs[2]`)
- **Domain Nuance**: FinBERT is pre-trained on financial news (where negative news drops equities). In central bank communication, rate hikes and tighter policy correspond to FinBERT negative (Hawkish), while easing corresponds to FinBERT positive (Dovish).

---

## WebSocket Architecture Contract

### Outbound Frames (Node 3 $\rightarrow$ Node 1)
- **Subscription**:
  ```json
  {"type": "subscribe", "topics": ["audio_chunk", "learn"]}
  ```
- **Sentiment**:
  ```json
  {
    "type": "sentiment",
    "data": {
      "hawkish": 0.72,
      "dovish": 0.11,
      "neutral": 0.17,
      "confidence": 0.72,
      "ts": 1700000000000
    }
  }
  ```
- **Transcript**:
  ```json
  {
    "type": "transcript",
    "data": {
      "text": "The Federal Reserve raised rates by 75 basis points.",
      "ts": 1700000000000
    }
  }
  ```
- **Health**:
  ```json
  {
    "type": "health",
    "data": {
      "rss_mb": 460.0,
      "models_loaded": ["moonshine", "finbert"],
      "uptime": 3600.0,
      "session_id": "ai-01"
    }
  }
  ```

### Inbound Frames (Node 1 $\rightarrow$ Node 3)
- **Audio Chunk**:
  ```json
  {"type": "audio_chunk", "data": "<base64 encoded float32 PCM 16kHz>"}
  ```
- **Learn (Closed Trade Outcome)**:
  ```json
  {"type": "learn", "features": [1.5, -0.4, 0.8], "target": 1.0}
  ```

---

## Deployment Instructions on Lightning AI Always-On VM

### Step 1 — Provision the Studio
Log into `lightning.ai`, create a new CPU Studio on the Always-On VM configuration.

### Step 2 — Clone and Enter Directory
```bash
cd /teamspace/studios/this_studio
git clone https://github.com/sergeabchiyt-cyber/Bot xauusd-node3
cd xauusd-node3/node3-ai
```

### Step 3 — Install Dependencies
```bash
pip install --no-cache-dir -r requirements.txt
```

### Step 4 — Download Models
```bash
python3 download_models.py
```
Or run directly:
```bash
mkdir -p models
python3 -c "
from huggingface_hub import snapshot_download
snapshot_download('Mer0vin8ian/moonshine-streaming-small-onnx', local_dir='models/moonshine-streaming-onnx')
snapshot_download('sekarkrishna/finbert-int8', local_dir='models/finbert-int8')
"
```

### Step 5 — Verify Model Files
```bash
ls -la models/moonshine-streaming-onnx/
# Expect: encoder_model_int8.onnx, decoder_model_int8.onnx, decoder_with_past_model_int8.onnx, tokenizer.json

ls -la models/finbert-int8/
# Expect: model_quantized.onnx, config.json, vocab.txt, tokenizer_config.json
```

### Step 6 — Configure Environment
```bash
cp .env.example .env
```
Ensure `.env` contains:
```env
NODE1_WS_URL=wss://engine-southeastasia-sng-main.onrender.com/ws
SESSION_ID=ai-01
RUST_LOG=info
```

### Step 7 — Configure and Start Systemd Unit
```bash
sudo cp xauusd-node3.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable xauusd-node3
sudo systemctl start xauusd-node3
sudo systemctl status xauusd-node3
```

### Step 8 — Verify Logs & Sentiment Round-Trip
```bash
tail -f node3.log
```
Verify sentiment prediction:
```bash
python3 -c "
from finbert_service import FinBERTService
fb = FinBERTService()
print(fb.predict('The Federal Reserve raised rates by 75 basis points.'))
"
```
Expected Output:
```
{'hawkish': 0.72, 'dovish': 0.11, 'neutral': 0.17, 'confidence': 0.72}
```

---

## File Directory Reference

```
node3-ai/
├── .env                       # Environment variables
├── .env.example               # Example template
├── download_models.py         # Automated model downloader & integrity verifier
├── finbert_service.py         # FinBERT INT8 ONNX wrapper & Fed sentiment scorer
├── main.py                    # FastAPI /health, /sentiment, /learn REST API
├── moonshine_service.py       # Moonshine Small Streaming ONNX INT8 wrapper
├── requirements.txt           # Python dependencies (torch-free)
├── rill_learner.py            # Online logistic regression model (rill-ml fallback)
├── ws_client.py               # Main daemon entry point (WebSocket client + keep-alive)
├── xauusd-node3.service       # Systemd service unit definition
└── models/                    # Downloaded model weights (persistent storage)
    ├── finbert-int8/
    └── moonshine-streaming-onnx/
```
