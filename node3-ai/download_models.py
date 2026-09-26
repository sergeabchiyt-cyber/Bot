"""
Model Downloader & Verifier for Node 3
======================================
Automates Step 4 and Step 5 of Node 3 Deployment:
- Downloads Moonshine Small Streaming ONNX INT8 (Mer0vin8ian/moonshine-streaming-small-onnx)
- Downloads FinBERT INT8 (sekarkrishna/finbert-int8)
- Verifies model integrity and file presence
"""

import os
import sys
import logging
from huggingface_hub import snapshot_download, hf_hub_download

logging.basicConfig(level=logging.INFO, format="%(asctime)s [%(levelname)s] %(message)s")
logger = logging.getLogger("download_models")

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


def download_repo(repo_id: str, local_dir: str, expected_files: list) -> bool:
    logger.info("Downloading %s to %s ...", repo_id, local_dir)
    os.makedirs(local_dir, exist_ok=True)

    try:
        snapshot_download(repo_id=repo_id, local_dir=local_dir, local_dir_use_symlinks=False)
    except Exception as e:
        logger.warning("snapshot_download failed for %s (%s). Retrying per-file...", repo_id, e)

    # Verify and per-file retry if needed
    missing = [f for f in expected_files if not os.path.isfile(os.path.join(local_dir, f))]
    if missing:
        logger.warning("Files missing in %s: %s. Attempting individual downloads...", local_dir, missing)
        for fname in missing:
            try:
                logger.info("Downloading individual file %s/%s ...", repo_id, fname)
                hf_hub_download(repo_id=repo_id, filename=fname, local_dir=local_dir)
            except Exception as e:
                logger.error("Failed to download %s: %s", fname, e)

    # Final check
    still_missing = [f for f in expected_files if not os.path.isfile(os.path.join(local_dir, f))]
    if still_missing:
        logger.error("Validation failed for %s. Missing: %s", repo_id, still_missing)
        return False

    logger.info("All expected files verified for %s:", repo_id)
    for fname in expected_files:
        fpath = os.path.join(local_dir, fname)
        size_mb = os.path.getsize(fpath) / (1024 * 1024)
        logger.info("  ✓ %s (%.2f MB)", fname, size_mb)
    return True


def main():
    curr_dir = os.path.dirname(os.path.abspath(__file__))
    models_dir = os.path.join(curr_dir, "models")
    os.makedirs(models_dir, exist_ok=True)

    moonshine_dir = os.path.join(models_dir, "moonshine-streaming-onnx")
    finbert_dir = os.path.join(models_dir, "finbert-int8")

    logger.info("Starting model downloads for Node 3...")
    ms_ok = download_repo("Mer0vin8ian/moonshine-streaming-small-onnx", moonshine_dir, EXPECTED_MOONSHINE_FILES)
    fb_ok = download_repo("sekarkrishna/finbert-int8", finbert_dir, EXPECTED_FINBERT_FILES)

    if ms_ok and fb_ok:
        logger.info("All models downloaded and verified successfully!")
        sys.exit(0)
    else:
        logger.warning("Some model files could not be downloaded in this environment. Ensure network access to huggingface.co.")
        sys.exit(1)


if __name__ == "__main__":
    main()
