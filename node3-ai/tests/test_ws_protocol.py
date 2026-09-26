import asyncio
import base64
import json
import os
import sys
import unittest
import numpy as np
import websockets

curr_dir = os.path.dirname(os.path.abspath(__file__))
parent_dir = os.path.dirname(curr_dir)
if parent_dir not in sys.path:
    sys.path.insert(0, parent_dir)

from ws_client import Node3Client, get_rss_mb


class TestWSProtocol(unittest.IsolatedAsyncioTestCase):
    async def test_ws_communication_roundtrip(self):
        received_frames = []
        server_ws = None

        # Start mock Node 1 WebSocket server on localhost
        async def echo_handler(websocket):
            nonlocal server_ws
            server_ws = websocket
            try:
                async for message in websocket:
                    frame = json.loads(message)
                    received_frames.append(frame)
            except websockets.exceptions.ConnectionClosed:
                pass

        server = await websockets.serve(echo_handler, "127.0.0.1", 18765)

        # Setup Node 3 client pointing to mock server
        client = Node3Client()
        client.url = "ws://127.0.0.1:18765"
        client_task = asyncio.create_task(client.run())

        # Wait briefly for connection and initial handshake
        await asyncio.sleep(0.5)

        # 1. Verify Subscribe frame
        subscribe_frames = [f for f in received_frames if f.get("type") == "subscribe"]
        self.assertTrue(len(subscribe_frames) >= 1)
        self.assertEqual(subscribe_frames[0]["topics"], ["audio_chunk", "learn"])

        # 2. Verify Health frame
        health_frames = [f for f in received_frames if f.get("type") == "health"]
        self.assertTrue(len(health_frames) >= 1)
        self.assertIn("rss_mb", health_frames[0]["data"])
        self.assertIn("models_loaded", health_frames[0]["data"])

        # 3. Simulate Node 1 sending a learn frame
        if server_ws:
            initial_samples = client.learner.total_samples
            learn_msg = json.dumps({"type": "learn", "features": [1.0, -0.5, 2.0], "target": 1.0})
            await server_ws.send(learn_msg)
            await asyncio.sleep(0.3)
            self.assertEqual(client.learner.total_samples, initial_samples + 1)

        # 4. Simulate Node 1 sending an audio chunk
        if server_ws:
            # 160 samples (multiple of 80) of dummy audio
            audio = np.zeros(160, dtype=np.float32)
            audio_b64 = base64.b64encode(audio.tobytes()).decode("utf-8")
            audio_msg = json.dumps({"type": "audio_chunk", "data": audio_b64})
            await server_ws.send(audio_msg)
            await asyncio.sleep(0.3)

        # Stop client and server
        client.running = False
        if client.ws:
            await client.ws.close()
        client_task.cancel()
        server.close()
        await server.wait_closed()

    def test_rss_mb(self):
        rss = get_rss_mb()
        self.assertIsInstance(rss, float)
        self.assertGreater(rss, 0.0)


if __name__ == "__main__":
    unittest.main()
