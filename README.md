# XAUUSD Node 3 — AI Speech & Sentiment Services

This repository branch (`Node3`) hosts the Python AI services for the XAUUSD trading system, configured to run on a **Lightning AI Always-On VM**.

For detailed setup, model specifications, and deployment steps, see **[`node3-ai/README.md`](node3-ai/README.md)**.

## Quick Start

```bash
cd node3-ai
pip install --no-cache-dir -r requirements.txt
python3 download_models.py
python3 -u ws_client.py
```
