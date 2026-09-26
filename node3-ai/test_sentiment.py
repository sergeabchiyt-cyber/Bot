import asyncio
import json
from finbert_service import FinBERTService


async def main():
    fb = FinBERTService()
    tests = [
        "The Federal Reserve raised rates by 75 basis points.",
        "The Fed signaled it would slow the pace of rate hikes.",
        "The labor market remains tight and inflation is elevated.",
    ]
    for t in tests:
        print(f"{t}\n  → {fb.predict(t)}\n")


if __name__ == "__main__":
    asyncio.run(main())
