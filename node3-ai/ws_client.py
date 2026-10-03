"""
Node 3 WebSocket Client — Main Service Entry Point (UPGRADED for 16GB / 4vCPU / 400GB)
=======================================================================================
Connects to Node 1 exclusively over a persistent WebSocket connection.
Executes:
1. Moonshine Streaming audio transcription (MEDIUM 245M default on 16GB, small fallback)
2. FOMC-RoBERTa sentiment scoring (355M, 93% FOMC acc) with FINBERT fallback
3. Online Adam logistic regression with Welford scaling
4. Background keep-alive CPU tick (tuned: every 45s on 16GB, lighter)
5. Periodic health reporting and exponential backoff reconnection
- Reports model tier, RSS, learner v2 stats to Node1 /health

4 vCPU tuning: sets ONNX intra_op threads via env already in services.
400GB note: learner_state.json can grow unbounded if features logged; we still keep single JSON.
"""

import asyncio
import base64
import json
import logging
import os
import signal
import sys
import time
from typing import Any, Dict, List, Optional

# practical default for 16GB box — increase ORT threads early
os.environ.setdefault("MOONSHINE_INTRA_THREADS", "4")
os.environ.setdefault("FINBERT_INTRA_THREADS", "4")

import numpy as np
import websockets
from websockets.exceptions import ConnectionClosed

# Add current directory to path for local imports
curr_dir = os.path.dirname(os.path.abspath(__file__))
if curr_dir not in sys.path:
    sys.path.insert(0, curr_dir)

from finbert_service import FinBERTService
from market_state import MarketState
from moonshine_service import MoonshineService
from rill_learner import OnlineLearner

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    handlers=[logging.StreamHandler(sys.stdout)],
)
logger = logging.getLogger("node3.ws")


def load_env(env_path: Optional[str] = None):
    """Simple .env loader if python-dotenv is not installed."""
    if env_path is None:
        env_path = os.path.join(curr_dir, ".env")
    if not os.path.isfile(env_path):
        return
    with open(env_path, "r") as f:
        for line in f:
            line = line.strip()
            if line and not line.startswith("#") and "=" in line:
                k, v = line.split("=", 1)
                os.environ.setdefault(k.strip(), v.strip())


load_env()

# Prefer Node1's direct same-region WSS endpoint (or private service DNS when
# supported); the public Singapore hostname is the fallback.
NODE1_WS_URL = os.getenv(
    "NODE1_WS_URL", "wss://engine-southeastasia-sng-main.onrender.com/ws"
)
SESSION_ID = os.getenv("SESSION_ID", "ai-01")
KEEP_ALIVE_INTERVAL = int(os.getenv("KEEP_ALIVE_INTERVAL", "45"))  # tighter on 16GB (was 60)
RECONNECT_MIN = float(os.getenv("RECONNECT_MIN_DELAY", "1.0"))
RECONNECT_MAX = float(os.getenv("RECONNECT_MAX_DELAY", "30.0"))
PING_INTERVAL = int(os.getenv("PING_INTERVAL", "20"))


def get_rss_mb() -> float:
    """Returns current process Resident Set Size in Megabytes."""
    try:
        if os.path.exists("/proc/self/statm"):
            with open("/proc/self/statm", "r") as f:
                fields = f.read().split()
                rss_pages = int(fields[1])
                page_size_kb = os.sysconf("SC_PAGE_SIZE") / 1024
                return round((rss_pages * page_size_kb) / 1024, 2)
    except Exception:
        pass
    try:
        import resource
        usage = resource.getrusage(resource.RUSAGE_SELF)
        return round(usage.ru_maxrss / 1024, 2)
    except Exception:
        return 0.0


class Node3Client:
    def __init__(self):
        self.start_time = time.time()
        self.url = NODE1_WS_URL
        self.session_id = SESSION_ID
        self.cached_sentiment: Optional[Dict[str, Any]] = None
        # Populated by the immediate levels + 15-candle replay on every
        # subscription/reconnect; execution components can read `levels` and
        # the ready 14-period ATR directly from this cache.
        self.market_state = MarketState()
        self.ws: Optional[websockets.WebSocketClientProtocol] = None
        self.running = True

        logger.info("Initializing Node 3 AI services (16GB upgraded tier)...")
        # Respect env preferences: MOONSHINE_PREFERRED=medium, FINBERT_PREFERRED=fomc
        # Default to upgraded tier if 16GB detected (>8GB) — auto-promote
        try:
            with open("/proc/meminfo") as f:
                mem_kb = int([l for l in f if "MemTotal" in l][0].split()[1])
            if mem_kb > 10_000_000 and "MOONSHINE_PREFERRED" not in os.environ:
                os.environ["MOONSHINE_PREFERRED"] = "medium"
            if mem_kb > 10_000_000 and "FINBERT_PREFERRED" not in os.environ:
                os.environ["FINBERT_PREFERRED"] = "fomc"
        except Exception:
            pass

        self.moonshine = MoonshineService()
        self.finbert = FinBERTService()
        self.learner = OnlineLearner()

        self.models_loaded: List[str] = []
        if self.moonshine.is_onnx_loaded:
            self.models_loaded.append(f"moonshine_{self.moonshine.model_tier}")
        else:
            self.models_loaded.append("moonshine_fallback")
            # still record tier attempt
            self.models_loaded.append(f"moonshine_{self.moonshine.model_tier}_not_loaded")

        if self.finbert.is_onnx_loaded:
            self.models_loaded.append(f"{self.finbert.label_mode}:{self.finbert.model_name}")
        else:
            self.models_loaded.append("finbert_heuristic")
            self.models_loaded.append(f"{self.finbert.label_mode}_not_loaded")

        # add learner version marker
        self.models_loaded.append(f"learner_v{getattr(self.learner,'version',1)}_adam")

        logger.info(
            "Node 3 upgraded services initialized (Models: %s, RSS: %.1f MB, vad=%s)",
            self.models_loaded,
            get_rss_mb(),
            bool(getattr(self.moonshine, 'vad_session', None)),
        )

    async def keep_alive(self):
        """
        Background CPU tick to prevent VM idle sleep.
        On 16GB box we can afford slightly more frequent but lighter tick.
        """
        logger.info("keep_alive loop started (interval=%ds)", KEEP_ALIVE_INTERVAL)
        while self.running:
            _ = sum(i * i for i in range(1000))
            await asyncio.sleep(KEEP_ALIVE_INTERVAL)

    async def health_reporter(self):
        """Periodically reports health status and RSS to Node 1 (every 60s)."""
        while self.running:
            await asyncio.sleep(60)
            if self.ws and not getattr(self.ws, 'closed', False):
                try:
                    payload = {
                        "type": "health",
                        "data": {
                            "rss_mb": get_rss_mb(),
                            "models_loaded": self.models_loaded,
                            "uptime": round(time.time() - self.start_time, 1),
                            "session_id": self.session_id,
                            "learner_samples": self.learner.total_samples,
                            "learner_acc": round(getattr(self.learner, 'running_acc', 0), 4),
                        },
                    }
                    await self.ws.send(json.dumps(payload))
                    logger.debug("Health frame sent: %s", payload["data"])
                except Exception as e:
                    logger.warning("Failed to send health frame: %s", e)

    async def send_frame(self, frame_type: str, data: Any):
        if not self.ws or getattr(self.ws, 'closed', False):
            return
        msg = json.dumps({"type": frame_type, "data": data})
        await self.ws.send(msg)

    async def send_trade_event(self, trade_event: Dict[str, Any]):
        """Publish a Node3 execution result/signal for Node1 dashboard fan-out."""
        required = {
            "trade_id", "symbol", "side", "size", "entry", "sl", "tp",
            "status", "timestamp",
        }
        if not isinstance(trade_event, dict) or not required.issubset(trade_event):
            missing = sorted(required - set(trade_event)) if isinstance(trade_event, dict) else sorted(required)
            raise ValueError(f"TradeEvent is missing required fields: {missing}")
        await self.send_frame("trades", trade_event)

    async def handle_audio_chunk(self, raw_data: Any):
        try:
            now_ms = int(time.time() * 1000)
            audio_bytes = None
            if isinstance(raw_data, str):
                try:
                    audio_bytes = base64.b64decode(raw_data)
                except Exception:
                    audio_bytes = None
            elif isinstance(raw_data, bytes):
                audio_bytes = raw_data
            elif isinstance(raw_data, dict) and "data" in raw_data:
                audio_bytes = base64.b64decode(raw_data["data"])
            elif isinstance(raw_data, dict) and "audio" in raw_data:
                # alternative field
                v = raw_data["audio"]
                if isinstance(v, str):
                    audio_bytes = base64.b64decode(v)
                elif isinstance(v, bytes):
                    audio_bytes = v

            if not audio_bytes:
                return

            # Support both float32 PCM and int16 fallback (some engines send int16)
            try:
                audio_arr = np.frombuffer(audio_bytes, dtype=np.float32)
                # sanity: if values all tiny and byte length even, maybe it's int16
                if len(audio_arr) > 0 and np.max(np.abs(audio_arr)) < 1e-6 and len(audio_bytes) % 2 == 0:
                    i16 = np.frombuffer(audio_bytes, dtype=np.int16)
                    audio_arr = (i16.astype(np.float32) / 32768.0)
            except Exception:
                audio_arr = np.frombuffer(audio_bytes, dtype=np.int16).astype(np.float32) / 32768.0

            if len(audio_arr) == 0:
                return

            # 1. Transcribe — medium tier if loaded does 15% better WER
            transcript = self.moonshine.transcribe(audio_arr)

            if transcript and transcript.strip():
                logger.info("Fed transcript [%s]: '%s'", self.moonshine.model_tier, transcript)
                await self.send_frame("transcript", {"text": transcript, "ts": now_ms, "tier": self.moonshine.model_tier})

                # 2. Score — FOMC-RoBERTa tier best for Fed speech
                sentiment = self.finbert.predict(transcript)
                sentiment["ts"] = now_ms
                self.cached_sentiment = sentiment
                await self.send_frame("sentiment", sentiment)
                logger.info("Pushed sentiment [%s]: %s", sentiment.get("model", "unknown"), sentiment)

        except Exception as e:
            logger.error("Error processing audio chunk: %s", e, exc_info=True)

    async def handle_learn(self, data: Any):
        try:
            # Contract supports both {"type":"learn","features":..} and {"type":"learn","data":{"features":...}}
            src = data
            if isinstance(data, dict) and "data" in data and isinstance(data["data"], dict) and "features" in data["data"]:
                src = data["data"]
            elif isinstance(data, dict) and "features" not in data and "target" not in data and "data" in data:
                src = data.get("data", data)
            features = src.get("features", []) if isinstance(src, dict) else []
            target = src.get("target", 0.0) if isinstance(src, dict) else 0.0
            if features is not None and target is not None and len(features) > 0:
                res = self.learner.update(features, float(target))
                logger.info("Online learner v2 update: %s", res)
                # optionally echo back learner update
                # await self.send_frame("learner_update", res)
        except Exception as e:
            logger.error("Error updating online learner: %s", e)

    async def handle_incoming_message(self, message: str):
        try:
            frame = json.loads(message)
            f_type = frame.get("type")
            data = frame.get("data")

            if f_type == "audio_chunk":
                await self.handle_audio_chunk(data or frame)
            elif f_type == "learn":
                await self.handle_learn(frame)
            elif f_type == "levels":
                if self.market_state.update_levels(data):
                    logger.debug(
                        "Cached %s levels: POC %.3f VAH %.3f VAL %.3f",
                        data["window"], data["poc"], data["vah"], data["val"],
                    )
            elif f_type == "candle":
                was_ready = self.market_state.atr_14 is not None
                if self.market_state.update_candle(data):
                    if not was_ready and self.market_state.atr_14 is not None:
                        logger.info(
                            "Market state initialized from replay: levels=%s ATR14=%.5f",
                            sorted(self.market_state.levels),
                            self.market_state.atr_14,
                        )
            elif f_type == "trades":
                # Node1 rebroadcasts a Node3 trade to all `trades` subscribers,
                # including this socket. Treat it as an acknowledgement only;
                # never feed the echo back into execution.
                logger.debug("Trade frame received from Node1: %s", data)
            elif f_type == "heartbeat":
                logger.debug("Received heartbeat from Node 1")
            elif f_type == "sentiment_req":
                if self.cached_sentiment:
                    await self.send_frame("sentiment", self.cached_sentiment)
            elif f_type == "predict_req":
                # ad-hoc feature -> prediction (for dashboard)
                feats = frame.get("features") or (data.get("features") if isinstance(data, dict) else None)
                if feats:
                    p = self.learner.predict_proba(feats)
                    await self.send_frame("prediction", {"proba": p, "threshold": self.learner.suggested_threshold()})
            else:
                logger.debug("Unhandled frame type from Node 1: %s", f_type)
        except json.JSONDecodeError:
            logger.warning("Received invalid JSON: %s", message[:100])
        except Exception as e:
            logger.error("Error handling incoming message: %s", e)

    async def run(self):
        delay = RECONNECT_MIN
        asyncio.create_task(self.keep_alive())
        asyncio.create_task(self.health_reporter())

        while self.running:
            logger.info("Connecting to %s (tier: moonshine=%s finbert=%s)", self.url, self.moonshine.model_tier, self.finbert.label_mode)
            try:
                async with websockets.connect(
                    self.url,
                    ping_interval=PING_INTERVAL,
                    ping_timeout=20,
                    close_timeout=10,
                    max_size=10 * 1024 * 1024,  # allow larger audio chunks on 16GB box
                ) as ws:
                    self.ws = ws
                    delay = RECONNECT_MIN
                    # Start clean so a missing frame cannot leave stale
                    # levels/ATR from the previous connection in execution state.
                    self.market_state = MarketState()
                    logger.info("WS connected (RSS %.1f MB)", get_rss_mb())

                    subscribe_frame = {
                        "type": "subscribe",
                        "topics": ["audio_chunk", "learn", "candle", "levels", "trades"],
                    }
                    await ws.send(json.dumps(subscribe_frame))
                    logger.info(
                        'Sent subscribe frame: ["audio_chunk", "learn", "candle", "levels", "trades"]'
                    )

                    if self.cached_sentiment:
                        await self.send_frame("sentiment", self.cached_sentiment)
                        logger.info("Re-pushed cached sentiment after reconnect: %s", self.cached_sentiment)

                    await self.send_frame(
                        "health",
                        {
                            "rss_mb": get_rss_mb(),
                            "models_loaded": self.models_loaded,
                            "uptime": round(time.time() - self.start_time, 1),
                            "session_id": self.session_id,
                            "tier": "upgraded_16gb",
                            "learner_samples": self.learner.total_samples,
                        },
                    )

                    async for message in ws:
                        if not self.running:
                            break
                        await self.handle_incoming_message(message)

            except (ConnectionClosed, OSError, Exception) as e:
                logger.warning("WS connection lost or failed: %s", e)

            if not self.running:
                break

            logger.info("Reconnecting in %.1fs...", delay)
            await asyncio.sleep(delay)
            delay = min(delay * 2, RECONNECT_MAX)

        logger.info("Client shutdown complete.")


def handle_signals(client: Node3Client, loop: asyncio.AbstractEventLoop):
    def stop():
        logger.info("Termination signal received. Shutting down gracefully...")
        client.running = False
        try:
            client.learner.save()
        except Exception:
            pass
        for task in asyncio.all_tasks(loop):
            if task is not asyncio.current_task():
                task.cancel()

    for sig in (signal.SIGTERM, signal.SIGINT):
        try:
            loop.add_signal_handler(sig, stop)
        except (NotImplementedError, RuntimeError):
            signal.signal(sig, lambda *_: stop())


def main():
    logger.info("Starting XAUUSD Node 3 AI service (ws_client.py) UPGRADED 16GB tier")
    client = Node3Client()
    loop = asyncio.new_event_loop()
    asyncio.set_event_loop(loop)
    handle_signals(client, loop)
    try:
        loop.run_until_complete(client.run())
    except (asyncio.CancelledError, KeyboardInterrupt):
        pass
    finally:
        try:
            client.learner.save()
        except Exception:
            pass
        loop.close()
        logger.info("Node 3 stopped.")


if __name__ == "__main__":
    main()
