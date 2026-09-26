"""
Moonshine Small Streaming Service (ONNX INT8)
=============================================
Speech-to-text transcription service using Moonshine Small Streaming ONNX INT8.

Model Source: Mer0vin8ian/moonshine-streaming-small-onnx on HuggingFace.
Base Architecture: UsefulSensors/moonshine-streaming-small (123M parameters).
Quantization: Dynamic INT8 (weight-only, MatMul/Gemm ops).

Files expected in models/moonshine-streaming-onnx:
- encoder_model_int8.onnx (~72 MB)
- decoder_model_int8.onnx (~142 MB)
- decoder_with_past_model_int8.onnx (~127 MB)
- tokenizer.json

Audio Contract:
- 16 kHz mono float32 PCM.
- Input length padded to a multiple of 80.
- Sub-300ms time-to-first-token on CPU.
"""

import logging
import os
from typing import List, Optional, Union

import numpy as np

logger = logging.getLogger("node3.moonshine")
if not logger.handlers:
    logging.basicConfig(level=logging.INFO)


class MoonshineService:
    def __init__(self, model_dir: Optional[str] = None):
        if model_dir is None:
            curr_dir = os.path.dirname(os.path.abspath(__file__))
            candidate1 = os.path.join(curr_dir, "models", "moonshine-streaming-onnx")
            candidate2 = os.path.join(os.getcwd(), "models", "moonshine-streaming-onnx")
            model_dir = candidate1 if os.path.isdir(candidate1) else candidate2

        self.model_dir = model_dir
        self.encoder_session = None
        self.decoder_session = None
        self.decoder_past_session = None
        self.tokenizer = None
        self.is_onnx_loaded = False
        self._init_model()

    def _init_model(self):
        """Loads ONNX sessions and tokenizer if model files exist."""
        encoder_path = os.path.join(self.model_dir, "encoder_model_int8.onnx")
        decoder_path = os.path.join(self.model_dir, "decoder_model_int8.onnx")
        decoder_past_path = os.path.join(
            self.model_dir, "decoder_with_past_model_int8.onnx"
        )
        tokenizer_path = os.path.join(self.model_dir, "tokenizer.json")

        # Fallback to non-int8 naming if present
        if not os.path.isfile(encoder_path):
            encoder_path = os.path.join(self.model_dir, "encoder_model.onnx")
        if not os.path.isfile(decoder_path):
            decoder_path = os.path.join(self.model_dir, "decoder_model.onnx")

        if not (os.path.isfile(encoder_path) and os.path.isfile(decoder_path)):
            logger.warning(
                "Moonshine ONNX model files not found in '%s'. "
                "Running in fallback / mock transcription mode.",
                self.model_dir,
            )
            return

        try:
            import onnxruntime as ort
            from tokenizers import Tokenizer

            opts = ort.SessionOptions()
            opts.intra_op_num_threads = 4
            opts.inter_op_num_threads = 1
            opts.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL
            opts.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL

            self.encoder_session = ort.InferenceSession(
                encoder_path, sess_options=opts, providers=["CPUExecutionProvider"]
            )
            self.decoder_session = ort.InferenceSession(
                decoder_path, sess_options=opts, providers=["CPUExecutionProvider"]
            )
            if os.path.isfile(decoder_past_path):
                self.decoder_past_session = ort.InferenceSession(
                    decoder_past_path,
                    sess_options=opts,
                    providers=["CPUExecutionProvider"],
                )

            if os.path.isfile(tokenizer_path):
                self.tokenizer = Tokenizer.from_file(tokenizer_path)

            self.encoder_input_name = self.encoder_session.get_inputs()[0].name
            self.is_onnx_loaded = True
            logger.info("Moonshine ONNX INT8 loaded successfully from %s", self.model_dir)
        except Exception as e:
            logger.warning("Failed to initialize Moonshine ONNX sessions: %s", e)
            self.is_onnx_loaded = False

    def transcribe(self, audio_ndarray: Union[np.ndarray, List[float]]) -> str:
        """
        Transcribes 16kHz mono float32 audio to text.

        Args:
            audio_ndarray: 1D numpy array of 16kHz float32 audio samples

        Returns:
            str: Transcribed text
        """
        if audio_ndarray is None:
            return ""

        audio = np.asarray(audio_ndarray, dtype=np.float32).flatten()
        if len(audio) == 0:
            return ""

        # Contract: Audio padded to multiple of 80
        if len(audio) % 80 != 0:
            pad_len = 80 - (len(audio) % 80)
            audio = np.pad(audio, (0, pad_len), mode="constant")

        if self.is_onnx_loaded and self.encoder_session and self.decoder_session:
            try:
                return self._transcribe_onnx(audio)
            except Exception as e:
                logger.error("Moonshine ONNX transcription failed: %s", e)
                return ""

        return self._transcribe_fallback(audio)

    def _transcribe_onnx(self, audio: np.ndarray) -> str:
        """Executes full encoder-decoder ONNX transcription pipeline."""
        # Shape: (batch_size=1, sequence_length)
        audio_input = np.expand_dims(audio, axis=0)
        enc_feed = {self.encoder_input_name: audio_input}

        enc_outputs = self.encoder_session.run(None, enc_feed)
        encoder_hidden_states = enc_outputs[0]

        # Autoregressive decoding
        # Start token (BOS): typically token 1 or special token <|startoftranscript|>
        bos_token_id = 1
        eos_token_id = 2
        if self.tokenizer:
            bos_id = self.tokenizer.token_to_id("<|startoftranscript|>")
            if bos_id is not None:
                bos_token_id = bos_id
            eos_id = self.tokenizer.token_to_id("<|endoftranscript|>")
            if eos_id is not None:
                eos_token_id = eos_id

        tokens = [bos_token_id]
        # Maximum tokens proportional to audio duration (~6 tokens/sec)
        max_tokens = max(16, int((len(audio) / 16000.0) * 10))

        # Inspect decoder input names
        dec_inputs = [inp.name for inp in self.decoder_session.get_inputs()]
        input_ids_name = dec_inputs[0] if dec_inputs else "input_ids"
        enc_states_name = (
            dec_inputs[1] if len(dec_inputs) > 1 else "encoder_hidden_states"
        )

        for _ in range(max_tokens):
            inp_ids = np.array([tokens], dtype=np.int64)
            dec_feed = {
                input_ids_name: inp_ids,
                enc_states_name: encoder_hidden_states,
            }

            dec_outputs = self.decoder_session.run(None, dec_feed)
            logits = dec_outputs[0]  # shape: (1, seq_len, vocab_size)
            next_token = int(np.argmax(logits[0, -1, :]))

            if next_token == eos_token_id:
                break
            tokens.append(next_token)

        # Skip start token
        gen_tokens = [t for t in tokens if t not in (bos_token_id, eos_token_id)]
        if self.tokenizer and gen_tokens:
            return self.tokenizer.decode(gen_tokens).strip()

        return ""

    def _transcribe_fallback(self, audio: np.ndarray) -> str:
        """Fallback transcription when running in mock or un-downloaded environment."""
        # Calculate RMS amplitude to verify audio signal exists
        rms = float(np.sqrt(np.mean(audio**2)))
        logger.debug(
            "Moonshine fallback called: audio length=%d samples (%.2f s), RMS=%.4f",
            len(audio),
            len(audio) / 16000.0,
            rms,
        )
        if rms < 1e-4:
            # Silent chunk
            return ""
        return ""
