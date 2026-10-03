import unittest

from market_state import ATR_PERIOD, CANDLE_HISTORY_LIMIT, MarketState


class MarketStateTests(unittest.TestCase):
    @staticmethod
    def candle(index, high_delta=2.0):
        open_price = 100.0 + index
        return {
            "time": 1_700_000_000_000 + index * 900_000,
            "open": open_price,
            "high": open_price + high_delta,
            "low": open_price - 1.0,
            "close": open_price + 1.0,
            "volume": 100.0,
            "source": "test",
        }

    def test_caches_only_supported_profile_windows(self):
        state = MarketState()
        for window in ("PW", "PS", "CW"):
            self.assertTrue(
                state.update_levels(
                    {
                        "window": window,
                        "poc": 2340.0,
                        "vah": 2350.0,
                        "val": 2330.0,
                        "sunday_open": 2341.25 if window == "PW" else None,
                    }
                )
            )
        self.assertEqual(set(state.levels), {"PW", "PS", "CW"})
        self.assertFalse(
            state.update_levels(
                {"window": "SWING_BULL", "poc": 2340, "vah": 2350, "val": 2330}
            )
        )
        self.assertEqual(state.levels["PW"]["sunday_open"], 2341.25)

    def test_fifteen_replayed_candles_seed_atr_14(self):
        state = MarketState()
        for index in range(CANDLE_HISTORY_LIMIT):
            self.assertTrue(state.update_candle(self.candle(index)))

        self.assertEqual(len(state.candles), 15)
        self.assertEqual(ATR_PERIOD, 14)
        self.assertAlmostEqual(state.atr_14, 3.0)

    def test_current_bar_revisions_replace_its_true_range(self):
        state = MarketState()
        for index in range(CANDLE_HISTORY_LIMIT):
            state.update_candle(self.candle(index))

        revised = self.candle(CANDLE_HISTORY_LIMIT - 1, high_delta=4.0)
        self.assertTrue(state.update_candle(revised))
        self.assertEqual(len(state.candles), CANDLE_HISTORY_LIMIT)
        self.assertAlmostEqual(state.atr_14, (13 * 3.0 + 5.0) / 14)

        # The next 15m bucket applies Wilder smoothing once; repeated updates
        # to that same bucket must not apply the smoothing a second time.
        next_bar = self.candle(CANDLE_HISTORY_LIMIT)
        self.assertTrue(state.update_candle(next_bar))
        expected = (13 * ((13 * 3.0 + 5.0) / 14) + 3.0) / 14
        self.assertAlmostEqual(state.atr_14, expected)
        self.assertEqual(len(state.candles), CANDLE_HISTORY_LIMIT)
        self.assertTrue(state.update_candle(dict(next_bar, high=next_bar["high"] + 1.0)))
        revised_expected = (13 * ((13 * 3.0 + 5.0) / 14) + 4.0) / 14
        self.assertAlmostEqual(state.atr_14, revised_expected)

    def test_rejects_malformed_candles(self):
        state = MarketState()
        self.assertFalse(state.update_candle({"time": 1, "open": 1}))
        bad = self.candle(0)
        bad["high"], bad["low"] = 99.0, 100.0
        self.assertFalse(state.update_candle(bad))
        self.assertEqual(state.candles, [])


if __name__ == "__main__":
    unittest.main()
