"""Small, dependency-free cache for Node 1 market state on the WS feed.

Node 1 replays PW/PS/CW levels and the latest ATR_PERIOD + 1 candles after a
subscription. Keeping candle bars keyed by their bucket timestamp makes live
in-progress updates replace the current bar instead of double-counting it.
"""

import math
from typing import Any, Dict, List, Optional

ATR_PERIOD = 14
CANDLE_HISTORY_LIMIT = ATR_PERIOD + 1
LEVEL_WINDOWS = frozenset(("PW", "PS", "CW"))


class MarketState:
    """Latest supported levels, recent 15m candles, and a Wilder ATR(14)."""

    def __init__(self) -> None:
        self.levels: Dict[str, Dict[str, Any]] = {}
        self._candles_by_time: Dict[int, Dict[str, Any]] = {}
        self._latest_time: Optional[int] = None
        self.atr_14: Optional[float] = None
        # ATR before the currently forming latest candle. Revisions to that
        # candle then replace its true range instead of applying Wilder's
        # smoothing repeatedly to the same 15-minute bar.
        self._atr_before_latest: Optional[float] = None

    @staticmethod
    def _number(value: Any) -> Optional[float]:
        try:
            number = float(value)
        except (TypeError, ValueError, OverflowError):
            return None
        return number if math.isfinite(number) else None

    @property
    def candles(self) -> List[Dict[str, Any]]:
        """Recent candles in ascending bucket-time order."""
        return [self._candles_by_time[t] for t in sorted(self._candles_by_time)]

    def update_levels(self, data: Any) -> bool:
        """Cache a PW/PS/CW frame; ignore swing windows and malformed payloads."""
        if not isinstance(data, dict):
            return False
        window = data.get("window")
        if not isinstance(window, str) or window not in LEVEL_WINDOWS:
            return False

        values = {key: self._number(data.get(key)) for key in ("poc", "vah", "val")}
        if any(value is None for value in values.values()):
            return False

        level = dict(data)
        level.update(values)
        if "sunday_open" in level:
            sunday_open = self._number(level["sunday_open"])
            if sunday_open is not None:
                level["sunday_open"] = sunday_open
            else:
                level.pop("sunday_open", None)
        self.levels[window] = level
        return True

    @staticmethod
    def _true_range(candle: Dict[str, Any], previous_close: float) -> float:
        return max(
            candle["high"] - candle["low"],
            abs(candle["high"] - previous_close),
            abs(candle["low"] - previous_close),
        )

    def _seed_atr(self, candles: List[Dict[str, Any]]) -> Optional[float]:
        if len(candles) < CANDLE_HISTORY_LIMIT:
            return None
        ranges = [
            self._true_range(candles[i], candles[i - 1]["close"])
            for i in range(1, len(candles))
        ]
        if len(ranges) < ATR_PERIOD:
            return None
        # Wilder's initial ATR is the arithmetic mean of its first 14 true
        # ranges. Node 1's 15-bar replay is exactly enough to seed this value.
        return sum(ranges[-ATR_PERIOD:]) / ATR_PERIOD

    def update_candle(self, data: Any) -> bool:
        """Insert/replace an OHLC bar and refresh the live Wilder ATR(14)."""
        if not isinstance(data, dict):
            return False

        raw_time = self._number(data.get("time"))
        if raw_time is None or raw_time <= 0 or not raw_time.is_integer():
            return False
        timestamp = int(raw_time)

        prices = {key: self._number(data.get(key)) for key in ("open", "high", "low", "close")}
        if any(value is None for value in prices.values()):
            return False
        if prices["high"] < prices["low"]:
            return False

        candle: Dict[str, Any] = {"time": timestamp}
        candle.update(prices)
        previous_latest = self._latest_time
        self._candles_by_time[timestamp] = candle

        # Keep only what's needed for the 14 true ranges (15 OHLC bars).
        ordered = self.candles
        for old_time in sorted(self._candles_by_time)[:-CANDLE_HISTORY_LIMIT]:
            del self._candles_by_time[old_time]
        ordered = self.candles
        latest_time = ordered[-1]["time"]
        self._latest_time = latest_time

        if len(ordered) < CANDLE_HISTORY_LIMIT:
            self.atr_14 = None
            self._atr_before_latest = None
            return True

        if previous_latest is None or timestamp > previous_latest:
            if self.atr_14 is None:
                self.atr_14 = self._seed_atr(ordered)
                self._atr_before_latest = None
            else:
                previous_close = ordered[-2]["close"]
                current_range = self._true_range(ordered[-1], previous_close)
                self._atr_before_latest = self.atr_14
                self.atr_14 = (
                    (ATR_PERIOD - 1) * self.atr_14 + current_range
                ) / ATR_PERIOD
        elif timestamp == latest_time:
            if self._atr_before_latest is None:
                # The latest bar is still part of the initial replay/seed.
                self.atr_14 = self._seed_atr(ordered)
            else:
                current_range = self._true_range(ordered[-1], ordered[-2]["close"])
                self.atr_14 = (
                    (ATR_PERIOD - 1) * self._atr_before_latest + current_range
                ) / ATR_PERIOD
        else:
            # An out-of-order correction can change the prior-close input.
            # Reseed from the bounded history rather than retaining a stale ATR.
            self.atr_14 = self._seed_atr(ordered)
            self._atr_before_latest = None

        return True
