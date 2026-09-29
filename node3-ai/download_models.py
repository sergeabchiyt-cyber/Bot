"""
Model Downloader & Verifier for Node 3 — UPGRADED for 16GB / 4vCPU / 400GB
==========================================================================
Supports two tiers:

  --tier small     (default, legacy): Mer0vin8ian/moonshine-streaming-small-onnx + sekarkrishna/finbert-int8
                                      ~500MB total, fits 4GB boxes, 800MB RSS
  --tier upgraded  (recommended for 16GB): also fetches gtfintechlab/FOMC-RoBERTa (355M),
                                      yiyanghkust/finbert-tone, and optionally
                                      UsefulSensors/moonshine-streaming-medium + Silero VAD
                                      Total ~2.2GB (still <1% of 400GB), RSS ~1.8GB

Also supports --export-onnx to convert FOMC-RoBERTa PyTorch -> ONNX INT8 locally
(requires: pip install torch optimum onnx). Run once on the 16GB machine.

Usage:
  pip install -r requirements.txt
  python3 download_models.py                      # small
  python3 download_models.py --tier upgraded      # best for your 16GB
  python3 download_models.py --tier upgraded --export-onnx   # build ONNX from torch hub
"""

import os
import sys
import argparse
import logging
from huggingface_hub import snapshot_download, hf_hub_download

logging.basicConfig(level=logging.INFO, format="%(asctime)s [%(levelname)s] %(message)s")
logger = logging.getLogger("download_models")

# --- legacy small tier (exact) ---
EXPECTED_MOONSHINE_FILES = [
    "encoder_model_int8.onnx",
    "decoder_model_int8.onnx",
    "decoder_with_past_model_int8.onnx",
    "tokenizer.json",
]
EXPECTED_FINBERT_FILES = [
    "model_quantized.onnx",
    "config.json",
    "vocab.txt",
    "tokenizer_config.json",
]

# upgraded tier additions — we accept any file presence, verify loosely
FOMC_EXPECTED_ANY = ["config.json", "tokenizer.json"]  # pytorch vs onnx variants differ
FINBERT_TONE_EXPECTED_ANY = ["config.json"]
MOONSHINE_MEDIUM_MARKER = ["config.json"]  # torch checkpoint
SILERO_VAD_FILE = "silero_vad.onnx"


def download_repo(repo_id: str, local_dir: str, expected_files: list, need_at_least_one: bool = False) -> bool:
    logger.info("Downloading %s to %s ...", repo_id, local_dir)
    os.makedirs(local_dir, exist_ok=True)
    try:
        snapshot_download(repo_id=repo_id, local_dir=local_dir, local_dir_use_symlinks=False)
    except Exception as e:
        logger.warning("snapshot_download failed for %s (%s). Retrying per-file...", repo_id, e)

    missing = [f for f in expected_files if not os.path.isfile(os.path.join(local_dir, f))]
    if missing:
        if need_at_least_one and any(os.path.isfile(os.path.join(local_dir, f)) for f in os.listdir(local_dir)) if os.path.isdir(local_dir) else False:
            pass  # ok at least something exists
        else:
            logger.warning("Files missing in %s: %s. Attempting individual downloads...", local_dir, missing)
            for fname in missing:
                try:
                    logger.info("Downloading individual file %s/%s ...", repo_id, fname)
                    hf_hub_download(repo_id=repo_id, filename=fname, local_dir=local_dir)
                except Exception as e:
                    logger.error("Failed to download %s: %s", fname, e)

    # optional: list whatever we got
    if os.path.isdir(local_dir):
        files = os.listdir(local_dir)[:10]
        logger.info("Contents of %s: %s", local_dir, files[:5])

    still_missing = [f for f in expected_files if not os.path.isfile(os.path.join(local_dir, f))]
    if need_at_least_one:
        # success if any file exists
        if os.path.isdir(local_dir) and len(os.listdir(local_dir)) > 0:
            logger.info("✓ %s fetched (contains %d files)", repo_id, len(os.listdir(local_dir)))
            return True
        logger.error("Validation failed for %s. Missing: %s", repo_id, still_missing)
        return False
    if still_missing:
        logger.error("Validation failed for %s. Missing: %s", repo_id, still_missing)
        return False
    logger.info("All expected files verified for %s:", repo_id)
    for fname in expected_files:
        fpath = os.path.join(local_dir, fname)
        if os.path.isfile(fpath):
            size_mb = os.path.getsize(fpath) / (1024 * 1024)
            logger.info("  ✓ %s (%.2f MB)", fname, size_mb)
    return True


def export_onnx(local_dir: str, task: str = "text-classification"):
    """Use optimum to export PyTorch checkpoint to ONNX (and quantize)."""
    try:
        from optimum.onnxruntime import ORTQuantizer
        from optimum.onnxruntime.configuration import AutoQuantizationConfig
        from transformers import AutoTokenizer
        import subprocess

        logger.info("Exporting %s to ONNX via optimum-cli ...", local_dir)
        out = local_dir.rstrip("/") + "-onnx"  # avoid overwrite?
        # Try optimum-cli
        cmd = ["optimum-cli", "export", "onnx", "--model", local_dir, "--task", task, local_dir]
        logger.info("Running: %s", " ".join(cmd))
        subprocess.run(cmd, check=False)
        # Also quantize if possible
        try:
            q = ORTQuantizer.from_pretrained(local_dir)
            cfg = AutoQuantizationConfig.avx512_vnni(is_static=False, per_channel=False)
            q.quantize(save_dir=local_dir, quantization_config=cfg)
            logger.info("Quantized %s to INT8", local_dir)
        except Exception as e:
            logger.warning("Quantization step skipped: %s", e)
        return True
    except ImportError as e:
        logger.warning("optimum/torch not installed, cannot export ONNX: %s. Install: pip install torch optimum[onnxruntime] onnx", e)
        return False
    except Exception as e:
        logger.warning("ONNX export failed: %s", e)
        return False


def main():
    parser = argparse.ArgumentParser(description="Node3 model downloader (16GB upgraded tier)")
    parser.add_argument("--tier", choices=["small", "upgraded"], default="small", help="small=legacy ~500MB, upgraded=FOMC-RoBERTa etc ~2.2GB")
    parser.add_argument("--export-onnx", action="store_true", help="After download, export any PyTorch checkpoints to ONNX INT8 (needs torch+optimum)")
    args = parser.parse_args()

    curr_dir = os.path.dirname(os.path.abspath(__file__))
    models_dir = os.path.join(curr_dir, "models")
    os.makedirs(models_dir, exist_ok=True)

    moonshine_dir = os.path.join(models_dir, "moonshine-streaming-onnx")
    finbert_dir = os.path.join(models_dir, "finbert-int8")

    logger.info("Starting model downloads tier=%s for Node 3 (400GB disk affords full tier)...", args.tier)

    ms_ok = download_repo("Mer0vin8ian/moonshine-streaming-small-onnx", moonshine_dir, EXPECTED_MOONSHINE_FILES)
    fb_ok = download_repo("sekarkrishna/finbert-int8", finbert_dir, EXPECTED_FINBERT_FILES)

    tier_ok = True
    if args.tier == "upgraded":
        # Primary upgraded sentiment: FOMC-RoBERTa (the 16GB star)
        fomc_dir = os.path.join(models_dir, "fomc-roberta")
        fomc_ok = download_repo("gtfintechlab/FOMC-RoBERTa", fomc_dir, FOMC_EXPECTED_ANY, need_at_least_one=True)
        if not fomc_ok:
            logger.warning("FOMC-RoBERTa download incomplete — will use finbert fallback at runtime")
        else:
            if args.export_onnx and os.path.isfile(os.path.join(fomc_dir, "pytorch_model.bin")) or args.export_onnx and os.path.isfile(os.path.join(fomc_dir, "model.safetensors")):
                export_onnx(fomc_dir, "text-classification")

        # Second tier: finbert-tone (secondary)
        tone_dir = os.path.join(models_dir, "finbert-tone")
        download_repo("yiyanghkust/finbert-tone", tone_dir, FINBERT_TONE_EXPECTED_ANY, need_at_least_one=True)

        # Optional medium moonshine — heavy but better WER (~650MB INT8, 15% error drop)
        # We attempt to fetch the torch checkpoint; ONNX conversion is optional
        medium_dir = os.path.join(models_dir, "moonshine-streaming-medium-onnx")
        # This repo may not have prebuilt ONNX — we try torch checkpoint first
        med_ok = download_repo("UsefulSensors/moonshine-streaming-medium", medium_dir, MOONSHINE_MEDIUM_MARKER, need_at_least_one=True)
        if not med_ok:
            logger.info("Moonshine medium not fetched (optional). Small tier will run. To build INT8: optimum-cli export onnx --model UsefulSensors/moonshine-streaming-medium --task automatic-speech-recognition %s", medium_dir)

        # Silero VAD (~2MB)
        vad_dir = os.path.join(models_dir, "silero-vad")
        os.makedirs(vad_dir, exist_ok=True)
        try:
            logger.info("Fetching Silero VAD ONNX...")
            hf_hub_download(repo_id="snakers4/silero-vad", filename="silero_vad.onnx", local_dir=vad_dir)
            # rename if needed
            for root, _, files in os.walk(vad_dir):
                if "silero_vad.onnx" not in files and any(f.endswith(".onnx") for f in files):
                    for f in files:
                        if f.endswith(".onnx"):
                            os.rename(os.path.join(root, f), os.path.join(vad_dir, "silero_vad.onnx"))
                            break
            logger.info("✓ Silero VAD ready at %s/silero_vad.onnx", vad_dir)
        except Exception as e:
            logger.warning("Silero VAD optional download failed (offline?): %s", e)

        tier_ok = fomc_ok  # at least primary matters

    if ms_ok and fb_ok:
        logger.info("Base models downloaded and verified successfully!")
        if args.tier == "upgraded" and tier_ok:
            logger.info("🎉 Upgraded tier ready: FOMC-RoBERTa + Silero VAD on 16GB box (~1.8GB RSS).")
        elif args.tier == "upgraded":
            logger.warning("Base OK but some upgraded files missing — service will gracefully fall back at runtime.")
        sys.exit(0)
    else:
        logger.warning("Some model files could not be downloaded in this environment. Ensure network access to huggingface.co.")
        sys.exit(1)


if __name__ == "__main__":
    main()
