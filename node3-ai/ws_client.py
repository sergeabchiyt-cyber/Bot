"""
Node 3 WebSocket Client — Main Service Entry Point
==================================================
Connects to Node 1 exclusively over a persistent WebSocket connection.
Executes:
1. Moonshine Small Streaming audio transcription
2. FinBERT INT8 hawkish/dovish sentiment scoring
3. Online logistic regression learning on closed trade features
4. Background keep-alive CPU tick to prevent Lightning AI VM sleep
5. Periodic health reporting and automatic exponential backoff reconnection
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

import numpy as np
import websockets
from websockets.exceptions import ConnectionClosed

# Add current directory to path for local imports
curr_dir = os.path.dirname(os.path.abspath(__file__))
if curr_dir not in sys.path:
    sys.path.insert(0, curr_dir)

from finbert_service import FinBERTService
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

NODE1_WS_URL = os.getenv(
    "NODE1_WS_URL", "wss://engine-southeastasia-sng-main.onrender.com/ws"
)
SESSION_ID = os.getenv("SESSION_ID", "ai-01")
KEEP_ALIVE_INTERVAL = int(os.getenv("KEEP_ALIVE_INTERVAL", "60"))
RECONNECT_MIN = float(os.getenv("RECONNECT_MIN_DELAY", "1.0"))
RECONNECT_MAX = float(os.getenv("RECONNECT_MAX_DELAY", "30.0"))
PING_INTERVAL = int(os.getenv("PING_INTERVAL", "20"))


def get_rss_mb() -> float:
    """Returns current process Resident Set Size in Megabytes."""
    try:
        # Read from /proc/self/statm for Linux
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
        # On Linux ru_maxrss is in KB
        return round(usage.ru_maxrss / 1024, 2)
    except Exception:
        return 0.0


class Node3Client:
    def __init__(self):
        self.start_time = time.time()
        self.url = NODE1_WS_URL
        self.session_id = SESSION_ID
        self.cached_sentiment: Optional[Dict[str, Any]] = None
        self.ws: Optional[websockets.WebSocketClientProtocol] = None
        self.running = True

        logger.info("Initializing Node 3 AI services...")
        self.moonshine = MoonshineService()
        self.finbert = FinBERTService()
        self.learner = OnlineLearner()

        self.models_loaded: List[str] = []
        if self.moonshine.is_onnx_loaded:
            self.models_loaded.append("moonshine")
        else:
            self.models_loaded.append("moonshine_fallback")

        if self.finbert.is_onnx_loaded:
            self.models_loaded.append("finbert")
        else:
            self.models_loaded.append("finbert_fallback")

        logger.info(
            "Node 3 services initialized (Models: %s, RSS: %.1f MB)",
            self.models_loaded,
            get_rss_mb(),
        )

    async def keep_alive(self):
        """
        Background CPU tick to prevent Lightning AI Always-On VM idle sleep.
        Performs light computation every KEEP_ALIVE_INTERVAL seconds.
        """
        logger.info("keep_alive loop started (interval=%ds)", KEEP_ALIVE_INTERVAL)
        while self.running:
            _ = sum(i * i for i in range(1000))  # tiny CPU work
            await asyncio.sleep(KEEP_ALIVE_INTERVAL)

    async def health_reporter(self):
        """Periodically reports health status and RSS to Node 1."""
        while self.running:
            await asyncio.sleep(60)
            if self.ws and not self.ws.closed:
                try:
                    payload = {
                        "type": "health",
                        "data": {
                            "rss_mb": get_rss_mb(),
                            "models_loaded": self.models_loaded,
                            "uptime": round(time.time() - self.start_time, 1),
                            "session_id": self.session_id,
                            "learner_samples": self.learner.total_samples,
                        },
                    }
                    await self.ws.send(json.dumps(payload))
                    logger.debug("Health frame sent: %s", payload["data"])
                except Exception as e:
                    logger.warning("Failed to send health frame: %s", e)

    async def send_frame(self, frame_type: str, data: Any):
        """Helper to send a typed JSON frame over WebSocket."""
        if not self.ws or self.ws.closed:
            return
        msg = json.dumps({"type": frame_type, "data": data})
        await self.ws.send(msg)

    async def handle_audio_chunk(self, raw_data: Any):
        """
        Transcribes incoming base64 PCM audio chunk with Moonshine,
        scores with FinBERT, and pushes sentiment back to Node 1.
        """
        try:
            now_ms = int(time.time() * 1000)
            audio_bytes = None
            if isinstance(raw_data, str):
                audio_bytes = base64.b64decode(raw_data)
            elif isinstance(raw_data, bytes):
                audio_bytes = raw_data
            elif isinstance(raw_data, dict) and "data" in raw_data:
                audio_bytes = base64.b64decode(raw_data["data"])

            if not audio_bytes:
                return

            audio_arr = np.frombuffer(audio_bytes, dtype=np.float32)
            if len(audio_arr) == 0:
                return

            # 1. Transcribe with Moonshine Small Streaming
            transcript = self.moonshine.transcribe(audio_arr)

            if transcript and transcript.strip():
                logger.info("Fed transcript: '%s'", transcript)
                await self.send_frame("transcript", {"text": transcript, "ts": now_ms})

                # 2. Score with FinBERT INT8
                sentiment = self.finbert.predict(transcript)
                sentiment["ts"] = now_ms
                self.cached_sentiment = sentiment

                # 3. Push sentiment to Node 1
                await self.send_frame("sentiment", sentiment)
                logger.info("Pushed sentiment: %s", sentiment)

        except Exception as e:
            logger.error("Error processing audio chunk: %s", e, exc_info=True)

    async def handle_learn(self, data: Any):
        """
        Updates online logistic regression learner with closed trade outcome.
        Contract: {"features": [...], "target": 0.0 | 1.0}
        """
        try:
            features = data.get("features", [])
            target = data.get("target", 0.0)
            if features is not None and target is not None:
                res = self.learner.update(features, float(target))
                logger.info("Online learner update: %s", res)
        except Exception as e:
            logger.error("Error updating online learner: %s", e)

    async def handle_incoming_message(self, message: str):
        """Parses and dispatches inbound WebSocket frame from Node 1."""
        try:
            frame = json.loads(message)
            f_type = frame.get("type")
            data = frame.get("data")

            if f_type == "audio_chunk":
                await self.handle_audio_chunk(data or frame)
            elif f_type == "learn":
                await self.handle_learn(frame)
            elif f_type == "heartbeat":
                logger.debug("Received heartbeat from Node 1")
            elif f_type == "sentiment_req":
                # Manual test trigger or query
                if self.cached_sentiment:
                    await self.send_frame("sentiment", self.cached_sentiment)
            else:
                logger.debug("Unhandled frame type from Node 1: %s", f_type)
        except json.JSONDecodeError:
            logger.warning("Received invalid JSON: %s", message[:100])
        except Exception as e:
            logger.error("Error handling incoming message: %s", e)

    async def run(self):
        """Main persistent connection loop with exponential backoff."""
        delay = RECONNECT_MIN

        # Spawn background tasks
        asyncio.create_task(self.keep_alive())
        asyncio.create_task(self.health_reporter())

        while self.running:
            logger.info("Connecting to %s", self.url)
            try:
                async with websockets.connect(
                    self.url,
                    ping_interval=PING_INTERVAL,
                    ping_timeout=20,
                    close_timeout=10,
                ) as ws:
                    self.ws = ws
                    delay = RECONNECT_MIN  # reset backoff upon successful connection
                    logger.info("WS connected")

                    # Step 8 contract: Subscribe to audio_chunk and learn
                    subscribe_frame = {
                        "type": "subscribe",
                        "topics": ["audio_chunk", "learn"],
                    }
                    await ws.send(json.dumps(subscribe_frame))
                    logger.info('Sent subscribe frame: ["audio_chunk", "learn"]')

                    # Re-push cached sentiment if available
                    if self.cached_sentiment:
                        await self.send_frame("sentiment", self.cached_sentiment)
                        logger.info(
                            "Re-pushed cached sentiment after reconnect: %s",
                            self.cached_sentiment,
                        )

                    # Initial health report
                    await self.send_frame(
                        "health",
                        {
                            "rss_mb": get_rss_mb(),
                            "models_loaded": self.models_loaded,
                            "uptime": round(time.time() - self.start_time, 1),
                            "session_id": self.session_id,
                        },
                    )

                    # Message listening loop
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
        client.learner.save()
        # Cancel all running tasks except current
        for task in asyncio.all_tasks(loop):
            if task is not asyncio.current_task():
                task.cancel()

    for sig in (signal.SIGTERM, signal.SIGINT):
        try:
            loop.add_signal_handler(sig, stop)
        except (NotImplementedError, RuntimeError):
            # Windows or non-main thread fallback
            signal.signal(sig, lambda *_: stop())


def main():
    logger.info("Starting XAUUSD Node 3 AI service (ws_client.py)")
    client = Node3Client()
    loop = asyncio.new_event_loop()
    asyncio.set_event_loop(loop)
    handle_signals(client, loop)

    try:
        loop.run_until_complete(client.run())
    except (asyncio.CancelledError, KeyboardInterrupt):
        pass
    finally:
        client.learner.save()
        loop.close()
        logger.info("Node 3 stopped.")


if __name__ == "__main__":
    main()
