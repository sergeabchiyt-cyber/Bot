# XAUUSD Node 3 — AI Speech & Sentiment Services (UPGRADED 16GB tier)

This repository branch (`Node3`) hosts the Python AI services for the XAUUSD trading system. Originally tuned for Lightning AI Always-On, it now ships an **upgraded tier for 16GB RAM / 4 vCPU / 400GB disk** (better accuracy → better trades).

- **Analysis & upgraded specs:** see [`node3-ai/README.md`](node3-ai/README.md) (repo walk-through + model tables + tier comparison)
- **Copy-paste launch:** see [`node3-ai/LAUNCH.md`](node3-ai/LAUNCH.md) (systemd / Docker / bare, 30-sec to boot)
- **Upgrades in this branch:** `FOMC-RoBERTa 355M` (93% FOMC), `Moonshine Medium 245M + Silero VAD`, `Adam+Welford learner v2`

## Quick Start (legacy small — still works)

```bash
cd node3-ai
pip install --no-cache-dir -r requirements.txt
python3 download_models.py --tier small
python3 -u ws_client.py
```

## Upgraded for 16GB (recommended — your hardware)

```bash
cd node3-ai
pip install --no-cache-dir -r requirements.txt
python3 download_models.py --tier upgraded   # FOMC-RoBERTa + VAD + medium optional
cp .env.example .env
python3 -u ws_client.py                      # ~1.8GB RSS, FOMC-RoBERTa tier
# or: python3 -m uvicorn main:app --host 0.0.0.0 --port 8000  # HTTP health/test
```
