# LAUNCH — XAUUSD Node3 Upgraded (16GB / 4vCPU / 400GB)

Copy-paste steps tested for three setups: **systemd (Lightning AI Always-On / Ubuntu VPS)**, **Docker**, and **bare binary (no systemd)**. All keep ONNX-only at runtime.

---

## 0) Pre-flight (one minute)

```bash
# confirm hardware matches README claim
free -h                # expect ~16GB
nproc                  # expect 4
df -h /teamspace 2>/dev/null || df -h .
lscpu | grep -E "Model name|AVX|vnni"  # expect avx512_vnni (speeds INT8 ~2x)
python3 --version      # 3.10+ fine

cd /teamspace/studios/this_studio/xauusd-node3/node3-ai   # or wherever you cloned
git status
cat node3-ai/README.md | head -80  # see analysis if you need it
```

If you're on this repo's clone at `/home/user/Bot`, `cd /home/user/Bot/node3-ai`.

---

## 1) Dependencies (30s, torch-free runtime)

```bash
pip install --no-cache-dir -r requirements.txt
python3 -c "import onnxruntime as ort; print(ort.__version__, ort.get_available_providers())"
# -> 1.20.0 ['CPUExecutionProvider']
```

If `huggingface_hub` missing:

```bash
pip install huggingface-hub==0.26.0 tokenizers==0.20.0 transformers==4.46.0
```

---

## 2) Models — pick your tier

### Recommended for your 16GB box: upgraded tier
~2.2 GB total, RSS ~1.8 GB, FOMC-RoBERTa 93% FOMC acc. With your 400GB disk you barely notice.

```bash
python3 download_models.py --tier upgraded
# Download log should end with: 🎉 Upgraded tier ready: FOMC-RoBERTa + Silero VAD on 16GB box

ls -lh models/
# expect:
# fomc-roberta/            ~400MB-1.4GB (quantized vs FP32)
# finbert-tone/            ~420MB
# finbert-int8/            ~110MB
# moonshine-streaming-onnx/ ~341MB (small)
# moonshine-streaming-medium-onnx/ ~650MB if medium fetch succeeded
# silero-vad/silero_vad.onnx 2.2MB

du -sh models/*
```

**If network restricted or HF rate-limited**, `small` tier still trades (fallback heuristic is calibrated):

```bash
python3 download_models.py --tier small
# still gets: moonshine small + finbert-int8 only (~500MB)
```

### One-time ONNX build (optional, only if `models/fomc-roberta` has `pytorch_model.bin` but no `.onnx`)

On the 16GB machine you can afford torch for the build:

```bash
pip install torch --index-url https://download.pytorch.org/whl/cpu
pip install "optimum[onnxruntime]==1.20.0" onnx==1.18.0
python3 download_models.py --tier upgraded --export-onnx
ls models/fomc-roberta/*.onnx
# expect model.onnx or model_quantized.onnx (~400MB)
```

You do this **once**; runtime stays torch-free after.

### Verify models load (no WebSocket needed)

```bash
python3 - << 'PY'
from finbert_service import FinBERTService
from moonshine_service import MoonshineService
from rill_learner import OnlineLearner
fb = FinBERTService(); print("FINBERT:", fb.model_name, fb.label_mode, fb.is_onnx_loaded, fb.predict("The Federal Reserve raised rates by 75 basis points."))
ms = MoonshineService(); print("MOONSHINE:", ms.model_tier, ms.is_onnx_loaded, ms.model_dir, "VAD", bool(ms.vad_session))
l = OnlineLearner(); print("LEARNER v", l.version, l.update([1.2,-0.4,0.68,2.1],1.0))
PY
```

Expected (upgraded):
```
FINBERT: fomc:roberta_model_quantized.onnx fomc True {'hawkish':0.8..., ... latency_ms:...}
MOONSHINE: medium True ... VAD True/False (VAD optional)
LEARNER v 2 {'loss':..., 'running_acc':...}
```

If FOMC-RoBERTa missing you'll see `finbert:heuristic_fallback` — still trades via heuristic, upgrade when network allows.

---

## 3) Config

```bash
cp .env.example .env
cat .env
# Same-region Singapore deployment: point directly at Node 1's secure WS
# endpoint (provider-private DNS/networking if available; otherwise its direct
# public WSS hostname). Never use Node2 or a localhost URL from Node3.
NODE1_WS_URL=wss://engine-southeastasia-sng-main.onrender.com/ws
# SESSION_ID=ai-01
# MOONSHINE_PREFERRED=medium   # small if you want lower latency
# FINBERT_PREFERRED=fomc       # tone or finbert to force fallback
```

`.env` is auto-loaded by both `ws_client.py` and `main.py`.

---

## 4) Launch — pick ONE

### A) Systemd (Lightning AI Always-On / Ubuntu VPS) — production

```bash
sudo cp xauusd-node3.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable xauusd-node3
sudo systemctl start xauusd-node3
sudo systemctl status xauusd-node3 --no-pager

# logs (400GB headroom → keep 30d, then vacuum)
tail -f node3.log
journalctl -u xauusd-node3 -f --no-pager
tail -f node3.err

# verify live
curl -s http://127.0.0.1:8000/health 2>&1 | head -c 200 || echo "ws_client doesn't start HTTP; use main.py for HTTP"
# ws_client is WS only. To test sentiment HTTP separately:
python3 -m uvicorn main:app --host 0.0.0.0 --port 8000 &
curl -s -X POST http://127.0.0.1:8000/sentiment -H "Content-Type: application/json" \
  -d '{"text":"The labor market remains tight and inflation is elevated"}' | python -m json.tool
# -> {"hawkish":0.78..., "dovish":..., "neutral":..., "confidence":..., "model":"fomc:..."}
```

To stop / restart:

```bash
sudo systemctl restart xauusd-node3
sudo systemctl stop xauusd-node3
```

**400GB log hygiene:**
```bash
journalctl --vacuum-size=500M
truncate -s 0 node3.log node3.err   # when you rotate
```

### B) No systemd (container / shared box / quick test)

```bash
# foreground WS client (connects to Node1)
python3 -u ws_client.py
# logs to stdout: "WS connected", "Fed transcript", "Pushed sentiment"

# In another shell — optional HTTP API for probes:
python3 -m uvicorn main:app --host 0.0.0.0 --port 8000 --log-level info
curl http://127.0.0.1:8000/
curl http://127.0.0.1:8000/health
curl http://127.0.0.1:8000/stats
```

### C) Docker (if your 16GB box prefers it)

```bash
# Dockerfile is minimal (add if not present)
cat > Dockerfile.node3 << 'DF'
FROM python:3.11-slim
WORKDIR /app
COPY node3-ai/requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt
COPY node3-ai/ .
RUN mkdir -p models
CMD ["python3","-u","ws_client.py"]
DF

docker build -f Dockerfile.node3 -t xauusd-node3:upgraded .

docker run -d --name node3 --restart=unless-stopped \
  --cpus=4 --memory=14g \
  -v $PWD/node3-ai/models:/app/models \
  -v $PWD/node3-ai/learner_state.json:/app/learner_state.json \
  --env-file node3-ai/.env \
  -p 8000:8000 \
  xauusd-node3:upgraded
docker logs -f node3
# exec sentiment test
docker exec node3 python3 -c "from finbert_service import FinBERTService; print(FinBERTService().predict('pause rate hikes'))"
```

---

## 5) Verify end-to-end (Node1 → Node3 → Node1 round trip)

**From Node3 shell, fake an audio chunk and learn message:**

```bash
python3 - << 'PY'
import asyncio, json, base64, numpy as np, websockets
# Quick in-process test without needing Node1
from finbert_service import FinBERTService
from moonshine_service import MoonshineService
fb=FinBERTService(); print(fb.predict("We will slow the pace of rate hikes as inflation has eased."))
print(fb.predict("Inflation remains elevated and the labor market is tight; higher for longer may be warranted."))

# Fake 1s of 16kHz silence + tone then transcribe (falls back if no model)
ms=MoonshineService()
silence=np.zeros(16000, dtype=np.float32)
print("silence ->", repr(ms.transcribe(silence))[:100])
PY
```

**Live WS check (when Node1 is up):**

```bash
# ws_client logs should show every 60s:
# Health frame sent / RSS ~1780 MB

# Send a test learn frame via ws_client's own path (or via Node1 dashboard)
python3 - << 'PY'
from rill_learner import OnlineLearner
l=OnlineLearner()
# simulate 10 wins / losses with mixed-scale features like live XAUUSD
import random
for i in range(10):
    feats=[random.uniform(-5,5), random.uniform(-2,2), random.uniform(0,1), random.uniform(-1,1)]
    tgt=1.0 if sum(feats)>0.5 else 0.0
    print(l.update(feats, tgt))
print(l.stats(), "suggested threshold", l.suggested_threshold())
PY
```

---

## 6) Ops notes for 16GB / 400GB

- **MemoryMax 14G** in service file leaves 2G for system/journal. RSS upgraded ~1.8GB so you could comfortably run a 2nd small sidecar (e.g., weekly volume profile) without swapping.
- **400GB disk:** keep `models/` (~2.2GB) pinned; `learner_state.json` stays KBs even after 1M trades (weights are `feature_dim` floats). Logs are the only grower → rotate.
- **Threads:** `MOONSHINE_INTRA_THREADS=4` + `FINBERT_INTRA_THREADS=4` will contend if both run concurrently on audio spikes. In `ws_client` they run sequentially (transcribe then sentiment), so no oversub. If you add parallel, set `OMP_NUM_THREADS=2` per service.
- **VAD tuning:** if you over-gate (threshold too high) Fed whispers get dropped. Default `0.45` is permissive. For noisy YouTube Fed feeds, bump to `0.6`.
- **Fallbacks:** missing `models/fomc-roberta` automatically uses `finbert-tone` then `finbert-int8` then heuristic. You never go dark.
- **Learner v2 migration:** first run on old `learner_state.json` (v1) auto-migrates Adam moments to zero — no wipe needed. Back up `learner_state.json` before first upgraded boot if you want rollback.

---

## 7) Rollback to small (if you need low-RAM test)

```bash
MOONSHINE_PREFERRED=small FINBERT_PREFERRED=finbert python3 -u ws_client.py
# or edit .env
# MOONSHINE_PREFERRED=small
# FINBERT_PREFERRED=finbert
sudo systemctl restart xauusd-node3
```

---

## 8) Security Checklist

- `.env` holds only `NODE1_WS_URL` + `SESSION_ID`. No secrets. Don't commit `.env`.
- `learner_state.json` contains only trade features/targets — safe to back up to 400GB disk.
- `models/` can be re-downloaded; consider `tar -czf models-$(date +%F).tar.gz models/` after first success for quick restore.

---

## TL;DR one-liner for your 16GB box

```bash
pip install --no-cache-dir -r requirements.txt && python3 download_models.py --tier upgraded && cp .env.example .env && python3 -u ws_client.py
```

That boots the upgraded tier: **FOMC-RoBERTa hawkish/dovish + Moonshine Medium + VAD + Adam learner** at ~1.8GB RSS, ready to push better sentiment to Node1.
