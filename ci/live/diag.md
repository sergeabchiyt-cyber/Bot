# SiftingIO Spec Extract

## 1. llms-full.txt (200, 58006 chars)
# SiftingIO — full LLM context

Market data APIs for stocks, forex, crypto, commodities, and on-chain venues. REST + WebSocket, multi-venue aggregation, sub-100ms streams, 99.9% SLA.

Where multiple independent sources are available, SiftingIO aggregates market data across them and normalizes it into one unified JSON schema, so supported asset classes share a consistent set of fields, auth model, and integration flow over REST and WebSocket. This file is the extended, auto-updating index; the concise version lives at https://sifting.io/llms.txt.

## Key links

- Website: https://sifting.io
- Documentation: https://sifting.io/docs
- Pricing: https://sifting.io/pricing  (machine-readable: https://sifting.io/pricing.md)
- OpenAPI (REST) spec: https://sifting.io/openapi.yaml
- AsyncAPI (WebSocket) spec: https://sifting.io/asyncapi.yaml
- Postman workspace: https://www.postman.com/siftingio/siftingio-market-data-api
- Contact: hello@sifting.io

## SDKs

- Go: https://sifting.io/sdks/go
- Python: https://sifting.io/sdks/python
- JavaScript: https://sifting.io/sdks/javascript

## Integrations

- Market data MCP server: https://sifting.io/integrations/mcp
  An open-source market data MCP server that connects Claude, Cursor, or VS Code to real-time and historical prices over the Model Context Protocol. The client launches it with `npx -y siftingio-mcp` plus one JSON config block; the server runs on the user's machine and reads the key from the SIFTING_API_KEY environment variable. 36 tools span forex, crypto, stocks, commodities, SEC fundamentals (filings, XBRL financials, insider and 13F holdings), market hours, and on-chain DEX data, returning the same normalized shapes as the REST API. A free API key works: 10,000 REST calls a month, 60 requests a minute, full historical depth. MIT-licensed, available in TypeScript and Python.
- n8n community node: https://sifting.io/integrations/n8n
  Pull live prices, historical bars, fundamentals, and filings into low-code n8n workflows, or wire SiftingIO up as a tool for an AI Agent.
- Supabase Foreign Data Wrapper: https://sifting.io/integrations/supabase
  Query live and historical market data directly from Postgres as SQL foreign tables, with no ETL pipeline to maintain.

## Pricing summary

- Crypto: Builder $99/mo · Pro $299/mo · Ultra $799/mo (USD, monthly; ~20% off annual)
- DEX & DeFi: Builder $129/mo · Pro $399/mo · Ultra $1099/mo (USD, monthly; ~20% off annual)
- Forex: Builder $99/mo · Pro $299/mo · Ultra $799/mo (USD, monthly; ~20% off annual)
- Commodities: Builder $99/mo · Pro $299/mo · Ultra $799/mo (USD, monthly; ~20% off annual)
- US Stocks: Builder $129/mo · Pro $399/mo · Ultra $1099/mo (USD, monthly; ~20% off annual)

Bundle discounts (Pro/Ultra, auto-applied): 2 markets 10% off, 3 markets 15% off, 4 markets 20% off, 5 markets 25% off.
Full machine-readable pricing: https://sifting.io/pricing.md
Pricing page: https://sifting.io/pricing

## API endpoints

### Discovery
- GET /v1/fnd/stocks/search — Ticker / company search: Substring lookup over the SEC ticker registry (~13k US issuers). Matches against ticker (prefix) and company name.
  https://sifting.io/docs/discovery/search
- GET /v1/fnd/stocks/:ticker/profile — Company profile: Name, exchange listings, SIC industry classification, fiscal year end, and entity type for a single issuer.
  https://sifting.io/docs/discovery/company

### SEC filings
- GET /v1/fnd/stocks/:ticker/filings — Recent SEC filings: Up to 1000 most recent filings across all form types for a company. Filterable by form and filing-date window.
  https://sifting.io/docs/filings/list
- GET /v1/fnd/stocks/:ticker/filings/:accession — Single filing detail: Get one SEC filing's metadata by accession number: form type, filing date, primary document URL, and the full file manifest from its EDGAR archive folder.
  https://sifting.io/docs/filings/get
- GET /v1/fnd/stocks/:ticker/events — 8-K material events: Get a company's 8-K filings for material corporate events, from earnings releases to officer changes. Optionally filter by item code (substring match).
  https://sifting.io/docs/filings/events
- GET /v1/fnd/stocks/:ticker/ownership — Schedule 13D / 13G beneficial ownership: Get beneficial-ownership filings, plus amendments, for every investor crossing the 5% threshold. Each row includes the form type, accession number, filing date, and a canonical link to the SEC document.
  https://sifting.io/docs/filings/ownership
- GET /v1/fnd/stocks/:ticker/compensation — DEF 14A proxy statements: Get DEF 14A proxy statements, including amendments and additional definitive material. Each row includes the form type, accession number, filing date, and a canonical link to the SEC document.
  https://sifting.io/docs/filings/compensation
- GET /v1/fnd/stocks/:ticker/earnings — Earnings release history: Every 8-K filed with item 2.02 ('Results of Operations and Financial Condition') for this company, the canonical timeline of when this company has reported earnings.
  https://sifting.io/docs/filings/earnings

### Filing text & analysis
- GET /v1/fnd/stocks/:ticker/filings/:accession/sections — Extract every section: Returns every standard 10-K / 10-Q item we could extract from a filing's primary HTML, as clean text. Missing keys mean we couldn't locate that section in this particular filing, extraction is best-effort, SEC HTML formatting varies.
  https://sifting.io/docs/filing-text/all-sections
- GET /v1/fnd/stocks/:ticker/filings/:accession/sections/:section — Extract one section: Same data as /sections but returns just one section. Smaller payload, drop straight into a single LLM prompt.
  https://sifting.io/docs/filing-text/section
- GET /v1/fnd/stocks/:ticker/risk-factors-diff — Risk Factors year-over-year diff: Compares the latest 10-K's Risk Factors against the prior year's. Paragraph-level diff with three buckets: added, removed, modified, the killer 'what changed in this company's risk profile' answer without analyst-hours of reading.
  https://sifting.io/docs/filing-text/risk-factors-diff

### Financials (XBRL)
- GET /v1/fnd/stocks/:ticker/financials — Full XBRL bundle: Every reported XBRL concept for a company across every period and unit. Payload is large (5+ MB for a mature filer); strictly requires Accept-Encoding: gzip.
  https://sifting.io/docs/financials/all
- GET /v1/fnd/stocks/:ticker/financials/:concept — Single concept across periods: One XBRL concept across every reported period for a company. Smaller than the full bundle but still gzip-required.
  https://sifting.io/docs/financials/concept
- GET /v1/fnd/stocks/screener/:concept/:period — Cross-sectional screener: Every filer's reported value for one concept in one period. The screener primitive, answer 'who reported the highest Revenue in Q1 2024?' in a single call.
  https://sifting.io/docs/financials/screener
- GET /v1/fnd/stocks/:ticker/ratios — Fundamental ratios: Standard fundamental ratios computed from XBRL data, across every reported period (newest-first). Pure XBRL, no stock-price-dependent ratios (P/E, P/B, EV/EBITDA) since this version doesn't carry equity prices.
  https://sifting.io/docs/financials/ratios

### Insiders & holdings
- GET /v1/fnd/stocks/:ticker/insiders — Insider transactions (Form 3 / 4 / 5): Insider ownership filings parsed into a flat row-per-transaction view. Each Form 4 produces one or more rows, buys, sales, grants, option exercises, gifts, and tax withholdings.
  https://sifting.io/docs/holdings/insiders
- GET /v1/fnd/filers/:filer/holdings — 13F-HR institutional holdings: Latest 13F-HR positions for an institutional manager. Accepts a ticker (if the manager is also a public issuer) or a CIK (for managers not in the SEC ticker map).
  https://sifting.io/docs/holdings/13f

### Economic calendar
- GET /v1/fnd/economic-calendar — Upcoming US economic events: Schedule of upcoming US economic releases, CPI, NFP, FOMC, GDP, etc., sourced from official agency calendars. Filter by date range, impact tier, agency, or specific event_id. All parameters are optional; defaults return the next 30 days of US events ordered by scheduled_at.
  https://sifting.io/docs/economic-calendar/list

### Market hours & calendars
- GET /v1/fnd/markets — List markets: The catalog of every supported market: slug, display name, type (exchange / forex / crypto), IANA timezone, region, and the constituent ISO 10383 MIC codes for equity markets.
  https://sifting.io/docs/markets/list
- GET /v1/fnd/markets/status — All-markets status snapshot: Open / closed snapshot for every market in one call. The 'is the US market open?' question for a dashboard widget: one round trip, one response, returned in catalog order.
  https://sifting.io/docs/markets/status
- GET /v1/fnd/markets/:market/status — Single market status: The same status object as the all-markets endpoint, but data is a single object instead of an array. Use this when you only care about one venue.
  https://sifting.io/docs/markets/status-one
- GET /v1/fnd/markets/:market/hours — Weekly hours: The recurring weekly schedule for a market. Times are venue-local (clients convert via the timezone field). The shape varies by market type: exchanges report regular / pre / post sessions plus any lunch breaks; forex reports the weekly open/close anchor and four UTC session windows; crypto has no schedule.
  https://sifting.io/docs/markets/hours
- GET /v1/fnd/markets/:market/calendar — Holiday calendar: Holidays and half-days for an exchange in a date range. Built for backtesting clients ('which days was the market closed in 2023?') and dashboard 'next holiday' widgets. Forex and crypto return an empty array. They don't observe holidays.
  https://sifting.io/docs/markets/calendar

### Historical OHLCV
- GET /v1/hist/stocks/:ticker/bars — US equities bars: Paginated OHLCV bars for a US-listed security, from 1-minute intraday through monthly. Each bar carries t (Unix epoch milliseconds, marking the bucket's open time, same convention as live ticks), o/h/l/c as float prices, and v as integer share volume. Coverage is regular US trading hours (09:30–16:00 ET, Mon–Fri, excluding US market holidays); half-day sessions emit fewer intraday bars; closed sessions emit none.
  https://sifting.io/docs/historical/stocks
- GET /v1/hist/forex/:pair/bars — Forex bars: Paginated OHLCV bars for a forex pair, from 1-minute through monthly. Each bar carries t (Unix epoch milliseconds, UTC, marking the bucket's open time), o/h/l/c as historical prices, and v as the source's reported tick count or 0 when unavailable. This is not traded currency volume. Price basis can differ across sources. All time inputs are UTC; closed market periods may have no bars.
  https://sifting.io/docs/historical/forex
- GET /v1/hist/crypto/:symbol/bars — Crypto bars: Paginated OHLCV bars for a USD-quoted crypto asset, from 1-minute through monthly. Each bar carries t (Unix epoch milliseconds, UTC, marking the bucket's open time), o/h/l/c as float prices, and v as the base-asset volume (e.g. 12.345 BTC). v is a float, so fractional volumes are preserved. Note that on /hist/stocks v is whole shares and on /hist/forex v is a reported tick count or 0 when unavailable; the wire shape is shared, the semantics differ per asset class. All time inputs are UTC.
  https://sifting.io/docs/historical/crypto
- GET /v1/hist/dex/:symbol/bars — DEX bars: Paginated OHLCV bars for a USD-quoted DEX token, from 1-minute through monthly. Each bar carries t (Unix epoch milliseconds, UTC, marking the bucket's open time), o/h/l/c as realised swap prices within the bucket, and v as the base-asset volume (e.g. ETH swapped, not USD value). Buckets with zero trades are omitted entirely: the series is sparse during low-activity periods, and clients drawing charts should treat a missing minute as 'no data' rather than a zero-priced bar.
  https://sifting.io/docs/historical/dex
- GET /v1/hist/commodities/:symbol/bars — Commodities bars: Paginated OHLCV bars for a commodity, from 1-minute through monthly. Each bar carries t (Unix epoch milliseconds, UTC, marking the bucket's open time), o/h/l/c as float prices, and v as the tick-count for the bucket. Commodities quote against USD on a rolling near-24/7 calendar, so all time inputs are read as UTC.
  https://sifting.io/docs/historical/commodities

### Live market data
- GET /v1/last/trade/:venue/:symbol — Last trade: Get the latest trade data for supported stocks, FX, crypto, commodities, and DEX markets, including price, size, symbol, and timestamp.
  https://sifting.io/docs/live/trade
- GET /v1/last/quote/:venue/:symbol — Last top-of-book quote: Get the latest top-of-book quote for supported stocks, FX, crypto, commodities, and DEX markets, including best bid, best ask, sizes, symbol, and timestamp.
  https://sifting.io/docs/live/quote
- GET /v1/snapshot/:venue — Market snapshot: Get a full market snapshot across stocks, FX, crypto, commodities, and DEX, every symbol a venue publishes in a single round-trip, or scope it down to your own watchlist. Each tick carries price, sizes, best bid, best ask, symbol, and timestamp.
  https://sifting.io/docs/live/snapshot
- GET /v1/last/close/:venue/:symbol — Previous daily close: Get the close price for the most recent completed trading day across stocks, crypto, FX, and commodities, including the close, the UTC day it belongs to, symbol, and timestamp.
  https://sifting.io/docs/live/previous-close
- GET /v1/last/movers/:venue — Market movers: Get the top gainers and top losers across stocks, crypto, FX, and commodities, ranked by percentage change since the last daily close, including symbol, current price, previous close, change, percent change, and day volume. One request returns both lists, or just one with direction.
  https://sifting.io/docs/live/movers
- GET /v1/last/most-active/:venue — Most active: Get the most actively traded US stocks and crypto assets today, ranked by volume, highest first, including symbol, current price, day volume, previous day volume, previous close, change, and percent change.
  https://sifting.io/docs/live/most-active
- GET /v1/last/eod/:venue/:symbol — End-of-day (EOD) data: Get the latest end-of-day (EOD) price for a stock, forex pair, crypto asset, or commodity. Returns the previous completed trading day's daily OHLCV bar: open, high, low, close, volume, the UTC date, and a timestamp. One request returns one bar, with no date ranges, intervals, or pagination to manage.
  https://sifting.io/docs/live/eod
- GET /v1/last/tvl/:chain/:pair — Aggregated TVL · per chain, per pair: Total value locked for a specific token pair on a specific chain. Sums across every pool MarketEngine tracks for that pair on that chain (concentrated-liquidity AMMs across fee tiers, stable-pair AMMs, etc.).
  https://sifting.io/docs/live/tvl

### Technical signals
- GET /v1/last/signals/:venue/:symbol — Live signal: Get the current technical-analysis signal for a stock, FX pair, crypto asset, or commodity: a decision-ready summary (strong_sell to strong_buy) with a score from -1 to +1, plus the oscillator and moving-average indicators that produced it, each with its value and vote. Indicators still in warmup are omitted rather than reported as neutral, so the vote counts always sum to the rows actually returned.
  https://sifting.io/docs/signals/live
- GET /v1/hist/:venue/:symbol/signals — Signal history: Get the technical-analysis signal as a time series, one point per bar: summary, score, close, vote counts, the full oscillator and moving-average breakdown with each indicator's value and vote, and any discrete events (golden_cross, death_cross, macd_cross_up, macd_cross_down) that fired on that bar. Same computation as the live signal, so a historical point is identical to the one that fired live: backtest what you saw. t is the bar-open time in Unix epoch milliseconds, the same as the bars routes.
  https://sifting.io/docs/signals/history

### Currency converter
- GET /v1/convert/:from/:to — FX / Crypto Converter: Convert any amount between supported forex and crypto symbols at the current market rate. Returns the converted amount, the rate applied, and the timestamp of the underlying market price so clients can see how fresh the conversion is.
  https://sifting.io/docs/convert/rate

### DEX wallets
- GET /v1/fnd/dex/wallet/:chain/:address — Wallet portfolio: Native-coin and ERC-20 token holdings for one wallet on one EVM chain. The native row only appears when the wallet has a non-zero native balance.
  https://sifting.io/docs/wallets/portfolio

## Blog

- US Stock and ETF Data APIs: Prices, History and NAV [US Equities] (2026-10-26)
  https://sifting.io/blog/us-stock-etf-data-api-prices-history-nav
  Understand US stock and ETF API data: real-time prices, 25+ years of stock history, NAV, distributions and the difference between price return and total return.
- US Stocks API: Build a Research Page with Prices, History and Filings [US Equities] (2026-10-25)
  https://sifting.io/blog/us-stocks-api-research-page-prices-history-filings
  Build a US stock research page with API prices, historical OHLCV and SEC filings. Includes cURL examples, panel design and clear timestamp handling.
- Bollinger Bands in Python: OHLCV Data and Live Prices [Developer Tutorials] (2026-10-23)
  https://sifting.io/blog/bollinger-bands-python-ohlcv-live-prices
  Calculate Bollinger Bands in Python with pandas, verify the math, plot OHLCV closes and add live WebSocket prices without confusing ticks with candles.
- Candlestick Chart in Python with Plotly and OHLCV Data [Developer Tutorials] (2026-10-22)
  https://sifting.io/blog/candlestick-chart-python-plotly-ohlcv-data
  Build a Python candlestick chart with Plotly and Bitcoin OHLCV data. Add volume, handle UTC timestamps, and save an interactive HTML chart with SiftingIO.
- Gold-silver ratio in Python: calculate it from API prices [Commodities] (2026-10-21)
  https://sifting.io/blog/gold-silver-ratio-python-api-prices
  Calculate the gold-silver ratio with Python and commodity price data. Match timestamps, preserve missing bars and avoid misleading ratio candles.
- Forex API JSON: build a multi-pair watchlist in JavaScript [Developer Tutorials] (2026-10-20)
  https://sifting.io/blog/forex-api-json-multi-pair-watchlist-javascript
  Build a forex watchlist from one SiftingIO JSON snapshot. Use Node.js to request multiple pairs, preserve missing rows and label timestamps correctly.
- Normalize prices to 100 in Python: compare market data [Developer Tutorials] (2026-10-19)
  https://sifting.io/blog/normalize-prices-to-100-python-compare-market-data
  Compare forex, gold, stocks and crypto on one chart. Rebase prices to 100 in Python with a shared baseline, tested pandas code and clear handling of gaps.
- Market data API caching: TTL, cache keys and shared requests [Developer Tutorials] (2026-10-18)
  https://sifting.io/blog/market-data-api-caching-ttl-cache-keys-request-coalescing
  Cache market data API responses with a tested Node.js example. Learn TTL, request coalescing, safe cache keys and why cache age is not price age.
- Currency conversion in JavaScript: rates, rounding and formatting [Developer Tutorials] (2026-10-17)
  https://sifting.io/blog/currency-conversion-javascript-rates-rounding-intl-numberformat
  Build a JavaScript currency converter with direct and inverse FX rates, exact decimal rounding and Intl.NumberFormat. Includes code, examples and tests.
- Stop-loss and take-profit in the same candle: OHLC backtests [Quant Research & Backtesting] (2026-10-17)
  https://sifting.io/blog/stop-loss-take-profit-same-candle-ohlc-backtest
  Detect same-candle stop-loss and take-profit ambiguity in Python, compare fill assumptions, and see when finer OHLCV data can resolve the sequence.
- Historical forex data in CSV and Excel: a practical guide [Forex & Crypto] (2026-10-16)
  https://sifting.io/blog/download-historical-forex-data-csv-excel
  Download historical forex data as CSV or Excel. Start with EURUSD, check UTC timestamps and candle completeness, then calculate price changes in a worksheet.
- Market data monitoring with Prometheus: detect old prices and silent feeds [Developer Tutorials] (2026-10-13)
  https://sifting.io/blog/market-data-feed-monitoring-prometheus-tick-age-reconnects-limits
  Monitor tick age, silent feeds and failed market-status polls with a small Python metrics adapter, Prometheus alerts and a credential-free demo.
- Align stock and crypto returns in Python: use the same closing time [Quant Research & Backtesting] (2026-10-11)
  https://sifting.io/blog/align-daily-ohlcv-bars-us-stocks-crypto-session-day-vs-utc-day
  Compare stock and crypto close-to-close returns on one session calendar. A tested pandas example handles DST, early closes and missing observations.
- API key 401 Unauthorized: fix headers, keys and access errors [Developer Tutorials] (2026-10-09)
  https://sifting.io/blog/api-key-401-unauthorized-missing-key-bearer-header-invalid-key
  Diagnose SiftingIO API key errors with a 401/403 comparison table, curl checks and a Python test that keeps credentials out of logs.
- Home Assistant stock price sensor: REST and YAML setup [Dashboards & Frontend] (2026-10-07)
  https://sifting.io/blog/home-assistant-stock-price-sensor-rest-yaml-us-stocks
  Build a Home Assistant stock price sensor with YAML, market-hours polling and freshness labels. Includes a request budget for a three-stock watchlist.
- Five DuckDB recipes for market data CSV exports [Developer Tutorials] (2026-10-05)
  https://sifting.io/blog/duckdb-market-data-csv-five-sql-recipes
  Check OHLCV CSV coverage, timestamp gaps, UTC daily bars, close returns and rolling means with five DuckDB SQL recipes.
- Google Sheets market data template: a watchlist you can copy [Developer Tutorials] (2026-10-04)
  https://sifting.io/blog/google-sheets-market-data-watchlist-template
  Copy a Google Sheets market data template to track up to 20 stock, crypto, forex and commodity prices with a SiftingIO API key, plus troubleshooting.
- Move a Pipedream price alert to n8n [Developer Tutorials] (2026-10-03)
  https://sifting.io/blog/migrate-pipedream-price-alert-to-n8n
  Move a scheduled price alert from Pipedream to n8n. Map credentials, timing and saved state, then verify the handover with an existing AAPL workflow.
- WebSocket fan-out with Redis pub/sub: one market data connection for every service [Developer Tutorials] (2026-10-03)
  https://sifting.io/blog/websocket-fan-out-redis-pub-sub-one-market-data-connection
  Share one market data WebSocket across services: read max_conn and max_subs from the ack, republish ticks on Redis, snapshot for late joiners. Python code.
- Wallet portfolio API: handle spam, dust and unpriced tokens honestly [DEX & DeFi] (2026-10-02)
  https://sifting.io/blog/wallet-portfolio-api-spam-tokens-hide-dust-unpriced-balance
  Build a clearer wallet balance screen with separate buckets for priced, dust, unpriced and unrecognised tokens, using a tested decimal-based classifier.
- API key rotation: a safe cutover and what to do after a leak [Developer Tutorials] (2026-10-02)
  https://sifting.io/blog/rotate-api-key-without-downtime-two-keys-parallel-revoke-leaked
  A practical API key rotation checklist for REST jobs and WebSocket workers. Separate planned overlap from urgent revocation after a leak.
- Market data API request calculator: estimate calls before choosing a plan [Developer Tutorials] (2026-10-01)
  https://sifting.io/blog/market-data-api-request-volume-estimate-symbols-users-refresh
  Estimate market data API calls from symbols, polling intervals, shared workers and backfill. Includes worked examples and a tested Python calculator.
- Parse JSON numbers as decimals in Python, Go and JavaScript [Developer Tutorials] (2026-09-30)
  https://sifting.io/blog/parse-json-number-as-decimal-market-data-python-go-javascript
  Preserve market data decimals from JSON to your database with tested Python, Go and JavaScript examples, plus a precision regression test.
- DEX TVL alerts: detect a sustained drop without claiming a rug pull [DEX & DeFi] (2026-09-29)
  https://sifting.io/blog/pool-tvl-api-alert-dex-pair-loses-liquidity-ethereum-base-arbitrum
  Build a tested DEX pair TVL watcher with a fixed baseline, three-minute confirmation, timestamp checks and explicit handling of missing data.
- R market data API: OHLCV to xts with httr2 [Quant Research & Backtesting] (2026-09-28)
  https://sifting.io/blog/r-market-data-api-replace-getsymbols-httr2-xts
  Replace the quantmod getSymbols loading step with an R market data API: fetch US stock OHLCV bars with httr2, paginate by cursor and build an xts object.
- Excel Power Query: forex, gold and stock prices from an API [Developer Tutorials] (2026-09-28)
  https://sifting.io/blog/excel-power-query-market-data-api-live-fx-gold-stock-quotes-no-vba
  Pull forex, gold and stock quotes into Excel with Power Query. Keep timestamps and fallback prices visible, protect your API key, and budget refresh calls.
- Market data API rate limits: handle HTTP 429 without a retry storm [Developer Tutorials] (2026-09-27)
  https://sifting.io/blog/market-data-api-rate-limits-handle-http-429-without-retry-storm
  Tell a burst 429 (rate_limit_exceeded) from a spent monthly quota (monthly_quota_exceeded), retry safely with jitter, and cut the polling that causes it.
- Forex ATR in Python: calculate volatility from OHLCV [Forex & Crypto] (2026-09-26)
  https://sifting.io/blog/forex-atr-python-ohlcv-volatility
  Calculate forex ATR in Python from OHLCV bars. Test Wilder smoothing, convert ATR to pips and percentages, and avoid seed and unfinished-candle errors.
- Crypto tick data in Python: record a reference-price stream [Forex & Crypto] (2026-09-25)
  https://sifting.io/blog/crypto-tick-data-record-your-own-dataset-websocket-python
  Record BTCUSD and ETHUSD reference-price updates to Parquet with Python. Includes a tested WebSocket recorder, session reports and clear data limitations.
- Forex correlation matrix in Python: use returns, not prices [Forex & Crypto] (2026-09-24)
  https://sifting.io/blog/forex-correlation-matrix-python-api
  Build a forex correlation matrix in Python with API data. Compare EURUSD, GBPUSD and USDJPY returns without hiding missing days or reversing quote direction.
- Forex API in Grafana: build a EUR/USD price dashboard [Forex & Crypto] (2026-09-23)
  https://sifting.io/blog/grafana-market-data-api-chart-panel-rest-json-infinity-data-source
  Connect the SiftingIO Forex API to Grafana: chart EUR/USD OHLCV, show bid and ask, protect your API key, and choose a refresh rate that fits your plan.
- 406 Not Acceptable gzip API error: send Accept-Encoding: gzip and decode the reply [Developer Tutorials] (2026-09-22)
  https://sifting.io/blog/406-not-acceptable-gzip-api-accept-encoding-decode-reply
  Fix SiftingIO's 406 gzip_required error in curl, Python, Node and Go. Learn which clients decode automatically and when to handle gzip yourself.
- How to use a market data API in a web app without exposing your API key [Dashboards & Frontend] (2026-09-21)
  https://sifting.io/blog/market-data-api-web-app-without-exposing-api-key
  Keep your market data API key out of browser code. Build a server-side quote route with allowed symbols, access checks and safe error handling.
- Market data in Google Sheets: update forex, gold and crypto prices with Apps Script [Developer Tutorials] (2026-09-18)
  https://sifting.io/blog/market-data-in-google-sheets-forex-gold-crypto-prices-apps-script
  A copyable Apps Script recipe that pulls EURUSD, XAUUSD and BTCUSD into Google Sheets, with value time, fetch time and a per-row status. Manual run, then a trigger.
- n8n price alert workflow: message Slack only when a symbol crosses a level [Developer Tutorials] (2026-09-17)
  https://sifting.io/blog/n8n-price-alert-workflow-slack-message-on-level-cross-2
  Build an n8n price alert for Slack with market-hours checks, input validation, and state that detects level crossings between sampled prices.
- Crypto token symbols are not unique: how to match an ERC-20 token to the right price [DEX & DeFi] (2026-09-16)
  https://sifting.io/blog/crypto-token-symbols-not-unique-match-erc20-token-to-right-price
  Crypto token symbols are not unique. Key ERC-20 tokens on chain plus validated contract address and map each identity to a price explicitly, never by symbol.
- Delayed vs live stock prices: how to tell which one your app is showing [US Equities] (2026-09-15)
  https://sifting.io/blog/delayed-vs-live-stock-prices-how-to-tell-which-your-app-shows
  Delayed vs live stock prices look identical on screen. Decide per feature which one you need, measure freshness without fooling yourself, and label it honestly.
- Economic calendar API: pull scheduled CPI, jobs report, and rate decision events as structured data [Economic Calendar] (2026-09-13)
  https://sifting.io/blog/economic-calendar-api-scheduled-cpi-jobs-report-rate-decision-events
  How to use an economic calendar API to fetch scheduled US macro releases as JSON: UTC times, impact tiers, filtering by event, gating code and windowing a year of events.
- Missing candles in OHLCV data: how to detect gaps and backfill them safely [Quant Research & Backtesting] (2026-09-11)
  https://sifting.io/blog/missing-candles-ohlcv-data-detect-gaps-backfill-safely
  Detect missing candles in OHLCV bar data with a UTC grid and a trading calendar, then backfill safely without forward-filling volume or range.
- Stale market data: how to detect and handle it in real-time trading applications [Developer Tutorials] (2026-09-10)
  https://sifting.io/blog/stale-market-data-detect-handle-real-time-trading-applications
  Detect stale market data in real-time trading apps: venue vs receive timestamps, per-asset freshness thresholds, WebSocket heartbeats, reconnect, monitoring.
- Cursor pagination for market data APIs: pull years of 1-minute OHLCV bars without gaps [Developer Tutorials] (2026-09-07)
  https://sifting.io/blog/cursor-pagination-market-data-api-years-of-1-minute-ohlcv-bars
  How cursor pagination works on a market data API, with request math, a Python loop, and a gap check for multi-year pulls of 1-minute OHLCV bars.
- Economic calendar API: pull macro events as JSON and join each release to its price bar [Economic Calendar] (2026-09-07)
  https://sifting.io/blog/economic-calendar-api-macro-events-json-join-release-to-price-bar
  Pull US economic calendar events (CPI, NFP, FOMC) as JSON and join each release to the one-minute OHLCV bar that was open when it hit, using the real field names.
- How market data API pricing works: a buyer's guide for developers [Developer Tutorials] (2026-09-06)
  https://sifting.io/blog/how-market-data-api-pricing-works-buyers-guide-for-developers
  Market data API pricing explained: per-market tiers, call quotas vs rate limits, WebSocket limits, history depth, overage, free tiers, and redistribution rights.
- Plotting trades on a price chart: get the exact 1-minute bar at the fill time [Dashboards & Frontend] (2026-09-05)
  https://sifting.io/blog/plotting-trades-on-a-price-chart-exact-1-minute-bar-at-fill-time
  How to fetch the exact 1-minute OHLCV bar under each trade fill for stocks, forex, and crypto with one API, with verified page limits, intervals, and pagination.
- When does the weekly candle open and close? Weekly bar boundaries by asset class [Quant Research & Backtesting] (2026-09-04)
  https://sifting.io/blog/when-does-the-weekly-candle-open-and-close-weekly-bar-boundaries
  Weekly candle open and close times for forex, crypto, and US stocks, and how to rebuild either week convention from timestamped SiftingIO bars in pandas.
- Base currency and quote currency in FX and commodity data: inverse rates, bid and ask, symbols [Forex & Crypto] (2026-09-02)
  https://sifting.io/blog/base-currency-and-quote-currency-fx-data-inverse-rates-bid-ask
  How base currency and quote currency work in FX and commodity data: direction of a move, inverting bid and ask, EURUSD and XAUUSD symbols, cross rates.
- Building a Multi-Asset Market Data Stack: What Enterprise Teams Actually Need [Developer Tutorials] (2026-09-01)
  https://sifting.io/blog/multi-asset-market-data-stack-what-enterprise-teams-actually-need
  What enterprise teams need from a multi-asset market data API: normalized schemas, real-time plus historical, filings, on-chain data, FIX and SDK delivery.
- Cryptocurrency API: real-time and historical crypto data aggregated across venues [Forex & Crypto] (2026-08-31)
  https://sifting.io/blog/cryptocurrency-api-real-time-historical-crypto-data-aggregated
  Cryptocurrency API guide: stream real-time crypto prices over WebSocket and pull 15+ years of historical OHLCV over REST, aggregated across venues.
- How to Evaluate a Market Data API Before Going to Production [Developer Tutorials] (2026-08-30)
  https://sifting.io/blog/how-to-evaluate-a-market-data-api-before-going-to-production
  A vendor-neutral checklist for evaluating a market data API: price accuracy, latency percentiles, WebSocket stability, gaps, rate limits, and licensing.
- Supporting enterprise market data customers in China: evaluation to production [Developer Tutorials] (2026-08-30)
  https://sifting.io/blog/enterprise-market-data-customers-china-evaluation-to-production
  How an institution in China evaluates and onboards an international market data vendor: free-tier testing, FIX API, licensing in writing, USD invoicing, SLAs.
- Insider trading data API: pull Form 4 buys, sells, and grants for US stocks [SEC Filings & Fundamentals] (2026-08-29)
  https://sifting.io/blog/insider-trading-data-api-form-4-transactions-us-stocks
  Pull Form 4 insider transactions for US stocks from a REST API, decode transaction codes P, S, M, and F, and separate open-market buys from routine noise.
- How accurate is real-time market data? A live spread snapshot across crypto, FX, and gold [Quant Research & Backtesting] (2026-08-26)
  https://sifting.io/blog/how-accurate-is-real-time-market-data-live-spread-snapshot
  Consolidated bid/ask spreads measured live on BTCUSD, EURUSD, XAUUSD and more: why one price is really a band, and how to reproduce the numbers yourself.
- Multi-currency portfolio valuation: convert every position to one base currency [Forex & Crypto] (2026-08-25)
  https://sifting.io/blog/multi-currency-portfolio-valuation-convert-to-base-currency
  Convert a multi-currency portfolio to one base currency: mid vs bid, timestamp alignment, triangulated crosses, and weekend FX gaps, with worked numbers.
- How to store OHLCV data: a Postgres schema that survives gaps, corrections, and adjustments [Developer Tutorials] (2026-08-25)
  https://sifting.io/blog/how-to-store-ohlcv-data-postgres-schema-gaps-corrections
  A Postgres schema for OHLCV market data: primary key design, numeric types, honest gap handling, UPSERT for corrections, and an idempotent backfill pattern.
- Forex session overlaps: what happens to spreads, volume, and volatility [Forex & Crypto] (2026-08-23)
  https://sifting.io/blog/forex-session-overlaps-spreads-volume-volatility
  The four forex sessions in UTC, their overlaps, why London to New York carries the most volume, and how spreads behave at rollover and in the quiet hours.
- Financial ratios API: get margins, ROE, and debt to equity without parsing XBRL [SEC Filings & Fundamentals] (2026-08-22)
  https://sifting.io/blog/financial-ratios-api-margins-roe-debt-to-equity-without-xbrl
  Pull profit margins, ROE, and debt to equity from SEC filings with one API call, and drop down to raw XBRL concepts when a precomputed ratio isn't enough.
- How to get historical stock price data from a REST API (daily and intraday OHLCV) [US Equities] (2026-08-21)
  https://sifting.io/blog/how-to-get-historical-stock-price-data-rest-api
  How to pull daily and intraday OHLCV stock bars from a REST API: request format, response fields, intervals, cursor pagination, history depth, and pitfalls.
- Intraday stock data for student research: how Quantitative Trading at Brown uses the SiftingIO API [Quant Research & Backtesting] (2026-08-20)
  https://sifting.io/blog/intraday-stock-data-student-research-quantitative-trading-at-brown
  How Quantitative Trading at Brown uses intraday US equities OHLCV bars from the SiftingIO API for student research on returns, volatility, and trading volume.
- How to calculate a currency cross rate: EURGBP from EURUSD and GBPUSD [Forex & Crypto] (2026-08-18)
  https://sifting.io/blog/how-to-calculate-a-currency-cross-rate-eurgbp-triangulation
  The cross rate formula with worked math: derive EURGBP from EURUSD and GBPUSD, carry bid and ask through the triangulation, and compute it in Python.
- TA-Lib installation error? Get RSI and MACD readings from an API instead [Developer Tutorials] (2026-08-17)
  https://sifting.io/blog/ta-lib-installation-error-rsi-macd-readings-api
  TA-Lib won't install? Skip the C compile and numpy version matrix: pull RSI, MACD, and moving average readings from two REST endpoints with plain requests.
- Commodities data API: coverage, fields, and symbol names for gold, silver, oil, and metals [Commodities] (2026-08-14)
  https://sifting.io/blog/commodities-data-api-coverage-symbols-gold-silver-oil
  SiftingIO's commodities data API: real symbols (XAUUSD, XAGUSD, WTIUSD, XCUUSD, NATGAS), OHLCV history, live REST quotes, and WebSocket streaming.
- Technical signals API: pull RSI, MACD, and moving average summaries in one REST call [Developer Tutorials] (2026-08-13)
  https://sifting.io/blog/technical-signals-api-rsi-macd-moving-average-summaries
  SiftingIO's technical signals API: live RSI, MACD, and moving average summaries plus per-bar signal history with cross events, over two REST endpoints.
- CPI consensus vs actual: why an in-line inflation print barely moves markets [Economic Calendar] (2026-08-13)
  https://sifting.io/blog/cpi-consensus-vs-actual-why-an-in-line-print-barely-moves-markets
  July 2026 CPI matched consensus exactly and markets barely moved. Why consensus, actual, revisions, and exact timestamps are the calendar fields that matter.
- 1-minute OHLCV data: how to resample minute bars into 5m, 15m, and 1h [Quant Research & Backtesting] (2026-08-12)
  https://sifting.io/blog/1-minute-ohlcv-data-resample-minute-bars-into-5m-15m-1h
  1-minute OHLCV bars are the base timeframe. The exact rules for resampling minute bars into 5m, 15m, and 1h bars, with the alignment and gap traps to avoid.
- SEC filings API: get a company's filings by CIK with accession numbers and dates [SEC Filings & Fundamentals] (2026-08-10)
  https://sifting.io/blog/sec-filings-api-get-company-filings-by-cik
  Get a company's SEC filings by CIK via API: 10-K, 10-Q, and 8-K lists with accession numbers, filing dates, and form types. No EDGAR scraper to maintain.
- Token vesting fair market value: what is a token worth the minute it vests? [Forex & Crypto] (2026-08-05)
  https://sifting.io/blog/token-vesting-fair-market-value-at-the-minute-it-vests
  Under IRC section 83, vested tokens are income at fair market value. Why one venue's last trade is weak, and how a cross-venue weighted median holds up.
- What is a pip in forex? Pip size, fractional pips, and pip value math for EURUSD and USDTRY [Forex & Crypto] (2026-08-02)
  https://sifting.io/blog/what-is-a-pip-in-forex-pip-size-value-eurusd-usdtry
  What a pip is in forex: pip size for four-decimal and JPY pairs, fractional pips, and step-by-step pip value math for EURUSD and USDTRY from live quotes.
- Base currency vs quote currency: how to read a forex pair like EURUSD [Forex & Crypto] (2026-08-02)
  https://sifting.io/blog/base-currency-vs-quote-currency-how-to-read-a-forex-pair
  What base and quote currency mean, why it's EURUSD and never USDEUR, direct vs indirect quoting, and how to read any FX price. With live API examples.
- What timezone is market data in? UTC timestamps, DST pitfalls, and session times explained [Developer Tutorials] (2026-08-02)
  https://sifting.io/blog/what-timezone-is-market-data-in-utc-timestamps-dst-sessions
  Market data timestamps are UTC: epoch ms ticks, RFC 3339 bars, date-only filings. How to handle DST, weekend boundaries, and convert correctly in Python and JS.
- Daily candle open and close times: forex, crypto and US stocks [Quant Research & Backtesting] (2026-08-02)
  https://sifting.io/blog/when-does-a-daily-candle-open-and-close-daily-bar-boundaries
  Compare daily candle open and close times for forex, crypto and US stocks, including UTC vs New York cutoffs, daylight saving and regular trading sessions.
- Natural gas price API: what NATGAS measures, MMBtu explained, and a unit conversion table (2026-08-02)
  https://sifting.io/blog/natural-gas-price-api-natgasusd-mmbtu-unit-conversion
  What the NATGAS natural gas price measures, what an MMBtu is, how to convert to therms, kWh, MWh and GJ, and how to fetch bars and live prices.
- What is a lot in forex? Standard, mini, and micro lot sizes with position value math [Forex & Crypto] (2026-08-02)
  https://sifting.io/blog/what-is-a-lot-in-forex-standard-mini-micro-lot-size-position-value
  What a lot means in forex: standard, mini, micro, and nano lot sizes in one table, plus position value and pip value math computed from a live EURUSD quote.
- Pre-market, regular hours, and after-hours: what US stock data exists in each session [US Equities] (2026-08-02)
  https://sifting.io/blog/premarket-regular-after-hours-us-stock-data-sessions
  Pre-market, regular hours, and after-hours US stock sessions with times in ET and UTC, what data exists in each, and how an app should handle the boundaries.
- XAUUSD explained: gold price per gram, karat, and local currency math with a market data API [Commodities] (2026-08-01)
  https://sifting.io/blog/xauusd-gold-price-per-gram-karat-local-currency-math
  What XAUUSD means, the exact troy ounce to gram conversion, 24k/22k/18k karat math, and lira pricing via USDTRY, computed from real SiftingIO market data.
- Crude oil prices API: Brent (UKOUSD) and WTI (WTIUSD), historical bars and live quotes [Commodities] (2026-08-01)
  https://sifting.io/blog/crude-oil-prices-api-brent-ukousd-wti-wtiusd-historical-live
  Pull Brent (UKOUSD) and WTI (WTIUSD) crude oil prices from one API: historical OHLCV bars with cursor pagination, REST snapshots, and WebSocket streaming.
- Stock portfolio tracker API: compute unrealized P&L from bars and live quotes [US Equities] (2026-07-30)
  https://sifting.io/blog/stock-portfolio-tracker-api-compute-pnl
  Build a stock portfolio tracker with a US stocks API: positions as data, daily-bar backfill, unrealized P&L from live quotes, and a WebSocket upgrade path.
- Streaming gold, silver, and oil prices: a WebSocket commodities price API [Commodities] (2026-07-30)
  https://sifting.io/blog/streaming-gold-silver-oil-prices-websocket-commodities-api
  Stream live gold, silver, and oil prices over one WebSocket connection instead of polling REST. Subscribe protocol, tick format, and a reconnect-safe client.
- Commodities market data API: backfill historical bars, then stream live prices [Commodities] (2026-07-29)
  https://sifting.io/blog/commodities-market-data-api-backfill-history-stream-live-prices
  Pull historical OHLCV bars for gold, crude, and other commodities over REST, stream live prices over WebSocket, and handle the seam where they meet.
- CIK, CUSIP, ISIN, SEDOL, and FIGI: mapping a US stock ticker to its identifiers [SEC Filings & Fundamentals] (2026-07-28)
  https://sifting.io/blog/map-a-stock-ticker-to-cik-cusip-isin-sedol-figi
  Map a stock ticker to its CIK, CUSIP, ISIN, SEDOL, and FIGI: who issues each, which are free, and how to resolve AAPL to its CIK with one API call.
- Market data MCP server: one connector for stocks, forex, crypto, DEX, commodities, and fundamentals [Developer Tutorials] (2026-07-27)
  https://sifting.io/blog/market-data-mcp-server-one-connector-six-asset-classes
  SiftingIO's MCP server gives an AI assistant forex, stock, crypto, commodity, DEX, and fundamentals data through one connector and a single API key.
- Share one API subscription with your whole team: SiftingIO's new Team feature [Developer Tutorials] (2026-07-26)
  https://sifting.io/blog/share-one-api-subscription-with-your-team
  SiftingIO's Team feature lets one subscription cover a whole team: seats by tier (3 Builder, 10 Pro, 25 Ultra), email invites, and one usage view.
- Market hours widget: embed a live open or closed badge with one iframe tag [Dashboards & Frontend] (2026-07-25)
  https://sifting.io/blog/market-hours-widget-embed-live-open-closed-badge-iframe
  Embed a free market hours widget with one iframe tag: live open and closed status for 23 equity markets plus forex and crypto sessions. No account needed.
- Market hours API: check whether a market is open before your code polls it [Economic Calendar] (2026-07-25)
  https://sifting.io/blog/market-hours-api-check-market-open-holidays-half-days
  Use a market hours API to check open status, holiday calendars, and half days across 23 markets so your polling code stops serving stale prices.
- Golang WebSocket market data: stream real-time forex and crypto prices [Developer Tutorials] (2026-07-24)
  https://sifting.io/blog/golang-websocket-market-data-stream-forex-crypto-prices
  Stream real-time forex and crypto prices in Go over WebSocket: authenticate, subscribe, parse tick frames, and handle pings and reconnects with working code.
- How to give an AI agent market data: MCP server, REST tools, and WebSocket streaming [Developer Tutorials] (2026-07-23)
  https://sifting.io/blog/how-to-give-an-ai-agent-market-data-mcp-rest-websocket
  How to give an AI agent live and historical market data: an MCP server for Claude and Cursor, REST tool definitions, and WebSocket streaming for monitors.
- Why two backtests of the same strategy disagree: the historical price data faults to check first [Quant Research & Backtesting] (2026-07-21)
  https://sifting.io/blog/why-two-backtests-disagree-historical-price-data-faults
  Five ways historical price data silently corrupts a backtest: adjustment method, venue-dependent highs and lows, gap policy, session cuts, and restated bars.
- What price should a trading game settle on? Settlement rules for simulators and contests [Developer Tutorials] (2026-07-19)
  https://sifting.io/blog/what-price-should-a-trading-game-settle-on
  How trading simulators and paper-trading apps should pick a settlement price: venue disagreement, stale quotes, adjusted history, and a rule players can read.
- The market data behind an investor relations page [Dashboards & Frontend] (2026-07-18)
  https://sifting.io/blog/market-data-behind-an-investor-relations-page
  What an investor relations page needs from a market data API: delayed quotes, five-year OHLCV charts, self-updating XBRL financials, and a build checklist.
- DEX swap cost basis: why the daily index price is not the price you got [DEX & DeFi] (2026-07-16)
  https://sifting.io/blog/dex-swap-cost-basis-daily-index-price-vs-actual-fill
  Crypto tax software prices DEX swaps with daily index averages that exclude DEX data. What Form 1099-DA changes for 2026, and why the swap record is stronger.
- Forex OHLCV data: how FX candles are built [Forex & Crypto] (2026-07-16)
  https://sifting.io/blog/forex-ohlcv-data-how-fx-candles-are-built
  Forex OHLCV data explained: how FX candles are built from aggregated quotes, why daily bars depend on the day boundary you pick, and how to pull them by API.
- Live bid and ask price API: build a correct EURUSD and BTCUSD quote client [Forex & Crypto] (2026-07-15)
  https://sifting.io/blog/live-bid-ask-price-api-correct-eurusd-btcusd-quote-client
  How to read live bid and ask prices for EURUSD and BTCUSD over REST and WebSocket: string-typed fields, epoch milliseconds, ping keepalive, and rate limits.
- Adjusted vs unadjusted stock prices: total return vs price return in a backtest [Quant Research & Backtesting] (2026-07-11)
  https://sifting.io/blog/adjusted-vs-unadjusted-stock-prices-backtest-returns
  Build a total-return series from as-traded prices, split factors, and cash dividends with reinvestment math, and see where the adjusted-close shortcut misleads a backtest.
- Insider trading data API: pull SEC Form 4 transactions for a stock watchlist [SEC Filings & Fundamentals] (2026-07-11)
  https://sifting.io/blog/insider-trading-data-api-sec-form-4-watchlist
  Pull SEC Form 4 insider transactions for a stock watchlist with Python: decode transaction codes, paginate the API, and build a screening table with pandas.
- Aligning economic calendar events with your backtest's bar timeline [Economic Calendar] (2026-07-11)
  https://sifting.io/blog/aligning-economic-calendar-events-with-backtest-bars-2
  How to pull scheduled US economic events from one endpoint and align them with OHLCV bars so a backtest knows when CPI, NFP, and FOMC actually hit.
- US stock market data API: build a live price chart with historical bars and real-time ticks [Dashboards & Frontend] (2026-07-07)
  https://sifting.io/blog/building-a-symbol-price-page-live-quotes-and-charts
  Build a live US stock price and chart page: backfill adjusted OHLCV history over REST, stream real-time ticks over WebSocket, and handle market sessions.
- yfinance rate limited: migrating Python price pulls to a keyed market data API [Developer Tutorials] (2026-07-07)
  https://sifting.io/blog/yfinance-rate-limited-migrating-python-price-pulls-keyed-api
  yfinance rate limited or throwing Too Many Requests? A call-for-call map to move daily history, intraday bars, and latest-price pulls to a keyed API in Python.
- MiCA crypto trade reporting: one trade format for 27 countries is a normalization problem [Forex & Crypto] (2026-07-07)
  https://sifting.io/blog/mica-crypto-trade-reporting-normalization-problem
  MiCA crypto trade reporting mandates one uniform trade format across the EU's 27 countries. Why it's a cross-venue data-normalization problem, not paperwork.
- Economic calendar webhook alerts: get CPI and jobs report releases pushed to your app [Economic Calendar] (2026-07-06)
  https://sifting.io/blog/economic-calendar-webhook-alerts-cpi-jobs-report
  Get CPI and jobs report releases pushed to any webhook: a Node.js poller on SiftingIO's economic calendar API, with plan limit math and common pitfalls.
- Weighted median price aggregation: why averaging venue prices fails [Quant Research & Backtesting] (2026-07-06)
  https://sifting.io/blog/weighted-median-price-aggregation-why-averaging-fails
  Why the same asset shows different prices on different venues, how naive averages get poisoned by bad feeds, and how a weighted median keeps the price honest.
- Economic calendar API: handling revisions, timezones, and the bar each release moved [Economic Calendar] (2026-07-06)
  https://sifting.io/blog/economic-calendar-api-revisions-timezones-and-the-bar-it-moved
  Ingest CPI and NFP releases from an economic calendar API: handle first prints vs revisions, normalize times to UTC, and tag the bar each event moved.
- Bid ask spread: how to read a quote and flag a wide or stale one in code [Developer Tutorials] (2026-07-06)
  https://sifting.io/blog/bid-ask-spread-read-a-quote-flag-wide-or-stale
  How to read bid and ask from a quote snapshot, compute the bid ask spread in basis points, and flag wide or stale quotes in Python before users see them.
- Do Bitcoin Spreads Widen on Holidays? A July 4 Liquidity Study [Forex & Crypto] (2026-07-05)
  https://sifting.io/blog/do-bitcoin-spreads-widen-on-holidays-july-4-liquidity-study
  How to measure bitcoin bid-ask spreads and cross-venue liquidity over the July 4 holiday weekend, and what spread widening means for execution and alerts.
- Why financial platforms need a clear market data methodology [Developer Tutorials] (2026-07-04)
  https://sifting.io/blog/why-financial-platforms-need-a-clear-market-data-methodology
  A market data methodology explains where a price comes from and how it is validated. Here is why that documentation matters and how to read it in an API.
- Forex Market Data API: A Practical Guide for Developers [Forex & Crypto] (2026-07-01)
  https://sifting.io/blog/forex-market-data-api-a-practical-guide-for-developers
  Pull live FX quotes and historical OHLCV bars over REST, and stream live forex ticks over WebSocket. A practical Forex API guide for developers.
- Currency Conversion API vs Forex Market Data API: Which One You Need [Forex & Crypto] (2026-06-26)
  https://sifting.io/blog/currency-conversion-api-vs-forex-market-data-api
  A currency conversion API returns one mid rate per minute. A forex market data API gives tick bid/ask and OHLCV bars. Here is how to pick before you build.
- Market data over FIX 4.4 for high-performance trading systems [Forex & Crypto] (2026-06-25)
  https://sifting.io/blog/market-data-over-fix-4-4-for-high-performance-trading-systems
  Pull cross-venue reference prices over a FIX 4.4 market-data session for crypto, forex, and metals, then feed them into MetaTrader. Market data only.
- Building Financial AI Agents With Real-Time and Historical Market Data [Developer Tutorials] (2026-06-23)
  https://sifting.io/blog/building-financial-ai-agents-with-real-time-historical-market-data
  Why financial AI agents need structured market data APIs instead of web search, and how to wire prices, OHLCV, and fundamentals in as agent tools.
- What is OHLCV data? Bars, candles, and how providers build them [Developer Tutorials] (2026-06-20)
  https://sifting.io/blog/what-is-ohlcv-data-and-why-it-matters-in-market-data-apis
  What OHLCV data is, how bars are built from ticks, why two providers disagree on the same candle, adjusted vs unadjusted bars, and how to pull bars via API.
- Read any wallet's token portfolio across chains without RPC calls [DEX & DeFi] (2026-06-15)
  https://sifting.io/blog/read-any-wallets-token-portfolio-across-chains-no-rpc-calls
  Use one SiftingIO endpoint to pull native and ERC-20 balances on Ethereum, Base, and Arbitrum, then value them with consensus snapshots instead of manual on-chain math.
- Detecting a stale or manipulated quote with a cross-venue consensus price [Forex & Crypto] (2026-06-13)
  https://sifting.io/blog/detecting-a-stale-or-manipulated-quote-with-consensus-price
  How to use a robust cross-venue fair price as a validation layer to flag when one venue is printing a stale, thin, or manipulated quote.
- How Much Do Bitcoin Prices Differ Across Exchanges? A 90-Day, 5-Venue Study [Research] (2026-06-11)
  https://sifting.io/blog/bitcoin-price-dispersion-across-exchanges
  We measured Bitcoin price dispersion across Binance, Coinbase, Kraken, OKX, and Bybit over 90 days. After adjusting for USDT, the five venues agree to about 2.4 bps, and roughly 63% of the apparent cross-exchange spread is the stablecoin, not the exchange.
- Streaming on-chain DEX swaps over WebSocket without reconnect bugs [DEX & DeFi] (2026-06-10)
  https://sifting.io/blog/streaming-on-chain-dex-swaps-over-websocket-without-reconnect-bugs
  Build a resilient WebSocket subscriber for the SiftingIO dex product: chain:PAIR symbols, the auth handshake, last-cached-then-live emits, and a 60s ping.
- How to Get Real-Time Forex Prices in Python [Developer Tutorials] (2026-06-07)
  https://sifting.io/blog/real-time-forex-prices-in-python
  Get real-time forex prices in Python: a requests call for a snapshot, a websockets feed for live updates, the official siftingio SDK with sync and async clients, and buffering ticks into a pandas DataFrame.
- How to Get Gold and Oil Spot Prices via API [Commodities] (2026-06-06)
  https://sifting.io/blog/get-gold-and-oil-spot-prices-via-api
  Gold and oil spot prices from one API call. How SiftingIO symbols commodities (XAUUSD, WTIUSD), how to read a last trade or quote, and how the published price is aggregated across venues.
- How to Get the Latest Price and Volume From a DEX Without the On-Chain Math [DEX & DeFi] (2026-06-05)
  https://sifting.io/blog/get-latest-price-and-volume-from-a-dex
  A DEX has no last-price field. Getting a usable price and volume means pool math, decimals, token ordering, and swap-log parsing behind an RPC node. Here is what that takes and the one-call alternative.
- REST vs WebSocket for Real-Time Market Data: When to Use Each [Developer Tutorials] (2026-06-01)
  https://sifting.io/blog/rest-vs-websocket-for-real-time-market-data
  REST or WebSocket for live market data? A plain guide to how each works, when to pull on demand versus stream, why fast polling scales badly, and how to use both together.
- Tracking institutional position changes across 13F filings [SEC Filings & Fundamentals] (2026-06-01)
  https://sifting.io/blog/tracking-institutional-position-changes-across-13f-filings
  How to diff 13F-HR holdings between reporting cycles using SiftingIO, find new positions, exits, and meaningful size changes.
- What are the 7 C commodities? [Commodities] (2026-05-27)
  https://sifting.io/blog/what-are-the-7-c-commodities
  The 7 C commodities are coffee, corn, cotton, copper, crude oil, cocoa, and cattle. A plain guide to what the seven mean, why they are grouped, and how to track their prices.
- How to get real-time forex prices from SiftingIO [Forex & Crypto] (2026-05-26)
  https://sifting.io/blog/how-to-get-real-time-forex-prices-from-siftingio
  A plain guide to getting live forex prices from SiftingIO: a single request for the current rate, or a live feed that updates on its own as the market moves.
- Aligning economic calendar events with your backtest's bar timeline [Economic Calendar] (2026-05-24)
  https://sifting.io/blog/aligning-economic-calendar-events-with-backtest-bars
  How to pull scheduled US economic events from one endpoint and align them with OHLCV bars so a backtest knows when CPI, NFP, and FOMC actually hit.


## 2. pricing.md (200, 3961 chars)
# Pricing — SiftingIO

SiftingIO sells multi-asset market data (crypto, DEX, forex, commodities, US stocks) over REST and WebSocket. Pricing is per market: pick the markets you use, on the same tier ladder across every market. All prices are USD. Annual billing is ~20% cheaper than monthly.

Source of truth: https://sifting.io/pricing

## Per-market prices

Each market is priced independently on the Builder / Pro / Ultra tiers. Every market also has a free tier (see the tier ladder below) and an Enterprise option.

| Market | Builder | Pro | Ultra |
| --- | --- | --- | --- |
| Crypto | $99/mo · $79/mo annual | $299/mo · $239/mo annual | $799/mo · $639/mo annual |
| DEX & DeFi | $129/mo · $103/mo annual | $399/mo · $320/mo annual | $1099/mo · $879/mo annual |
| Forex | $99/mo · $79/mo annual | $299/mo · $239/mo annual | $799/mo · $639/mo annual |
| Commodities | $99/mo · $79/mo annual | $299/mo · $239/mo annual | $799/mo · $639/mo annual |
| US Stocks | $129/mo · $103/mo annual | $399/mo · $320/mo annual | $1099/mo · $879/mo annual |

Prices differ by market because the cost to source and license the underlying data varies. The canonical schema, SLA, and tier limits are identical across every market.

## Bundle discounts

Subscribe to two or more markets at the same bundle-eligible tier (Pro or Ultra) on one account and a discount applies automatically:

- 2 markets (Duo): 10% off, applied automatically at invoice
- 3 markets (Trio): 15% off, applied automatically at invoice
- 4 markets (Quartet): 20% off, applied automatically at invoice
- 5 markets (Platform): 25% off, applied automatically at invoice

You can mix tiers across markets freely; the bundle discount keys off markets at the same eligible tier.

## Tier ladder

The same five tiers apply to every market. Limits below are per market unless noted.

### Free
- REST quota: 10,000 calls/mo
- Rate limit: 60 req/min
- WebSocket connections: 1
- Symbol subscriptions: 5
- Historical depth: Up to 25+ years
- Uptime SLA: Best-effort
- Notes: Per market. Community support. No card required.

### Builder
- REST quota: 250,000 calls/mo
- Rate limit: 100 req/sec
- WebSocket connections: 3
- Symbol subscriptions: 100
- Historical depth: Up to 25+ years
- Uptime SLA: 99.5%
- Notes: Per market. Email support. Scoped API keys.

### Pro
- REST quota: 5,000,000 calls/mo
- Rate limit: 150 req/sec
- WebSocket connections: 10
- Symbol subscriptions: 1,000
- Historical depth: Up to 25+ years
- Uptime SLA: 99.9% + service credits
- Notes: Per market. Bundle-eligible. Audit logs. Priority email.

### Ultra
- REST quota: Unlimited calls
- Rate limit: 250 req/sec (upgradeable)
- WebSocket connections: 50
- Symbol subscriptions: Unlimited
- Historical depth: Up to 25+ years
- Uptime SLA: 99.9% + auto credits
- Notes: Per market. Bundle-eligible. IP allowlist. Slack support.

### Enterprise
- REST quota: Custom
- Rate limit: Custom
- WebSocket connections: Custom
- Symbol subscriptions: Custom
- Historical depth: Up to 25+ years + custom backfill
- Uptime SLA: 99.95% + credits
- Notes: Cross-market. SSO/SAML, SCIM, dedicated AM, Net-30/60 invoicing, DPA/MSA.

## Add-ons

- Extra REST calls: from $79/mo per 1M calls — Top up Builder or Pro with additional monthly call quota in 1M packs.
- REST rate-limit upgrade: from $199/mo per +200 req/sec — Raise your sustained request rate on Pro or Ultra in 200 req/sec increments.
- Extra WebSocket capacity: from $59/mo per +5 conns / +500 subs — Add connections and subscriptions beyond your tier without upgrading.
- Historical backfill bundles: from $499/bundle — Backfills for custom datasets or coverage, billed per bundle. Full available history is already included on every plan.
- Dedicated single-tenant cluster: Quote on request — Isolated infrastructure with custom rate, region, and routing.

## Contact

- Plans and self-serve checkout: https://sifting.io/pricing
- Enterprise and custom contracts: hello@sifting.io


## 3. openapi.yaml (200, 91968 chars)
openapi: 3.1.0
info:
  title: SiftingIO Market Data API
  version: "1.0.0"
  description: |
    REST data-plane for the SiftingIO market data API: live prices,
    SEC fundamentals (filings, XBRL financials, ratios, insiders, ownership),
    historical OHLCV bars, market calendars, DEX wallet portfolios, and the
    US economic calendar.

    Authentication is by API key, sent as the `X-API-Key` header (preferred)
    or the `api_key` query parameter.

    Several endpoints return large payloads and **require** `Accept-Encoding: gzip`
    (they respond `406` otherwise): the screener, both financials endpoints,
    all historical-bars endpoints, and the market snapshot.

    This document describes the data plane only. Account, billing, and auth
    (the `/ops/v1` control plane) are out of scope.
  license:
    name: MIT
    identifier: MIT
  contact:
    name: SiftingIO
    url: https://sifting.io/docs

servers:
  - url: https://api.sifting.io
    description: Production

security:
  - ApiKeyHeader: []
  - ApiKeyQuery: []

tags:
  - name: Symbols
    description: Instrument catalog, search and reference metadata across asset classes.
  - name: Live
    description: Live market data read from the engine's latest snapshot.
  - name: Stocks
    description: US-equity fundamentals (SEC) and historical bars.
  - name: Filers
    description: 13F institutional holdings.
  - name: Markets
    description: Market catalog, status, hours, and holiday calendars.
  - name: Forex
    description: Historical OHLC bars for FX pairs.
  - name: Commodities
    description: Historical OHLC bars for commodities (metals, energy, industrials).
  - name: Crypto
    description: Historical OHLCV bars for crypto symbols.
  - name: DEX
    description: On-chain wallet portfolios and historical DEX bars.
  - name: EconomicCalendar
    description: US macro economic events.
  - name: Convert
    description: Live currency and token conversion across forex and crypto.
  - name: Signals
    description: Technical-analysis signals (live and historical) across stocks, forex, crypto, and commodities.

paths:
  /v1/symbols:
    get:
      tags: [Symbols]
      operationId: listSymbols
      summary: List active instruments
      description: |
        Search the instrument catalog across stocks, crypto, DEX, forex and
        commodities. Only active entries are listed. Results are ordered by
        asset class, symbol and a stable internal tie-breaker.
        Catalog access has no market entitlement gate, but normal API-key
        authentication, usage accounting and rate limits still apply.
        A catalog entry does not guarantee live or historical data availability.
        Gzip is supported but not required.
      parameters:
        - $ref: "#/components/parameters/CatalogAssetClassFilter"
        - $ref: "#/components/parameters/CatalogVenue"
        - $ref: "#/components/parameters/CatalogQuery"
        - $ref: "#/components/parameters/Cursor"
        - $ref: "#/components/parameters/CatalogLimit"
      responses:
        "200": { $ref: "#/components/responses/SymbolCatalogList" }
        "400": { $ref: "#/components/responses/BadRequest" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "429": { $ref: "#/components/responses/TooManyRequests" }
        "500": { $ref: "#/components/responses/CatalogInternalError" }
        "503": { $ref: "#/components/responses/ServiceUnavailable" }

  /v1/symbols/{asset_class}/{symbol}:
    get:
      tags: [Symbols]
      operationId: getSymbol
      summary: Get instrument details
      description: |
        Look up a stock, crypto, forex or commodity catalog entry using its
        asset class and symbol, for example `/v1/symbols/stocks/AAPL`.
        The asset class is normalized to lowercase and the symbol to uppercase.
        Inactive and delisted entries can also be returned.
        For DEX instruments, use the detail path with a separate chain segment.
        Normal API-key authentication and usage limits apply; no market
        entitlement is required. Gzip is supported but not required.
      parameters:
        - $ref: "#/components/parameters/CatalogAssetClass"
        - $ref: "#/components/parameters/CatalogSymbol"
      responses:
        "200": { $ref: "#/components/responses/SymbolCatalogDetail" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }
        "500": { $ref: "#/components/responses/CatalogInternalError" }
        "503": { $ref: "#/components/responses/ServiceUnavailable" }

  /v1/symbols/dex/{chain}/{symbol}:
    get:
      tags: [Symbols]
      operationId: getDexSymbol
      summary: Get DEX instrument details
      description: |
        Look up a DEX catalog entry by chain and symbol, for example
        `/v1/symbols/dex/eth/ETHUSD`. The chain is normalized to lowercase
        and the symbol to uppercase. Preserve the slash separators from the
        catalog slug; encode each path segment separately.
        Inactive and delisted entries can also be returned.
        Normal API-key authentication and usage limits apply; no market
        entitlement is required. Gzip is supported but not required.
      parameters:
        - $ref: "#/components/parameters/CatalogChain"
        - $ref: "#/components/parameters/CatalogSymbol"
      responses:
        "200": { $ref: "#/components/responses/SymbolCatalogDetail" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }
        "500": { $ref: "#/components/responses/CatalogInternalError" }
        "503": { $ref: "#/components/responses/ServiceUnavailable" }

  /v1/fnd/symbols:
    get:
      tags: [Symbols]
      operationId: listFundamentalSymbols
      summary: List active instruments (fundamentals alias)
      description: |
        Alias of `/v1/symbols`; uses the same filters and response.
        Search the instrument catalog across stocks, crypto, DEX, forex and
        commodities. Only active entries are listed. Results are ordered by
        asset class, symbol and a stable internal tie-breaker.
        Catalog access has no market entitlement gate, but normal API-key
        authentication, usage accounting and rate limits still apply.
        A catalog entry does not guarantee live or historical data availability.
        Gzip is supported but not required.
      parameters:
        - $ref: "#/components/parameters/CatalogAssetClassFilter"
        - $ref: "#/components/parameters/CatalogVenue"
        - $ref: "#/components/parameters/CatalogQuery"
        - $ref: "#/components/parameters/Cursor"
        - $ref: "#/components/parameters/CatalogLimit"
      responses:
        "200": { $ref: "#/components/responses/SymbolCatalogList" }
        "400": { $ref: "#/components/responses/BadRequest" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "429": { $ref: "#/components/responses/TooManyRequests" }
        "500": { $ref: "#/components/responses/CatalogInternalError" }
        "503": { $ref: "#/components/responses/ServiceUnavailable" }

  /v1/fnd/symbols/{asset_class}/{symbol}:
    get:
      tags: [Symbols]
      operationId: getFundamentalSymbol
      summary: Get instrument details (fundamentals alias)
      description: |
        Look up a stock, crypto, forex or commodity catalog entry using its
        asset class and symbol, for example `/v1/fnd/symbols/stocks/AAPL`.
        The asset class is normalized to lowercase and the symbol to uppercase.
        Inactive and delisted entries can also be returned.
        For DEX instruments, use the detail path with a separate chain segment.
        Normal API-key authentication and usage limits apply; no market
        entitlement is required. Gzip is supported but not required.
      parameters:
        - $ref: "#/components/parameters/CatalogAssetClass"
        - $ref: "#/components/parameters/CatalogSymbol"
      responses:
        "200": { $ref: "#/components/responses/SymbolCatalogDetail" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }
        "500": { $ref: "#/components/responses/CatalogInternalError" }
        "503": { $ref: "#/components/responses/ServiceUnavailable" }

  /v1/fnd/symbols/dex/{chain}/{symbol}:
    get:
      tags: [Symbols]
      operationId: getFundamentalDexSymbol
      summary: Get DEX instrument details (fundamentals alias)
      description: |
        Look up a DEX catalog entry by chain and symbol, for example
        `/v1/fnd/symbols/dex/eth/ETHUSD`. The chain is normalized to lowercase
        and the symbol to uppercase. Preserve the slash separators from the
        catalog slug; encode each path segment separately.
        Inactive and delisted entries can also be returned.
        Normal API-key authentication and usage limits apply; no market
        entitlement is required. Gzip is supported but not required.
      parameters:
        - $ref: "#/components/parameters/CatalogChain"
        - $ref: "#/components/parameters/CatalogSymbol"
      responses:
        "200": { $ref: "#/components/responses/SymbolCatalogDetail" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }
        "500": { $ref: "#/components/responses/CatalogInternalError" }
        "503": { $ref: "#/components/responses/ServiceUnavailable" }

  /v1/last/trade/{venue}/{symbol}:
    get:
      tags: [Live]
      operationId: getLastTrade
      summary: Latest trade
      parameters:
        - $ref: "#/components/parameters/Venue"
        - $ref: "#/components/parameters/Symbol"
      responses:
        "200":
          description: Latest trade snapshot.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/LastTrade" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }
        "503": { $ref: "#/components/responses/ServiceUnavailable" }

  /v1/last/quote/{venue}/{symbol}:
    get:
      tags: [Live]
      operationId: getLastQuote
      summary: Top-of-book quote
      parameters:
        - $ref: "#/components/parameters/Venue"
        - $ref: "#/components/parameters/Symbol"
      responses:
        "200":
          description: Top-of-book quote.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/LastQuote" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }
        "503": { $ref: "#/components/responses/ServiceUnavailable" }

  /v1/last/close/{venue}/{symbol}:
    get:
      tags: [Live]
      operationId: getLastClose
      summary: Previous close
      description: >
        The most recent previous close for one instrument. For crypto/forex/
        commodities this is the engine's fair price frozen at the last 00:00
        UTC boundary; for stocks it is the official exchange session close.
        The response is immutable until the next close, so it is edge-cacheable
        (Cache-Control max-age expires at the next boundary) unlike the live
        trade/quote reads.
      parameters:
        - $ref: "#/components/parameters/VenuePrevClose"
        - $ref: "#/components/parameters/Symbol"
      responses:
        "200":
          description: Latest previous close.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/LastClose" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }
        "503": { $ref: "#/components/responses/ServiceUnavailable" }

  /v1/last/eod/{venue}/{symbol}:
    get:
      tags: [Live]
      operationId: getLastEod
      summary: End-of-day bar
      description: >
        The previous completed trading day's daily (1d) OHLCV bar for one
        instrument, bounded at 00:00 UTC — one call, no window/interval/cursor.
        The current, still-forming day is always excluded, so the value is
        stable within the day; on a weekend or holiday it returns the last
        session that traded (e.g. Friday's bar on a Sunday) rather than an empty
        result. Immutable until the next 00:00 UTC boundary, so it is
        edge-cacheable. DEX has no EOD product.
      parameters:
        - $ref: "#/components/parameters/VenueEod"
        - $ref: "#/components/parameters/Symbol"
      responses:
        "200":
          description: Latest completed end-of-day bar.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/LastEOD" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }
        "503": { $ref: "#/components/responses/ServiceUnavailable" }

  /v1/last/tvl/{chain}/{pair}:
    get:
      tags: [Live]
      operationId: getLastTVL
      summary: Aggregated DEX pool TVL
      parameters:
        - $ref: "#/components/parameters/Chain"
        - $ref: "#/components/parameters/Pair"
      responses:
        "200":
          description: Aggregated TVL for the chain/pair.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/LastTVL" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }
        "503": { $ref: "#/components/responses/ServiceUnavailable" }

  /v1/last/signals/{venue}/{symbol}:
    get:
      tags: [Signals]
      operationId: getLiveSignal
      summary: Live technical signal
      description: >
        Current technical-analysis signal for a stock, FX pair, crypto asset,
        or commodity: a decision-ready summary (strong_sell to strong_buy) with
        a score from -1 to +1, plus the oscillator and moving-average indicators
        behind it, each with its value(s) and vote. Indicators still in warmup
        are omitted, so the vote counts sum to the rows returned.
        `price.bar_status` is `forming` when a live tick is folded into the
        current bar, `closed` when the last settled bar is used.
      parameters:
        - $ref: "#/components/parameters/VenueSignals"
        - $ref: "#/components/parameters/Symbol"
        - name: interval
          in: query
          description: Bar interval the signal is computed on. Default 1h.
          schema:
            {
              type: string,
              enum: ["1m", "5m", "15m", "30m", "1h", "1d", "1w", "1mo"],
              default: "1h",
            }
      responses:
        "200":
          description: Live technical signal.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/LiveSignalResponse" }
        "400": { $ref: "#/components/responses/BadRequest" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "403": { $ref: "#/components/responses/Forbidden" }
        "404": { $ref: "#/components/responses/NotFound" }
        "422": { $ref: "#/components/responses/InsufficientHistory" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/snapshot/{venue}:
    get:
      tags: [Live]
      operationId: getSnapshot
      summary: Full market snapshot (gzip required)
      description: |
        Requires `Accept-Encoding: gzip`. Returns one entry per symbol the
        venue currently publishes, or scope it to a watchlist with `symbols`.
        Not paginated: a snapshot is the complete picture at `meta.as_of`.
      parameters:
        - $ref: "#/components/parameters/AcceptEncodingGzip"
        - $ref: "#/components/parameters/Venue"
        - name: symbols
          in: query
          description: >
            Comma-separated symbols (up to 250, case-insensitive). Omit to
            receive the full market. Any malformed symbol rejects the whole
            request with 400.
          schema: { type: string }
          example: BTCUSD,ETHUSD,SOLUSD
      responses:
        "200":
          description: Market snapshot.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/SnapshotResponse" }
        "400": { $ref: "#/components/responses/BadRequest" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "403": { $ref: "#/components/responses/Forbidden" }
        "404": { $ref: "#/components/responses/NotFound" }
        "406": { $ref: "#/components/responses/GzipRequired" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/last/movers/{venue}:
    get:
      tags:
        - Live
      operationId: getMovers
      summary: Market movers (top gainers and losers)
      description: >
        The biggest percentage risers and fallers of a venue, ranked from each symbol's live price
        against its stored previous close (the same value `/v1/last/close` serves). Change is
        therefore measured since the last close snapshot. Symbols with no close or no price are
        skipped and unchanged symbols appear in neither list. Not available for DEX.
      parameters:
        - $ref: "#/components/parameters/VenuePrevClose"
        - name: limit
          in: query
          description: Rows per list (gainers and losers each).
          schema:
            type: integer
            minimum: 1
            maximum: 100
            default: 20
        - name: direction
          in: query
          description: Which list to return. `gainers` or `losers` returns only that list (the other key is left out of `data`, which also cuts the payload); `all` or omitted returns both.
          schema:
            type: string
            enum:
              - all
              - gainers
              - losers
            default: all
      responses:
        "200":
          description: Ranked gainers and losers.
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/MoversResponse"
        "400":
          $ref: "#/components/responses/BadRequest"
        "401":
          $ref: "#/components/responses/Unauthorized"
        "403":
          $ref: "#/components/responses/Forbidden"
        "404":
          $ref: "#/components/responses/NotFound"
        "429":
          $ref: "#/components/responses/TooManyRequests"
        "503":
          $ref: "#/components/responses/ServiceUnavailable"

  /v1/last/most-active/{venue}:
    get:
      tags:
        - Live
      operationId: getMostActive
      summary: Most active symbols
      description: >
        The symbols with the highest volume so far in the current trading day (the UTC day for
        crypto, the regular session for US stocks), highest first. Stocks are ranked by raw volume;
        crypto by estimated dollar volume (volume times current price), because coin sizes are not
        comparable across symbols. `meta.ranked_by` says which. Available for stocks and crypto
        only. Before the first bar of a new day (stocks pre-market, weekends, holidays) it shows
        the previous day's totals.
      parameters:
        - name: venue
          in: path
          required: true
          description: Market venue.
          schema:
            type: string
            enum:
              - stocks
              - crypto
        - name: limit
          in: query
          description: Number of rows.
          schema:
            type: integer
            minimum: 1
            maximum: 100
            default: 20
      responses:
        "200":
          description: Most active symbols.
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/MostActiveResponse"
        "400":
          $ref: "#/components/responses/BadRequest"
        "401":
          $ref: "#/components/responses/Unauthorized"
        "403":
          $ref: "#/components/responses/Forbidden"
        "404":
          $ref: "#/components/responses/NotFound"
        "429":
          $ref: "#/components/responses/TooManyRequests"
        "503":
          $ref: "#/components/responses/ServiceUnavailable"

  /v1/convert/{from}/{to}:
    get:
      tags: [Convert]
      operationId: convertAmount
      summary: Convert between forex and crypto at live rates
      description: >
        Converts an amount between any supported forex or crypto symbols at
        the current mid-market rate. Converting an asset to itself returns
        rate 1.
      parameters:
        - name: from
          in: path
          required: true
          description: Source currency or token (e.g. `USD`, `BTC`). Case-insensitive.
          schema: { type: string }
        - name: to
          in: path
          required: true
          description: Target currency or token (e.g. `GBP`, `ETH`). Case-insensitive.
          schema: { type: string }
        - name: amount
          in: query
          description: Amount to convert. Defaults to 1 (returns just the rate).
          schema: { type: number, default: 1 }
      responses:
        "200":
          description: Converted amount and the rate applied.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/ConvertResult" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/stocks/search:
    get:
      tags: [Stocks]
      operationId: searchStocks
      summary: Ticker / company search
      parameters:
        - name: q
          in: query
          required: true
          description: Search string (ticker, company name, or CIK substring).
          schema: { type: string }
        - name: limit
          in: query
          description: Max results.
          schema: { type: integer, default: 25, maximum: 100 }
      responses:
        "200":
          description: Search results.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/StockSearchResponse" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/stocks/screener/{concept}/{period}:
    get:
      tags: [Stocks]
      operationId: getScreener
      summary: Cross-sectional screener (gzip required)
      description: "One XBRL concept/period across many companies. Requires `Accept-Encoding: gzip`."
      parameters:
        - $ref: "#/components/parameters/AcceptEncodingGzip"
        - name: concept
          in: path
          required: true
          description: XBRL concept tag, e.g. `Assets`, `NetIncomeLoss`.
          schema: { type: string }
        - name: period
          in: path
          required: true
          description: Reporting period, e.g. `CY2024Q1`, `CY2024FY`.
          schema: { type: string }
        - name: taxonomy
          in: query
          schema: { type: string, default: us-gaap }
        - name: unit
          in: query
          schema: { type: string, default: USD }
        - $ref: "#/components/parameters/Cursor"
        - $ref: "#/components/parameters/Limit"
      responses:
        "200":
          description: Screener rows.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/ScreenerResult" }
        "400": { $ref: "#/components/responses/BadRequest" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "406": { $ref: "#/components/responses/GzipRequired" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/stocks/{ticker}/profile:
    get:
      tags: [Stocks]
      operationId: getCompanyProfile
      summary: Company profile
      parameters:
        - $ref: "#/components/parameters/Ticker"
      responses:
        "200":
          description: Company profile.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/CompanyProfile" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/stocks/{ticker}/filings:
    get:
      tags: [Stocks]
      operationId: listFilings
      summary: SEC filings
      parameters:
        - $ref: "#/components/parameters/Ticker"
        - name: form
          in: query
          description: Comma-separated exact form types, e.g. `10-K,10-Q`.
          schema: { type: string }
        - name: from
          in: query
          description: Lower bound on filed date (YYYY-MM-DD).
          schema: { type: string, format: date }
        - name: to
          in: query
          description: Upper bound on filed date (YYYY-MM-DD).
          schema: { type: string, format: date }
        - $ref: "#/components/parameters/Cursor"
        - $ref: "#/components/parameters/Limit"
      responses:
        "200":
          description: Paginated filings.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/FilingsResponse" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/stocks/{ticker}/filings/{accession}:
    get:
      tags: [Stocks]
      operationId: getFiling
      summary: Filing detail
      parameters:
        - $ref: "#/components/parameters/Ticker"
        - $ref: "#/components/parameters/Accession"
      responses:
        "200":
          description: Filing detail.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/FilingDetail" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/stocks/{ticker}/filings/{accession}/sections:
    get:
      tags: [Stocks]
      operationId: getFilingSections
      summary: All filing sections
      parameters:
        - $ref: "#/components/parameters/Ticker"
        - $ref: "#/components/parameters/Accession"
      responses:
        "200":
          description: Extracted sections.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/FilingSections" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/stocks/{ticker}/filings/{accession}/sections/{section}:
    get:
      tags: [Stocks]
      operationId: getFilingSection
      summary: One filing section
      parameters:
        - $ref: "#/components/parameters/Ticker"
        - $ref: "#/components/parameters/Accession"
        - name: section
          in: path
          required: true
          description: Section code, e.g. `business`, `risk-factors`, `mda`, `legal-proceedings`, `market-risk`.
          schema: { type: string }
      responses:
        "200":
          description: Section text.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/FilingSectionDetail" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/stocks/{ticker}/risk-factors-diff:
    get:
      tags: [Stocks]
      operationId: getRiskFactorsDiff
      summary: Year-over-year risk-factor diff
      parameters:
        - $ref: "#/components/parameters/Ticker"
      responses:
        "200":
          description: Item 1A diff between the two most recent 10-Ks.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/RiskFactorsDiff" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/stocks/{ticker}/ratios:
    get:
      tags: [Stocks]
      operationId: getRatios
      summary: Fundamental ratios
      parameters:
        - $ref: "#/components/parameters/Ticker"
      responses:
        "200":
          description: Latest ratios plus history.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/Ratios" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/stocks/{ticker}/earnings:
    get:
      tags: [Stocks]
      operationId: listEarnings
      summary: Earnings releases
      parameters:
        - $ref: "#/components/parameters/Ticker"
        - $ref: "#/components/parameters/Cursor"
        - $ref: "#/components/parameters/Limit"
      responses:
        "200":
          description: Paginated earnings-release filings.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/EventFilingsResponse" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/stocks/{ticker}/financials:
    get:
      tags: [Stocks]
      operationId: getFinancials
      summary: Full XBRL financials (gzip required)
      description: "Full XBRL concept bundle for a company. Requires `Accept-Encoding: gzip`."
      parameters:
        - $ref: "#/components/parameters/AcceptEncodingGzip"
        - $ref: "#/components/parameters/Ticker"
      responses:
        "200":
          description: XBRL financials.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/Financials" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "406": { $ref: "#/components/responses/GzipRequired" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/stocks/{ticker}/financials/{concept}:
    get:
      tags: [Stocks]
      operationId: getFinancialConcept
      summary: One XBRL concept (gzip required)
      description: "One XBRL concept's full time series. Requires `Accept-Encoding: gzip`."
      parameters:
        - $ref: "#/components/parameters/AcceptEncodingGzip"
        - $ref: "#/components/parameters/Ticker"
        - name: concept
          in: path
          required: true
          description: XBRL concept tag, e.g. `Assets`.
          schema: { type: string }
        - name: taxonomy
          in: query
          schema: { type: string, default: us-gaap }
      responses:
        "200":
          description: Concept time series.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/FinancialConcept" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "406": { $ref: "#/components/responses/GzipRequired" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/stocks/{ticker}/insiders:
    get:
      tags: [Stocks]
      operationId: listInsiders
      summary: Insider transactions (Form 3/4/5)
      parameters:
        - $ref: "#/components/parameters/Ticker"
        - $ref: "#/components/parameters/Cursor"
        - name: limit
          in: query
          description: Page size (lower cap due to XML parsing).
          schema: { type: integer, default: 10, maximum: 25 }
      responses:
        "200":
          description: Paginated insider transactions.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/InsidersResponse" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/stocks/{ticker}/ownership:
    get:
      tags: [Stocks]
      operationId: listOwnership
      summary: Schedule 13D/13G ownership filings
      parameters:
        - $ref: "#/components/parameters/Ticker"
        - $ref: "#/components/parameters/Cursor"
        - $ref: "#/components/parameters/Limit"
      responses:
        "200":
          description: Paginated ownership filings.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/OwnershipResponse" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/stocks/{ticker}/events:
    get:
      tags: [Stocks]
      operationId: listEvents
      summary: 8-K material events
      parameters:
        - $ref: "#/components/parameters/Ticker"
        - name: item
          in: query
          description: Filter by 8-K item code, e.g. `2.02`.
          schema: { type: string }
        - $ref: "#/components/parameters/Cursor"
        - $ref: "#/components/parameters/Limit"
      responses:
        "200":
          description: Paginated 8-K events.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/EventFilingsResponse" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/stocks/{ticker}/compensation:
    get:
      tags: [Stocks]
      operationId: listCompensation
      summary: DEF 14A proxy / compensation filings
      parameters:
        - $ref: "#/components/parameters/Ticker"
        - $ref: "#/components/parameters/Cursor"
        - $ref: "#/components/parameters/Limit"
      responses:
        "200":
          description: Paginated proxy filings.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/CompensationResponse" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/filers/{filer}/holdings:
    get:
      tags: [Filers]
      operationId: getFilerHoldings
      summary: 13F institutional holdings
      parameters:
        - name: filer
          in: path
          required: true
          description: Institutional filer CIK (numeric) or ticker.
          schema: { type: string }
        - $ref: "#/components/parameters/Cursor"
        - $ref: "#/components/parameters/Limit"
      responses:
        "200":
          description: Latest 13F-HR positions.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/Holdings" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/economic-calendar:
    get:
      tags: [EconomicCalendar]
      operationId: getEconomicCalendar
      summary: US economic calendar
      parameters:
        - name: from
          in: query
          description: Lower bound (YYYY-MM-DD or RFC3339). Default now.
          schema: { type: string }
        - name: to
          in: query
          description: Upper bound. Default from + 30 days.
          schema: { type: string }
        - name: country
          in: query
          schema: { type: string, default: US }
        - name: impact
          in: query
          schema: { type: string, enum: [low, medium, high] }
        - name: agency
          in: query
          schema: { type: string, enum: [BLS, BEA, Census, Fed, DOL, EIA] }
        - name: event_id
          in: query
          description: Filter to a recurring event, e.g. `us_cpi`.
          schema: { type: string }
        - name: limit
          in: query
          schema: { type: integer, default: 100, minimum: 1, maximum: 500 }
      responses:
        "200":
          description: Economic events.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/EconomicCalendarResponse" }
        "400": { $ref: "#/components/responses/BadRequest" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/markets:
    get:
      tags: [Markets]
      operationId: listMarkets
      summary: List markets
      parameters:
        - $ref: "#/components/parameters/Region"
      responses:
        "200":
          description: Market catalog.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/MarketsListResponse" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/markets/status:
    get:
      tags: [Markets]
      operationId: getAllMarketStatus
      summary: All market statuses
      parameters:
        - $ref: "#/components/parameters/Region"
      responses:
        "200":
          description: Open/closed snapshot for every market.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/MarketsStatusAllResponse" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/markets/{market}/status:
    get:
      tags: [Markets]
      operationId: getMarketStatus
      summary: One market status
      parameters:
        - $ref: "#/components/parameters/Market"
      responses:
        "200":
          description: Open/closed snapshot.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/MarketStatusResponse" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/markets/{market}/hours:
    get:
      tags: [Markets]
      operationId: getMarketHours
      summary: Market trading hours
      parameters:
        - $ref: "#/components/parameters/Market"
      responses:
        "200":
          description: Weekly schedule.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/MarketHoursResponse" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/markets/{market}/calendar:
    get:
      tags: [Markets]
      operationId: getMarketCalendar
      summary: Market holiday calendar
      parameters:
        - $ref: "#/components/parameters/Market"
        - name: from
          in: query
          schema: { type: string, format: date }
        - name: to
          in: query
          schema: { type: string, format: date }
      responses:
        "200":
          description: Holidays / half-days in range.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/MarketCalendarResponse" }
        "400": { $ref: "#/components/responses/BadRequest" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/hist/stocks/{ticker}/bars:
    get:
      tags: [Stocks]
      operationId: getStockBars
      summary: Historical stock OHLCV bars (gzip required)
      description: |
        Requires `Accept-Encoding: gzip`. All intervals (1m through 1mo)
        cover regular-session bars back to 2000, ascending (oldest first;
        page forward with `cursor`). `1d` is one US trading session, `1w`
        is Monday-aligned, `1mo` is a calendar month. Prices are as-traded
        (not adjusted for splits or dividends).
      parameters:
        - $ref: "#/components/parameters/AcceptEncodingGzip"
        - $ref: "#/components/parameters/Ticker"
        - $ref: "#/components/parameters/BarStart"
        - $ref: "#/components/parameters/BarEnd"
        - $ref: "#/components/parameters/BarIntervalStocks"
        - $ref: "#/components/parameters/Cursor"
        - name: limit
          in: query
          schema: { type: integer, default: 1000, maximum: 1000 }
      responses:
        "200":
          description: OHLCV bars.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/BarsResponse" }
        "400": { $ref: "#/components/responses/BadRequest" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "406": { $ref: "#/components/responses/GzipRequired" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/hist/forex/{pair}/bars:
    get:
      tags: [Forex]
      operationId: getForexBars
      summary: Historical forex OHLC bars (gzip required)
      description: |
        Requires `Accept-Encoding: gzip`. `v` is a per-bar tick count where
        the upstream reports one, else 0. When `start` predates upstream
        coverage the API returns `422` with code `data_unavailable` and an
        `earliest` field carrying the first date it can serve.
      parameters:
        - $ref: "#/components/parameters/AcceptEncodingGzip"
        - $ref: "#/components/parameters/Pair"
        - $ref: "#/components/parameters/BarStartRequired"
        - $ref: "#/components/parameters/BarEnd"
        - $ref: "#/components/parameters/BarInterval"
        - $ref: "#/components/parameters/BarOrder"
        - $ref: "#/components/parameters/Cursor"
        - name: limit
          in: query
          schema: { type: integer, default: 1000, maximum: 2000 }
      responses:
        "200":
          description: OHLC bars.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/BarsResponse" }
        "400": { $ref: "#/components/responses/BadRequest" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "406": { $ref: "#/components/responses/GzipRequired" }
        "422": { $ref: "#/components/responses/DataUnavailable" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/hist/commodities/{symbol}/bars:
    get:
      tags: [Commodities]
      operationId: getCommodityBars
      summary: Historical commodity OHLC bars (gzip required)
      description: |
        Requires `Accept-Encoding: gzip`. Covers precious metals
        (XAUUSD, XAGUSD, XPTUSD, XPDUSD), energy, and industrials
        (e.g. XCUUSD copper, USOUSD WTI, NATGAS). Volume may be 0 for
        instruments without a reported tick count. When `start` predates
        upstream coverage the API returns `422` with code
        `data_unavailable` and an `earliest` field carrying the first
        date it can serve.
      parameters:
        - $ref: "#/components/parameters/AcceptEncodingGzip"
        - name: symbol
          in: path
          required: true
          description: Commodity code (4-12 upper alphanumerics), e.g. `XAUUSD`, `XCUUSD`.
          schema: { type: string }
        - $ref: "#/components/parameters/BarStartRequired"
        - $ref: "#/components/parameters/BarEnd"
        - $ref: "#/components/parameters/BarIntervalCommodities"
        - $ref: "#/components/parameters/BarOrder"
        - $ref: "#/components/parameters/Cursor"
        - name: limit
          in: query
          schema: { type: integer, default: 1000, maximum: 2000 }
      responses:
        "200":
          description: OHLC bars.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/BarsResponse" }
        "400": { $ref: "#/components/responses/BadRequest" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "406": { $ref: "#/components/responses/GzipRequired" }
        "422": { $ref: "#/components/responses/DataUnavailable" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/hist/crypto/{symbol}/bars:
    get:
      tags: [Crypto]
      operationId: getCryptoBars
      summary: Historical crypto OHLCV bars (gzip required)
      description: |
        Requires `Accept-Encoding: gzip`. When `start` predates all upstreams'
        coverage the API returns `422` with code `data_unavailable` and an
        `earliest` field.
      parameters:
        - $ref: "#/components/parameters/AcceptEncodingGzip"
        - name: symbol
          in: path
          required: true
          description: USD-quoted crypto symbol, e.g. `BTCUSD`.
          schema: { type: string }
        - $ref: "#/components/parameters/BarStartRequired"
        - $ref: "#/components/parameters/BarEnd"
        - $ref: "#/components/parameters/BarInterval"
        - $ref: "#/components/parameters/Cursor"
        - name: limit
          in: query
          schema: { type: integer, default: 1000, maximum: 5000 }
      responses:
        "200":
          description: OHLCV bars.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/BarsResponse" }
        "400": { $ref: "#/components/responses/BadRequest" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "406": { $ref: "#/components/responses/GzipRequired" }
        "422": { $ref: "#/components/responses/DataUnavailable" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/hist/dex/{symbol}/bars:
    get:
      tags: [DEX]
      operationId: getDexBars
      summary: Historical DEX OHLCV bars (gzip required)
      description: |
        Requires `Accept-Encoding: gzip`. OHLCV bars for USD-quoted DEX
        tokens, aggregated from executed on-chain swaps. Buckets with zero
        trades are omitted (the series is sparse). Requires the `dex_defi`
        market on paid keys. When `start` predates the earliest indexed bar
        the API returns `422` with code `data_unavailable` and an `earliest`
        field. `v` is base-asset volume (e.g. ETH for ETHUSD), not USD value.
      parameters:
        - $ref: "#/components/parameters/AcceptEncodingGzip"
        - name: symbol
          in: path
          required: true
          description: Base token plus the `USD` suffix, e.g. `ETHUSD`. Case-insensitive.
          schema: { type: string }
        - $ref: "#/components/parameters/BarStartRequired"
        - $ref: "#/components/parameters/BarEnd"
        - $ref: "#/components/parameters/BarInterval"
        - $ref: "#/components/parameters/Cursor"
        - name: limit
          in: query
          schema: { type: integer, default: 1000, maximum: 5000 }
      responses:
        "200":
          description: OHLCV bars.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/BarsResponse" }
        "400": { $ref: "#/components/responses/BadRequest" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "403": { $ref: "#/components/responses/Forbidden" }
        "404": { $ref: "#/components/responses/NotFound" }
        "406": { $ref: "#/components/responses/GzipRequired" }
        "422": { $ref: "#/components/responses/DataUnavailable" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/hist/{venue}/{symbol}/signals:
    get:
      tags: [Signals]
      operationId: getSignalHistory
      summary: Historical technical-signal series
      description: >
        The technical-analysis signal as a time series, one point per bar:
        summary, score, close, vote counts, the oscillator and moving-average
        breakdown with each indicator's value and vote, and any discrete
        events (golden_cross, death_cross, macd_cross_up, macd_cross_down)
        that fired on that bar.
        Same computation as the live signal, so a historical point is identical
        to the one that fired live. `t` is the bar-open time in Unix epoch
        milliseconds, the same as the `/v1/hist/*` bars routes.
      parameters:
        - $ref: "#/components/parameters/VenueSignals"
        - $ref: "#/components/parameters/Symbol"
        - name: interval
          in: query
          description: Bar interval each point is computed on. Default 1d.
          schema:
            {
              type: string,
              enum: ["1m", "5m", "15m", "30m", "1h", "1d", "1w", "1mo"],
              default: "1d",
            }
        - name: limit
          in: query
          description: Return the most recent N points (newest last).
          schema: { type: integer, default: 100, maximum: 1000 }
      responses:
        "200":
          description: Historical signal series.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/SignalHistoryResponse" }
        "400": { $ref: "#/components/responses/BadRequest" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "403": { $ref: "#/components/responses/Forbidden" }
        "404": { $ref: "#/components/responses/NotFound" }
        "422": { $ref: "#/components/responses/InsufficientHistory" }
        "429": { $ref: "#/components/responses/TooManyRequests" }

  /v1/fnd/dex/wallet/{chain}/{address}:
    get:
      tags: [DEX]
      operationId: getWalletPortfolio
      summary: Wallet token portfolio
      parameters:
        - $ref: "#/components/parameters/Chain"
        - name: address
          in: path
          required: true
          description: 0x-prefixed 40-hex-char wallet address.
          schema: { type: string }
      responses:
        "200":
          description: Token holdings.
          content:
            application/json:
              schema: { $ref: "#/components/schemas/WalletPortfolio" }
        "400": { $ref: "#/components/responses/BadRequest" }
        "401": { $ref: "#/components/responses/Unauthorized" }
        "404": { $ref: "#/components/responses/NotFound" }
        "429": { $ref: "#/components/responses/TooManyRequests" }
        "503": { $ref: "#/components/responses/ServiceUnavailable" }

components:
  securitySchemes:
    ApiKeyHeader:
      type: apiKey
      in: header
      name: X-API-Key
    ApiKeyQuery:
      type: apiKey
      in: query
      name: api_key

  parameters:
    CatalogAssetClassFilter:
      name: asset_class
      in: query
      description: >-
        Case-insensitive asset-class filter: stocks, crypto, dex, forex or
        commodities. Unknown values return an empty list.
      schema: { type: string }
      example: stocks
    CatalogVenue:
      name: venue
      in: query
      description: >-
        Case-insensitive exact match on the catalog row's venue.
        Use a venue returned by the catalog; unknown values return an empty list.
      schema: { type: string }
    CatalogQuery:
      name: q
      in: query
      description: Case-insensitive substring search over symbol or name.
      schema: { type: string }
      example: BTC
    CatalogLimit:
      name: limit
      in: query
      description: >-
        Page size. Omitted, non-numeric or non-positive values use 50;
        values above 200 are clamped to 200.
      schema: { type: integer, minimum: 1, maximum: 200, default: 50 }
    CatalogAssetClass:
      name: asset_class
      in: path
      required: true
      description: Asset-class segment of the catalog slug; case-insensitive.
      schema: { type: string, enum: [stocks, crypto, forex, commodities] }
    CatalogSymbol:
      name: symbol
      in: path
      required: true
      description: >-
        Symbol segment returned by the catalog, normalized to uppercase.
        Encode this segment separately from the slash-delimited slug.
      schema: { type: string }
      example: AAPL
    CatalogChain:
      name: chain
      in: path
      required: true
      description: Chain segment from a DEX catalog slug; case-insensitive.
      schema: { type: string }
      example: eth

    Venue:
      name: venue
      in: path
      required: true
      description: Market venue.
      schema: { type: string, enum: [stocks, crypto, forex, dex] }
    VenuePrevClose:
      name: venue
      in: path
      required: true
      description: >
        Market venue for previous close. DEX has no previous-close product;
        commodities does (unlike the live trade/quote venues).
      schema: { type: string, enum: [stocks, crypto, forex, commodities] }
    VenueSignals:
      name: venue
      in: path
      required: true
      description: Market venue for technical signals.
      schema: { type: string, enum: [stocks, crypto, forex, commodities] }
    VenueEod:
      name: venue
      in: path
      required: true
      description: >
        Market venue for end-of-day. Same coverage as previous close
        (stocks, crypto, forex, commodities); DEX has no EOD product.
      schema: { type: string, enum: [stocks, crypto, forex, commodities] }
    Symbol:
      name: symbol
      in: path
      required: true
      description: Symbol, normalized to the venue's canonical form.
      schema: { type: string }
    Chain:
      name: chain
      in: path
      required: true
      description: EVM chain.
      schema: { type: string, enum: [eth, base, arbitrum, bsc, polygon] }
    Pair:
      name: pair
      in: path
      required: true
      description: DEX pair (TOKEN0-TOKEN1) or FX pair (e.g. EURUSD).
      schema: { type: string }
    Ticker:
      name: ticker
      in: path
      required: true
      description: US equity ticker (case-insensitive).
      schema: { type: string }
    Accession:
      name: accession
      in: path
      required: true
      description: SEC accession number.
      schema: { type: string }
    Market:
      name: market
      in: path
      required: true
      description: Market slug, e.g. `us_equities`, `nyse`, `forex`, `crypto`.
      schema: { type: string }
    Region:
      name: region
      in: query
      description: Region filter.
      schema:
        type: string
        enum: [north_america, europe, asia_pacific, latam, global]
    Cursor:
      name: cursor
      in: query
      description: Opaque pagination cursor from a previous response's `meta.next_cursor`.
      schema: { type: string }
    Limit:
      name: limit
      in: query
      description: Page size. Default 50, max 200.
      schema: { type: integer, default: 50, maximum: 200 }
    BarStart:
      name: start
      in: query
      description: Inclusive lower bound (YYYY-MM-DD or RFC3339).
      schema: { type: string }
    BarStartRequired:
      name: start
      in: query
      required: true
      description: Inclusive lower bound (YYYY-MM-DD or RFC3339). Required.
      schema: { type: string }
    BarEnd:
      name: end
      in: query
      description: Inclusive upper bound. Default now.
      schema: { type: string }
    BarInterval:
      name: interval
      in: query
      description: >
        Bar interval. Default 1m. All `/v1/hist/*` routes accept the same set:
        intraday (1m–1h), plus `1d` (UTC calendar day), `1w` (Monday-aligned),
        and `1mo` (calendar month). Coarser bars are aggregated from 1-minute
        history; a `1d`/`1w`/`1mo` bar opens at midnight UTC of the period start.
      schema:
        {
          type: string,
          enum: ["1m", "5m", "15m", "30m", "1h", "1d", "1w", "1mo"],
          default: "1m",
        }
    BarIntervalCommodities:
      name: interval
      in: query
      description: >
        Bar interval. Default 1m. Full set: intraday (1m–1h), `1d` (daily
        candle with a true session boundary), `1w` (Monday-aligned), and `1mo`
        (calendar month), all aggregated from 1-minute history.
      schema:
        {
          type: string,
          enum: ["1m", "5m", "15m", "30m", "1h", "1d", "1w", "1mo"],
          default: "1m",
        }
    BarIntervalStocks:
      name: interval
      in: query
      description: >
        Bar interval. Default 1m. Stocks additionally support `1d`, `1w`
        (Monday-aligned), and `1mo` (calendar month), all aggregated from
        the same 1-minute history back to 2000. Prices are as-traded (not
        split/dividend adjusted). A `1d`/`1w`/`1mo` bar opens at midnight
        UTC of the period start.
      schema:
        {
          type: string,
          enum: ["1m", "5m", "15m", "30m", "1h", "1d", "1w", "1mo"],
          default: "1m",
        }
    BarOrder:
      name: order
      in: query
      description: >
        Sort direction by time. `asc` (default) returns oldest→newest.
        `desc` returns newest→oldest and anchors the first page at the
        most recent bar, so fetching the latest N costs one request
        instead of paging the whole window. Supported on commodities and
        forex only; other historical routes are ascending-only.
      schema: { type: string, enum: [asc, desc], default: asc }
    AcceptEncodingGzip:
      name: Accept-Encoding
      in: header
      required: true
      description: Must include `gzip`; this endpoint returns 406 otherwise.
      schema: { type: string, enum: [gzip] }

  responses:
    SymbolCatalogList:
      description: >-
        Active catalog entries. An empty result is a successful response.
        A malformed cursor returns 400.
      headers:
        Cache-Control:
          description: Cache policy for the catalog list.
          schema: { type: string }
          example: "public, max-age=300"
      content:
        application/json:
          schema: { $ref: "#/components/schemas/SymbolCatalogListResponse" }
    SymbolCatalogDetail:
      description: Catalog entry, including inactive or delisted entries when present.
      headers:
        Cache-Control:
          description: Cache policy for a catalog entry.
          schema: { type: string }
          example: "public, max-age=3600"
      content:
        application/json:
          schema: { $ref: "#/components/schemas/SymbolCatalogDetailResponse" }
    CatalogInternalError:
      description: The catalog could not be read.
      content:
        application/json:
          schema: { $ref: "#/components/schemas/Error" }
          example: { error: "internal error" }

    BadRequest:
      description: Invalid request parameters.
      content:
        application/json:
          schema: { $ref: "#/components/schemas/Error" }
    Unauthorized:
      description: Missing or invalid API key.
      content:
        application/json:
          schema: { $ref: "#/components/schemas/Error" }
    NotFound:
      description: Resource not found.
      content:
        application/json:
          schema: { $ref: "#/components/schemas/Error" }
    GzipRequired:
      description: "This endpoint requires `Accept-Encoding: gzip`."
      content:
        application/json:
          schema: { $ref: "#/components/schemas/Error" }
    DataUnavailable:
      description: No upstream has data for the requested window.
      content:
        application/json:
          schema: { $ref: "#/components/schemas/Error" }
    InsufficientHistory:
      description: Not enough bars to compute a signal for this symbol and interval.
      content:
        application/json:
          schema: { $ref: "#/components/schemas/Error" }
    TooManyRequests:
      description: Rate limit exceeded.
      headers:
        Retry-After:
          description: Seconds to wait before retrying.
          schema: { type: integer }
      content:
        application/json:
          schema: { $ref: "#/components/schemas/Error" }
    ServiceUnavailable:
      description: Upstream or snapshot temporarily unavailable.
      content:
        application/json:
          schema: { $ref: "#/components/schemas/Error" }
    Forbidden:
      description: Valid key, but the plan or tier does not include this market/venue.
      content:
        application/json:
          schema: { $ref: "#/components/schemas/Error" }

  schemas:
    CatalogInstrument:
      type: object
      description: >-
        Instrument reference metadata. Optional string fields are omitted
        when unavailable. Catalog status does not establish the availability
        or freshness of live prices or historical bars.
      required: [slug, asset_class, venue, symbol, status, details]
      properties:
        slug:
          type: string
          description: >-
            Canonical identifier with literal slash separators: asset_class/symbol,
            or dex/chain/symbol for DEX. Encode segments separately when building
            a detail URL.
          example: stocks/AAPL
        asset_class:
          type: string
          enum: [stocks, crypto, dex, forex, commodities]
        venue:
          type: string
          description: Venue tag stored in the catalog.
        market_id:
          type: string
          description: Market identifier, when mapped.
        symbol:
          type: string
          description: Canonical instrument symbol.
          example: AAPL
        name:
          type: string
        base_asset:
          type: string
        quote_asset:
          type: string
        currency:
          type: string
        status:
          type: string
          enum: [active, inactive, delisted]
        details:
          type: object
          description: Asset-specific metadata; an empty object when unavailable.
          additionalProperties: true
    SymbolCatalogListResponse:
      type: object
      required: [data, meta]
      properties:
        data:
          type: array
          items: { $ref: "#/components/schemas/CatalogInstrument" }
        meta:
          type: object
          required: [as_of, count]
          properties:
            as_of:
              type: string
              format: date-time
              description: Response generation time, not the underlying market data timestamp.
            count:
              type: integer
              minimum: 0
              description: Number of entries in this page, not the total catalog size.
            next_cursor:
              type: string
              description: Opaque cursor for the next page; omitted on the last page.
    SymbolCatalogDetailResponse:
      type: object
      required: [data, meta]
      properties:
        data: { $ref: "#/components/schemas/CatalogInstrument" }
        meta:
          type: object
          required: [as_of]
          properties:
            as_of:
              type: string
              format: date-time
              description: Response generation time, not the underlying market data timestamp.

    Error:
      type: object
      required: [error]
      properties:
        error:
          type: string
          description: Machine-readable error code, e.g. `unknown_ticker`.
        message:
          type: string
        retry_after:
          type: number
          description: Present on rate-limit errors.
      additionalProperties: true

    ListMeta:
      type: object
      properties:
        next_cursor:
          type: string
          description: Cursor for the next page; absent on the last page.
        as_of:
          type: string
          format: date-time
        total:
          type: integer

    MoversResponse:
      type: object
      properties:
        data:
          type: object
          properties:
            gainers:
              type: array
              description: Largest percentage increases first. Omitted when `direction=losers`.
              items:
                $ref: "#/components/schemas/Mover"
            losers:
              type: array
              description: Largest percentage decreases first. Omitted when `direction=gainers`.
              items:
                $ref: "#/components/schemas/Mover"
        meta:
          type: object
          properties:
            as_of:
              type: string
              format: date-time
            venue:
              type: string
            limit:
              type: integer
              description: Rows per list in this response.
    Mover:
      type: object
      properties:
        symbol:
          type: string
        price:
          type: string
          description: Live price (decimal string).
        prev_close:
          type: string
          description: Stored previous close (decimal string).
        change:
          type: string
          description: price minus prev_close (decimal string).
        change_pct:
          type: number
          description: Percentage change vs prev_close, rounded to 4 decimals.
        day_volume:
          type: number
          description: Volume so far in the current trading day (our own aggregated volume, the same that feeds our OHLCV bars). The day is the UTC day for crypto, forex and commodities, and the regular session for US stocks. Not a rolling 24 hours. Omitted when no volume is available yet. Until the next day's first bar (stocks pre-market, weekends, holidays) this is still the previous day's full total.
        prev_day_volume:
          type: number
          description: Full volume of the trading day before the day_volume day. Omitted when unavailable.

    MostActiveResponse:
      type: object
      properties:
        data:
          type: array
          description: Highest volume first.
          items:
            $ref: "#/components/schemas/MostActiveRow"
        meta:
          type: object
          properties:
            as_of:
              type: string
              format: date-time
            venue:
              type: string
            limit:
              type: integer
              description: Rows in this response at most.
            ranked_by:
              type: string
              enum:
                - volume
                - volume_usd
              description: volume is raw day volume (stocks); volume_usd is estimated dollar volume (crypto).

    MostActiveRow:
      type: object
      properties:
        symbol:
          type: string
        price:
          type: string
          description: Live price (decimal string).
        day_volume:
          type: number
          description: Volume so far in the current trading day (not a rolling 24 hours).
        prev_day_volume:
          type: number
          description: Full volume of the trading day before. Omitted when unavailable.
        prev_close:
          type: string
          description: Stored previous close (decimal string). Omitted when the symbol has no close.
        change:
          type: string
          description: price minus prev_close (decimal string). Omitted without a close.
        change_pct:
          type: number
          description: Percentage change vs prev_close, rounded to 4 decimals. Omitted without a close.

    LastTrade:
      type: object
      properties:
        s: { type: string, description: Symbol. }
        p: { type: string, description: Last trade price. }
        P: { type: string, description: Last trade size. }
        t:
          {
            type: integer,
            format: int64,
            description: Unix epoch milliseconds.,
          }

    LastQuote:
      type: object
      properties:
        b: { type: string, description: Bid price. }
        B: { type: string, description: Bid size. }
        a: { type: string, description: Ask price. }
        A: { type: string, description: Ask size. }
        t: { type: integer, format: int64 }

    LastClose:
      type: object
      properties:
        s: { type: string, description: Symbol. }
        c: { type: string, description: Close price. }
        d: { type: string, description: "UTC close date, YYYY-MM-DD." }
        t:
          {
            type: integer,
            format: int64,
            description: "Close-boundary Unix epoch milliseconds.",
          }

    LastEOD:
      type: object
      properties:
        s: { type: string, description: Symbol. }
        d: { type: string, description: "UTC date of the bar, YYYY-MM-DD." }
        o: { type: number, description: Open. }
        h: { type: number, description: High. }
        l: { type: number, description: Low. }
        c: { type: number, description: Close. }
        v:
          {
            type: number,
            description: "Volume as reported by the source (may be 0 for some FX).",
          }
        t:
          {
            type: integer,
            format: int64,
            description: "Bar open time (00:00 UTC), Unix epoch milliseconds.",
          }

    LastTVL:
      type: object
      properties:
        chain: { type: string }
        pair: { type: string }
        usd: { type: string }
        r0: { type: string, description: Reserve of token0. }
        r1: { type: string, description: Reserve of token1. }
        n: { type: integer, description: Pools aggregated. }
        v: { type: integer }
        t: { type: integer, format: int64 }

    SnapshotEntry:
      type: object
      properties:
        s: { type: string, description: Symbol. }
        p: { type: string, description: Last trade price. }
        P: { type: string, description: Last trade size (0 for forex/stocks). }
        b: { type: string, description: Best bid price. }
        B: { type: string, description: Best bid size (0 for forex/stocks). }
        a: { type: string, description: Best ask price. }
        A: { type: string, description: Best ask size (0 for forex/stocks). }
        t:
          {
            type: integer,
            format: int64,
            description: Unix epoch milliseconds.,
          }

    SnapshotResponse:
      type: object
      properties:
        data:
          type: array
          items: { $ref: "#/components/schemas/SnapshotEntry" }
        meta:
          type: object
          properties:
            as_of: { type: string, format: date-time }
            venue: { type: string }
            count: { type: integer }

    ConvertResult:
      type: object
      properties:
        from: { type: string, description: Echo of the source symbol. }
        to: { type: string, description: Echo of the target symbol. }
        amount: { type: number, description: Amount sent in. }
        rate:
          { type: number, description: Rate applied; amount × rate = result. }
        result: { type: number, description: Converted amount. }
        t:
          type: integer
          format: int64
          description: Timestamp of the underlying market rate (Unix epoch ms).

    StockSearchResult:
      type: object
      properties:
        ticker: { type: string }
        name: { type: string }
        cik: { type: string }
        exchange: { type: string }

    StockSearchResponse:
      type: object
      properties:
        data:
          type: array
          items: { $ref: "#/components/schemas/StockSearchResult" }
        meta: { $ref: "#/components/schemas/ListMeta" }

    CompanyProfile:
      type: object
      properties:
        ticker: { type: string }
        cik: { type: string }
        name: { type: string }
        exchanges: { type: array, items: { type: string } }
        other_tickers: { type: array, items: { type: string } }
        sic_code: { type: string }
        sic_description: { type: string }
        entity_type: { type: string }
        fiscal_year_end: { type: string, description: MMDD. }

    Filing:
      type: object
      properties:
        accession: { type: string }
        form: { type: string }
        filed_at: { type: string, format: date }
        period_end: { type: string, format: date }
        accepted_at: { type: string, format: date-time }
        items: { type: string, description: Comma-separated 8-K item codes. }
        primary_document_url: { type: string }
        description: { type: string }
        has_xbrl: { type: boolean }

    FilingDetail:
      allOf:
        - $ref: "#/components/schemas/Filing"
        - type: object
          properties:
            ticker: { type: string }
            cik: { type: string }
            archive_url: { type: string }
            files: { type: array, items: { type: string } }

    FilingsResponse:
      type: object
      properties:
        data:
          type: array
          items: { $ref: "#/components/schemas/Filing" }
        meta: { $ref: "#/components/schemas/ListMeta" }

    FilingSection:
      type: object
      properties:
        section: { type: string }
        content: { type: string }

    FilingSections:
      type: object
      properties:
        ticker: { type: string }
        cik: { type: string }
        accession: { type: string }
        form: { type: string }
        filed_at: { type: string, format: date }
        sections:
          type: array
          items: { $ref: "#/components/schemas/FilingSection" }

    FilingSectionDetail:
      type: object
      properties:
        ticker: { type: string }
        cik: { type: string }
        accession: { type: string }
        form: { type: string }
        filed_at: { type: string, format: date }
        section: { type: string }
        content: { type: string }

    FilingRef:
      type: object
      properties:
        accession: { type: string }
        form: { type: string }
        filed_at: { type: string, format: date }
        period_end: { type: string, format: date }

    DiffPair:
      type: object
      properties:
        before: { type: string }
        after: { type: string }

    SectionDiffStats:
      type: object
      properties:
        before_paragraphs: { type: integer }
        after_paragraphs: { type: integer }
        unchanged_count: { type: integer }
        added_count: { type: integer }
        removed_count: { type: integer }
        modified_count: { type: integer }

    SectionDiff:
      type: object
      properties:
        added: { type: array, items: { type: string } }
        removed: { type: array, items: { type: string } }
        modified:
          type: array
          items: { $ref: "#/components/schemas/DiffPair" }
        stats: { $ref: "#/components/schemas/SectionDiffStats" }

    RiskFactorsDiff:
      type: object
      properties:
        ticker: { type: string }
        cik: { type: string }
        current: { $ref: "#/components/schemas/FilingRef" }
        previous: { $ref: "#/components/schemas/FilingRef" }
        diff: { $ref: "#/components/schemas/SectionDiff" }

    MetricValue:
      type: object
      properties:
        value: { type: number }
        unit: { type: string }
        period_start: { type: string, format: date }
        period_end: { type: string, format: date }
        fiscal_year: { type: integer }
        fiscal_period: { type: string }
        form: { type: string }
        accession: { type: string }
        filed_at: { type: string, format: date }

    ConceptBlock:
      type: object
      properties:
        taxonomy: { type: string }
        concept: { type: string }
        label: { type: string }
        description: { type: string }
        series:
          type: array
          items: { $ref: "#/components/schemas/MetricValue" }

    Financials:
      type: object
      properties:
        ticker: { type: string }
        cik: { type: string }
        name: { type: string }
        concepts:
          type: array
          items: { $ref: "#/components/schemas/ConceptBlock" }

    FinancialConcept:
      type: object
      properties:
        ticker: { type: string }
        cik: { type: string }
        taxonomy: { type: string }
        concept: { type: string }
        label: { type: string }
        description: { type: string }
        series:
          type: array
          items: { $ref: "#/components/schemas/MetricValue" }

    ScreenerRow:
      type: object
      properties:
        cik: { type: string }
        name: { type: string }
        value: { type: number }
        unit: { type: string }
        period_end: { type: string, format: date }
        accession: { type: string }

    ScreenerResult:
      type: object
      properties:
        taxonomy: { type: string }
        concept: { type: string }
        period: { type: string }
        unit: { type: string }
        label: { type: string }
        rows:
          type: array
          items: { $ref: "#/components/schemas/ScreenerRow" }
        meta: { $ref: "#/components/schemas/ListMeta" }

    FinancialRatio:
      type: object
      properties:
        fiscal_year: { type: integer }
        fiscal_period: { type: string }
        period_end: { type: string, format: date }
        form: { type: string }
        accession: { type: string }
        gross_margin: { type: number }
        operating_margin: { type: number }
        net_margin: { type: number }
        return_on_equity: { type: number }
        return_on_assets: { type: number }
        debt_to_equity: { type: number }
        current_ratio: { type: number }
        quick_ratio: { type: number }
        asset_turnover: { type: number }
        free_cash_flow: { type: number }
        fcf_margin: { type: number }

    Ratios:
      type: object
      properties:
        ticker: { type: string }
        cik: { type: string }
        latest: { $ref: "#/components/schemas/FinancialRatio" }
        history:
          type: array
          items: { $ref: "#/components/schemas/FinancialRatio" }

    EventFiling:
      type: object
      properties:
        accession: { type: string }
        filed_at: { type: string, format: date }
        accepted_at: { type: string, format: date-time }
        items: { type: array, items: { type: string } }
        primary_document_url: { type: string }
        description: { type: string }

    EventFilingsResponse:
      type: object
      properties:
        data:
          type: array
          items: { $ref: "#/components/schemas/EventFiling" }
        meta: { $ref: "#/components/schemas/ListMeta" }

    OwnershipFiling:
      type: object
      properties:
        form: { type: string }
        accession: { type: string }
        filed_at: { type: string, format: date }
        primary_document_url: { type: string }
        description: { type: string }

    OwnershipResponse:
      type: object
      properties:
        data:
          type: array
          items: { $ref: "#/components/schemas/OwnershipFiling" }
        meta: { $ref: "#/components/schemas/ListMeta" }

    CompensationFiling:
      type: object
      properties:
        form: { type: string }
        accession: { type: string }
        filed_at: { type: string, format: date }
        period_end: { type: string, format: date }
        primary_document_url: { type: string }

    CompensationResponse:
      type: object
      properties:
        data:
          type: array
          items: { $ref: "#/components/schemas/CompensationFiling" }
        meta: { $ref: "#/components/schemas/ListMeta" }

    InsiderTransaction:
      type: object
      properties:
        accession: { type: string }
        filed_at: { type: string, format: date }
        reporter: { type: string }
        reporter_cik: { type: string }
        roles: { type: array, items: { type: string } }
        officer_title: { type: string }
        security: { type: string }
        transaction_date: { type: string, format: date }
        transaction_code: { type: string }
        transaction_description: { type: string }
        direction: { type: string, description: "buy or sell" }
        shares: { type: number }
        price_per_share: { type: number }
        notional_usd: { type: number }
        shares_owned_after: { type: number }
        ownership: { type: string, description: "direct or indirect" }
        derivative: { type: boolean }

    InsidersResponse:
      type: object
      properties:
        data:
          type: array
          items: { $ref: "#/components/schemas/InsiderTransaction" }
        meta: { $ref: "#/components/schemas/ListMeta" }

    HoldingPosition:
      type: object
      properties:
        issuer: { type: string }
        security_type: { type: string }
        cusip: { type: string }
        value_usd: { type: number }
        shares: { type: number }
        shares_type: { type: string }
        discretion: { type: string }

    Holdings:
      type: object
      properties:
        filer_cik: { type: string }
        filer_name: { type: string }
        accession: { type: string }
        filed_at: { type: string, format: date }
        period_end: { type: string, format: date }
        total_value_usd: { type: number }
        positions:
          type: array
          items: { $ref: "#/components/schemas/HoldingPosition" }
        meta: { $ref: "#/components/schemas/ListMeta" }

    EconomicEvent:
      type: object
      properties:
        event_id: { type: string }
        name: { type: string }
        country: { type: string }
        currency: { type: string }
        agency: { type: string }
        impact: { type: string }
        scheduled_at: { type: string, format: date-time }
        actual: { type: [number, "null"] }
        previous: { type: [number, "null"] }
        consensus: { type: [number, "null"] }
        released_at: { type: [string, "null"], format: date-time }

    EconomicCalendarResponse:
      type: object
      properties:
        events:
          type: array
          items: { $ref: "#/components/schemas/EconomicEvent" }
        count: { type: integer }
        filter:
          type: object
          additionalProperties: true

    MarketStats:
      type: object
      properties:
        market_cap_usd: { type: integer, format: int64 }
        listed_companies: { type: integer }
        currency: { type: string }
        as_of: { type: string, format: date }

    Market:
      type: object
      properties:
        market: { type: string }
        name: { type: string }
        type: { type: string, description: "exchange | forex | crypto" }
        timezone: { type: string }
        region: { type: string }
        exchanges: { type: array, items: { type: string } }
        stats: { $ref: "#/components/schemas/MarketStats" }

    MarketStatus:
      type: object
      properties:
        market: { type: string }
        type: { type: string }
        is_open: { type: boolean }
        state: { type: string, description: "closed | regular | open" }
        session: { type: string }
        next_open: { type: string, format: date-time }
        next_close: { type: string, format: date-time }
        timezone: { type: string }
        stats: { $ref: "#/components/schemas/MarketStats" }

    HoursBreak:
      type: object
      properties:
        open: { type: string, description: "HH:MM" }
        close: { type: string, description: "HH:MM" }

    HoursSpec:
      type: object
      properties:
        open: { type: string }
        close: { type: string }
        breaks:
          type: array
          items: { $ref: "#/components/schemas/HoursBreak" }

    ForexSession:
      type: object
      properties:
        name: { type: string }
        start: { type: string, description: "HH:MM UTC" }
        end: { type: string }

    MarketHoursBlock:
      type: object
      properties:
        regular: { $ref: "#/components/schemas/HoursSpec" }
        pre_market: { $ref: "#/components/schemas/HoursSpec" }
        post_market: { $ref: "#/components/schemas/HoursSpec" }

    MarketHours:
      type: object
      properties:
        market: { type: string }
        type: { type: string }
        timezone: { type: string }
        hours: { $ref: "#/components/schemas/MarketHoursBlock" }
        opens_at: { type: string }
        closes_at: { type: string }
        sessions:
          type: array
          items: { $ref: "#/components/schemas/ForexSession" }
        exchanges: { type: array, items: { type: string } }

    CalendarDay:
      type: object
      properties:
        date: { type: string, format: date }
        name: { type: string }
        kind:
          { type: string, description: "full_day_holiday | half_day_holiday" }
        state: { type: string }
        early_close: { type: string, format: date-time }

    MarketsListResponse:
      type: object
      properties:
        data:
          type: array
          items: { $ref: "#/components/schemas/Market" }
        meta:
          type: object
          additionalProperties: true

    MarketsStatusAllResponse:
      type: object
      properties:
        data:
          type: array
          items: { $ref: "#/components/schemas/MarketStatus" }
        meta:
          type: object
          additionalProperties: true

    MarketStatusResponse:
      type: object
      properties:
        data: { $ref: "#/components/schemas/MarketStatus" }
        meta:
          type: object
          additionalProperties: true

    MarketHoursResponse:
      type: object
      properties:
        data: { $ref: "#/components/schemas/MarketHours" }
        meta:
          type: object
          additionalProperties: true

    MarketCalendarResponse:
      type: object
      properties:
        data:
          type: array
          items: { $ref: "#/components/schemas/CalendarDay" }
        meta:
          type: object
          additionalProperties: true

    Bar:
      type: object
      properties:
        t:
          {
            type: integer,
            format: int64,
            description: "Bar open time, Unix epoch ms.",
          }
        o: { type: number }
        h: { type: number }
        l: { type: number }
        c: { type: number }
        v: { type: number }

    BarsMeta:
      type: object
      properties:
        as_of: { type: string, format: date-time }
        next_cursor: { type: string }
        symbol: { type: string }
        interval: { type: string }

    BarsResponse:
      type: object
      properties:
        data:
          type: array
          items: { $ref: "#/components/schemas/Bar" }
        meta: { $ref: "#/components/schemas/BarsMeta" }

    WalletToken:
      type: object
      properties:
        contract_address: { type: string }
        symbol: { type: string }
        name: { type: string }
        decimals: { type: integer }
        logo: { type: string }
        raw_balance:
          { type: string, description: Integer balance in base units. }
        balance: { type: string, description: Human-readable balance. }
        native: { type: boolean }

    WalletPortfolio:
      type: object
      properties:
        chain: { type: string }
        address: { type: string }
        tokens:
          type: array
          items: { $ref: "#/components/schemas/WalletToken" }
        count: { type: integer }
        updated_at:
          { type: integer, format: int64, description: Unix epoch seconds. }

    SignalVerdict:
      type: string
      description: Decision-ready label. `score` runs -1 (strong_sell) to +1 (strong_buy).
      enum: [strong_sell, sell, neutral, buy, strong_buy]

    SignalVote:
      type: string
      description: A single indicator's vote.
      enum: [buy, neutral, sell]

    SignalEvent:
      type: string
      description: A discrete marker that fired on a bar.
      enum: [golden_cross, death_cross, macd_cross_up, macd_cross_down]

    SignalCounts:
      type: object
      description: Vote tally across every indicator returned.
      properties:
        buy: { type: integer }
        neutral: { type: integer }
        sell: { type: integer }

    SignalSummary:
      type: object
      properties:
        signal: { $ref: "#/components/schemas/SignalVerdict" }
        score: { type: number, description: "Overall score, -1 to +1." }
        counts: { $ref: "#/components/schemas/SignalCounts" }

    SignalIndicator:
      type: object
      properties:
        name:
          {
            type: string,
            description: "Indicator name and parameters, e.g. RSI(14).",
          }
        value:
          {
            type: number,
            description: "Primary reading. Omitted on rows reported via component fields only.",
          }
        signal_line:
          { type: number, description: "MACD signal line (MACD rows only)." }
        k:
          { type: number, description: "Stochastic %K (Stochastic rows only)." }
        d:
          { type: number, description: "Stochastic %D (Stochastic rows only)." }
        vote: { $ref: "#/components/schemas/SignalVote" }

    SignalGroup:
      type: object
      description: One indicator group (oscillators or moving averages) with its own verdict.
      properties:
        signal: { $ref: "#/components/schemas/SignalVerdict" }
        score: { type: number, description: "Group score, -1 to +1." }
        indicators:
          type: array
          items: { $ref: "#/components/schemas/SignalIndicator" }

    SignalPrice:
      type: object
      properties:
        close: { type: number }
        bar_status:
          type: string
          description: "`forming` (live tick folded into the current bar) or `closed` (last settled bar)."
          enum: [forming, closed]

    SignalMeta:
      type: object
      properties:
        as_of: { type: string, format: date-time }
        symbol: { type: string }
        interval: { type: string }

    LiveSignal:
      type: object
      properties:
        summary: { $ref: "#/components/schemas/SignalSummary" }
        oscillators: { $ref: "#/components/schemas/SignalGroup" }
        moving_averages: { $ref: "#/components/schemas/SignalGroup" }
        price: { $ref: "#/components/schemas/SignalPrice" }

    LiveSignalResponse:
      type: object
      properties:
        data: { $ref: "#/components/schemas/LiveSignal" }
        meta: { $ref: "#/components/schemas/SignalMeta" }

    SignalPoint:
      type: object
      properties:
        t:
          {
            type: integer,
            format: int64,
            description: "Bar open time, Unix epoch ms.",
          }
        close: { type: number }
        summary: { $ref: "#/components/schemas/SignalVerdict" }
        score: { type: number, description: "Score for the bar, -1 to +1." }
        events:
          type: array
          items: { $ref: "#/components/schemas/SignalEvent" }
        counts: { $ref: "#/components/schemas/SignalCounts" }
        oscillators: { $ref: "#/components/schemas/SignalGroup" }
        moving_averages: { $ref: "#/components/schemas/SignalGroup" }

    SignalHistoryResponse:
      type: object
      properties:
        data:
          type: array
          items: { $ref: "#/components/schemas/SignalPoint" }
        meta: { $ref: "#/components/schemas/SignalMeta" }


## 4. asyncapi.yaml (200, 5675 chars)
asyncapi: 3.0.0
info:
  title: SiftingIO Live Stream
  version: "1.0.0"
  description: |
    WebSocket API for live market data. Connect to
    `wss://stream.sifting.io/ws/v1?key=sft_...` — the API key is passed as the
    `key` query parameter (the only WebSocket auth method).

    After connecting, send `subscribe` operations for the products and symbols
    you want; the server streams `tick` and `tvl` frames and replies to control
    operations with `ack` / `pong` / `error` frames. All frames are JSON text.
  license:
    name: MIT
  contact:
    name: SiftingIO
    url: https://sifting.io/docs

servers:
  production:
    host: stream.sifting.io
    pathname: /ws/v1
    protocol: wss
    description: Production live stream. Append `?key=sft_...` to authenticate.

channels:
  stream:
    address: /ws/v1
    title: Live stream
    description: Single bidirectional channel carrying control ops and data frames.
    messages:
      auth:
        $ref: "#/components/messages/Auth"
      subscribe:
        $ref: "#/components/messages/Subscribe"
      unsubscribe:
        $ref: "#/components/messages/Unsubscribe"
      ping:
        $ref: "#/components/messages/Ping"
      ack:
        $ref: "#/components/messages/Ack"
      pong:
        $ref: "#/components/messages/Pong"
      tick:
        $ref: "#/components/messages/Tick"
      tvl:
        $ref: "#/components/messages/TVL"
      error:
        $ref: "#/components/messages/ErrorFrame"

operations:
  subscribe:
    action: send
    channel:
      $ref: "#/channels/stream"
    title: Subscribe to symbols
    messages:
      - $ref: "#/channels/stream/messages/subscribe"
  unsubscribe:
    action: send
    channel:
      $ref: "#/channels/stream"
    title: Unsubscribe from symbols
    messages:
      - $ref: "#/channels/stream/messages/unsubscribe"
  authenticate:
    action: send
    channel:
      $ref: "#/channels/stream"
    title: Authenticate (alternative to the key query param)
    messages:
      - $ref: "#/channels/stream/messages/auth"
  ping:
    action: send
    channel:
      $ref: "#/channels/stream"
    title: Application-level ping
    messages:
      - $ref: "#/channels/stream/messages/ping"
  receiveFrames:
    action: receive
    channel:
      $ref: "#/channels/stream"
    title: Server frames
    messages:
      - $ref: "#/channels/stream/messages/ack"
      - $ref: "#/channels/stream/messages/pong"
      - $ref: "#/channels/stream/messages/tick"
      - $ref: "#/channels/stream/messages/tvl"
      - $ref: "#/channels/stream/messages/error"

components:
  messages:
    Auth:
      name: auth
      title: Auth
      payload:
        $ref: "#/components/schemas/AuthOp"
    Subscribe:
      name: subscribe
      title: Subscribe
      payload:
        $ref: "#/components/schemas/SubscribeOp"
    Unsubscribe:
      name: unsubscribe
      title: Unsubscribe
      payload:
        $ref: "#/components/schemas/SubscribeOp"
    Ping:
      name: ping
      title: Ping
      payload:
        $ref: "#/components/schemas/PingOp"
    Ack:
      name: ack
      title: Ack
      payload:
        $ref: "#/components/schemas/AckFrame"
    Pong:
      name: pong
      title: Pong
      payload:
        $ref: "#/components/schemas/PongFrame"
    Tick:
      name: tick
      title: Tick
      summary: Price update.
      payload:
        $ref: "#/components/schemas/TickFrame"
    TVL:
      name: tvl
      title: TVL update
      payload:
        $ref: "#/components/schemas/TVLFrame"
    ErrorFrame:
      name: error
      title: Error
      payload:
        $ref: "#/components/schemas/ErrorFrame"

  schemas:
    Product:
      type: string
      enum: [cex, dex, fx, us, tvl]
      description: Subscribable product channel.

    AuthOp:
      type: object
      required: [op, key]
      properties:
        op: { const: auth }
        key: { type: string }

    SubscribeOp:
      type: object
      required: [op, product, symbols]
      properties:
        op: { type: string, enum: [subscribe, unsubscribe] }
        product: { $ref: "#/components/schemas/Product" }
        symbols:
          type: array
          items: { type: string }

    PingOp:
      type: object
      required: [op]
      properties:
        op: { const: ping }

    AckFrame:
      type: object
      required: [f]
      properties:
        f: { const: ack }
      additionalProperties: true

    PongFrame:
      type: object
      required: [f]
      properties:
        f: { const: pong }

    TickFrame:
      type: object
      required: [f, s, t]
      properties:
        f: { const: tick }
        s: { type: string, description: Symbol. }
        p: { type: string, description: Trade price. }
        P: { type: string, description: Trade size. }
        b: { type: string, description: Bid price. }
        B: { type: string, description: Bid size. }
        a: { type: string, description: Ask price. }
        A: { type: string, description: Ask size. }
        t:
          {
            type: integer,
            format: int64,
            description: "Unix epoch milliseconds.",
          }
        class: { type: string, description: Asset class tag. }

    TVLFrame:
      type: object
      required: [f, s, t]
      properties:
        f: { const: tvl }
        s: { type: string }
        usd: { type: string }
        r0: { type: string }
        r1: { type: string }
        n: { type: integer }
        t: { type: integer, format: int64 }

    ErrorFrame:
      type: object
      required: [f, code]
      properties:
        f: { const: error }
        code: { type: string }
        message: { type: string }
        limit: { type: integer }


