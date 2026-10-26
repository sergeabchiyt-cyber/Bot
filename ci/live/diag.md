# SiftingIO Commodities & Pagination Docs

## https://sifting.io/docs/historical/commodities.md -> 404 (95381 chars, ct=)
<!DOCTYPE html><html id="__next_error__"><head><meta charSet="utf-8"/><meta name="viewport" content="width=device-width, initial-scale=1"/><link rel="preload" as="script" fetchPriority="low" href="/_next/static/chunks/06k-vo3metpau.js"/><script src="/_next/static/chunks/3jr1hjl4vhypk.js" async=""></script><script src="/_next/static/chunks/3waltwpo9xdn6.js" async=""></script><script src="/_next/sta

## https://sifting.io/docs/historical/commodities -> 200 (259417 chars, ct=text/html; charset=utf-8)
Commodities bars API · SiftingIO
sifting
/
io
Product
Market data APIs
Market data.
One API.
Live and historical data across markets, through REST and WebSocket.
Explore all data APIs
Forex data API
Live currency quotes & price history
US stock data API
Stock quotes, trades & historical bars
Crypto data API
Aggregated prices & historical bars
DEX & DeFi data API
On-chain prices, swaps & liquidity
Commodity data API
Metals, energy & agricultural prices
Fundamentals API
SEC filings, financials & ownership
Need a market or dataset we don't cover?
Request a dataset
API docs
Pricing
Resources
Developers
Market data SDKs
Go, Python & JavaScript clients
Integrations
Excel, workflows & AI agents
Market data tools
Download market data
Historical prices in CSV & Excel
Free market data tools
Downloads, market hours & widgets
Market hours & holidays
Trading schedules & live market status
Market data symbols
Find stock tickers & trading pairs
Guides & research
Market data guides
Understand prices, bars & API concepts
Market data use cases
Trading, research & fintech workflows
Education program
Market data for students & researchers
SiftingIO blog
API tutorials & product updates
Tools, integration guides and practical examples.
Browse all resources
Contact
Log in
Start for free
Dashboard
sifting
/
io
Product
Forex data API
US stock data API
Crypto data API
DEX & DeFi data API
Commodity data API
Fundamentals API
Explore
API docs
Pricing
Resources
Contact
Historical prices in CSV & Excel
Download market data
Request a dataset
Tell us which market, instrument or dataset you need.
Log in
Start for Free
Dashboard
Documentation
+
Search documentation
/
Start here
Overview
Quickstart
API reference
Historical OHLCV
US equities bars
Forex bars
Crypto bars
DEX bars
Commodities bars
Live market data
Currency converter
Discovery
SEC filings
Filing text & analysis
Financials (XBRL)
Insiders & holdings
Economic calendar
Market hours & calendars
Technical signals
DEX wallets
Streaming
WebSocket protocol
FIX API
Operations
Errors & rate limits
Home
Docs
Historical OHLCV
Commodities bars
GET
/v1/hist/commodities/:symbol/bars
gzip required
Commodities bars
Paginated OHLCV bars for a commodity, from 1-minute through monthly. Each bar carries t (Unix epoch milliseconds, UTC, marking the bucket's open time), o/h/l/c as float prices, and v as the tick-count for the bucket. Commodities quote against USD on a rolling near-24/7 calendar, so all time inputs are read as UTC.
Auth
X-API-Key
header
Format
JSON
, gzip-encoded
Rate limits
Per-key, see limits
Example
request · shell
cURL
Python
JavaScript
copy
curl
-H
"X-API-Key:
$KEY
"
-H
"Accept-Encoding: gzip"
--compressed
\
"https://api.sifting.io/v1/hist/commodities/XAUUSD/bars?start=2026-05-01&end=2026-05-02&interval=1h&limit=24"
200
OK
application/json (gzip)
copy
1
{
2
"data"
:
[
3
{
4
"t"
:
1781568000000
,
5
"o"
:
4339.35
,
6
"h"
:
4339.94
,
7
"l"
:
4338.50
,
8
"c"
:
4338.58
,
9
"v"
:
21
10
}
11
]
,
12
"meta"
:
{
13
"symbol"
:
"XAUUSD"
,
14
"interval"
:
"1m"
,
15
"as_of"
:
"2026-06-18T11:26:51Z"
,
16
"next_cursor"
:
"MTc4MTU2ODI0MQ"
17
}
18
}
Loading runner…
First load only
Parameters
Parameter
Description
Parameter
symbol
required
string
·
path
Commodity code, 4–12 uppercase alphanumerics. e.g. XAUUSD (gold), XAGUSD (silver), XPTUSD (platinum), XPDUSD (palladium). Case-insensitive.
start
required
string
·
query
Inclusive lower bound on bar time. Required on the first page; cursor overrides on subsequent pages. Accepts YYYY-MM-DD (midnight UTC) or full RFC3339 (2026-05-01T00:00:00Z).
end
string
·
query
Inclusive upper bound on bar time. Same shape as start. Defaults to now. Must be strictly after start.
interval
string
·
query
Bar size. One of: 1m, 5m, 15m, 30m, 1h, 1d, 1w, 1mo. Default 1m, honored exactly. Same enum across every /hist/* bars endpoint, so a single client schema works across asset classes.
order
string
·
query
Sort direction by bar time. One of: asc, desc. Default asc (oldest first), the natural order for charting and forward pagination. Pass desc to return the most recent bars first.
limit
integer
·
query
Bars per page. Default 1000, max 2000.
cursor
string
·
query
Pagination token. Opaque. Pass back the value of meta.next_cursor from the previous response verbatim. When present, overrides start. Subsequent pages need no other parameters; the cursor encodes the next start.
Response fields
Field
Description
Field
data[]
array
OHLCV bars, oldest first, one per interval bucket.
data[].t
int64 (epoch ms)
Bar open time, Unix epoch milliseconds, UTC. The bar covers [t, t + interval).
data[].o
float
Open price, USD.
data[].h
float
High price, USD.
data[].l
float
Low price, USD.
data[].c
float
Close price, USD.
data[].v
integer
Tick-count volume: the number of price updates in the bucket.
meta.symbol
string
Echoed symbol.
meta.interval
string
Echoed bar size.
meta.as_of
string
When the response was produced. RFC 3339, UTC.
meta.next_cursor
string
Opaque pagination token. Present only when the page came back full and more data exists before end; absent at the tail.
Reference
Pagination pattern
Description
Pagination pattern
First call
?start=2026-05-01&interval=1m&limit=2000
Next call
If meta.next_cursor is present, call again with ?cursor=<token>. interval and limit are sticky by convention.
Tail
Repeat until meta.next_cursor is absent. A short page is the end-of-data signal.
Error responses
Status
Code
Meaning
404
not_found
Symbol outside our coverage.
1
{
2
"error"
:
"not_found"
,
3
"message"
:
"Symbol not found."
4
}
400
invalid_parameter
Malformed symbol, interval, start, end, limit, or cursor.
1
{
2
"error"
:
"invalid_parameter"
,
3
"message"
:
"start must be YYYY-MM-DD or RFC3339."
4
}
403
forbidden
Key is valid but the plan lacks commodities access.
1
{
2
"error"
:
"forbidden"
,
3
"message"
:
"Plan does not include the commodities market."
4
}
406
gzip_required
Heavy endpoint called without Accept-Encoding: gzip.
{
"error"
:
"gzip_required"
}
503
historical_unavailable
Commodities historical data is not configured on this deployment.
{
"error"
:
"historical_unavailable"
}
503
upstream_rate_limited
Our aggregation pipeline hit its rate limit. Try again shortly.
{
"error"
:
"upstream_rate_limited"
}
More in
Historical OHLCV
See all
US equities bars
GET
/v1/hist/stocks/:ticker/bars
Forex bars
GET
/v1/hist/forex/:pair/bars
Crypto bars
GET
/v1/hist/crypto/:symbol/bars
DEX bars
GET
/v1/hist/dex/:symbol/bars
Previous
DEX bars
Next
Last trade
On this page
Example
Try it
Parameters
Response fields
Reference
Error responses
Newsletter
Engineering & methodology updates
Deep-dives on the aggregation pipeline, methodology notes, and product releases. One email per release, no marketing fluff.
Email address
Subscribe
Website (leave empty)
We’ll send a confirmation link before adding you. Unsubscribe in one click.
sifting
/
io
Real-time and historical market data APIs for stocks, FX, crypto, commodities, DEX, and on-chain markets.
99.9% uptime
·
<100ms latency
·
24/7 support
Products
All products
Crypto data API
Forex data API
Stocks data API
DEX & DeFi data API
Developers
Documentation
Market Data Export
SDKs & libraries
Quickstart
WebSocket guide
FIX API
Official GitHub
Status page
Changelog
Integrations
All integrations
Excel
MCP
n8n
Supabase
Resources
All resources
Google Sheets template
Technical evaluation FAQ (PDF)
Guides
Data methodology
Free tools
Market hours
Symbol catalog
Use cases
Education program
Blog
Company
About
Pricing
Enterprise
Partnerships
Brand Assets
Request a dataset
Slack community
Book a call
Contact
Account
Login
Create account
Dashboard
Billing
Important.
SiftingIO (the “Service”) is operated by
SaltingIO LLC
(“SaltingIO”), a Wyoming limited liability company. The Service provides
aggregated
,
derived
, and
calculated
market data computed from multiple third-party sources. SaltingIO is
not an exchange
, broker, dealer, market maker, custodian, liquidity provider, or financial institution, and does not execute, route, match, or settle trades. The Service does
not
redistribute raw primary exchange feeds, official exchange-of-record prices, last-sale data, order books, or any venue’s proprietary data; outputs are synthetic reference values produced by SaltingIO’s aggregation, normalization, and fair-price methodologies and may differ materially from any specific venue, broker, or counterparty. All information is provided
“as is” and “as available”
without warranties of any kind, including accuracy, completeness, timeliness, fitness for a particular purpose, or non-infringement, and may contain errors, gaps, delays, or interruptions. Nothing on this site or in the Service constitutes investment, financial, trading, legal, tax, or accounting advice, an offer or solicitation to buy or sell any asset, or a recommendation of any strategy. Trading and investing involve substantial risk including total loss of capital; any decision or action taken in reliance on the data, including automated or algorithmic trading, is made
solely at the user’s own risk
, and SaltingIO, its parents, subsidiaries, affiliates, officers, directors, employees, agents, and licensors accept no liability for any direct, indirect, incidental, consequential, special, exemplary, or other losses arising from use of, or inability to use, the Service to the maximum extent permitted by law. Use of the Service is governed by the
Terms of Use
; customers are responsible for license compliance, accurate usage reporting, and ensuring the Service is appropriate for their jurisdiction and use case.
Read the full disclosures →
©
2026
SiftingIO · A SaltingIO LLC product · All rights reserved
Terms
Privacy
Cookies
Disclaimer
Cookie preferences
All systems operational

## https://sifting.io/blog/commodities-market-data-api-backfill-history-stream-live-prices.md -> 404 (91832 chars, ct=)
<!DOCTYPE html><html id="__next_error__"><head><meta charSet="utf-8"/><meta name="viewport" content="width=device-width, initial-scale=1"/><link rel="preload" as="script" fetchPriority="low" href="/_next/static/chunks/06k-vo3metpau.js"/><script src="/_next/static/chunks/3jr1hjl4vhypk.js" async=""></script><script src="/_next/static/chunks/3waltwpo9xdn6.js" async=""></script><script src="/_next/sta

## https://sifting.io/blog/commodities-market-data-api-backfill-history-stream-live-prices -> 200 (216296 chars, ct=text/html; charset=utf-8)
Commodities market data API: backfill historical bars, then stream live prices · SiftingIO
sifting
/
io
Product
Market data APIs
Market data.
One API.
Live and historical data across markets, through REST and WebSocket.
Explore all data APIs
Forex data API
Live currency quotes & price history
US stock data API
Stock quotes, trades & historical bars
Crypto data API
Aggregated prices & historical bars
DEX & DeFi data API
On-chain prices, swaps & liquidity
Commodity data API
Metals, energy & agricultural prices
Fundamentals API
SEC filings, financials & ownership
Need a market or dataset we don't cover?
Request a dataset
API docs
Pricing
Resources
Developers
Market data SDKs
Go, Python & JavaScript clients
Integrations
Excel, workflows & AI agents
Market data tools
Download market data
Historical prices in CSV & Excel
Free market data tools
Downloads, market hours & widgets
Market hours & holidays
Trading schedules & live market status
Market data symbols
Find stock tickers & trading pairs
Guides & research
Market data guides
Understand prices, bars & API concepts
Market data use cases
Trading, research & fintech workflows
Education program
Market data for students & researchers
SiftingIO blog
API tutorials & product updates
Tools, integration guides and practical examples.
Browse all resources
Contact
Log in
Start for free
Dashboard
sifting
/
io
Product
Forex data API
US stock data API
Crypto data API
DEX & DeFi data API
Commodity data API
Fundamentals API
Explore
API docs
Pricing
Resources
Contact
Historical prices in CSV & Excel
Download market data
Request a dataset
Tell us which market, instrument or dataset you need.
Log in
Start for Free
Dashboard
Home
Blog
Commodities
Commodities
July 29, 2026
·
6 min read
·
SiftingIO Team
Commodities market data API: backfill historical bars, then stream live prices
Pull historical OHLCV bars for gold, crude, and other commodities over REST, stream live prices over WebSocket, and handle the seam where they meet.
A commodities market data API has to solve two problems that look similar but behave differently: backfilling enough history to power charts and backtests, and keeping prices current once that history is loaded. This guide walks through both with SiftingIO's commodities coverage. You'll pull historical OHLCV bars over REST, subscribe to live prices over WebSocket, and handle the seam where they meet, plus the session and calendar quirks that separate commodities from a 24/7 crypto feed.
Commodities coverage: symbols, fields, and history depth
#
SiftingIO publishes spot pricing and OHLCV bars for precious metals, industrial metals, energy benchmarks, and agricultural products. Symbols are the commodity code plus USD, uppercase, no separator: XAUUSD for gold, XAGUSD for silver, XPTUSD for platinum, WTIUSD and BRENTUSD for the crude benchmarks, NATGASUSD for natural gas, COPPERUSD for copper, and CORNUSD for corn. The symbols page on the site lists the full catalog.
Each published price is a consensus value: a volume-and-reputation-weighted median aggregated across multiple independent venues rather than a pass-through of any single feed. That matters here because commodity spot markets are fragmented and a single source can go quiet without warning. Every tick carries a quality flag, so a degraded feed announces itself. Treat it as a reference price for research, dashboards, and validation rather than an execution feed.
Every plan includes the full available commodity archive, including Free and Builder. The earliest date varies by series and source coverage. Current request allowances and rate limits are on the pricing page.
Backfilling historical bars over REST
#
Historical bars live under /v1/hist. For commodities the request looks like this:
curl --compressed -H "X-API-Key: $SIFTING_KEY" \
"https://api.sifting.io/v1/hist/commodities/XAUUSD/bars?interval=1d&limit=2000"
Two details in that command are load-bearing. Authentication is the X-API-Key header (there is no Bearer auth), and, compressed makes curl send Accept-Encoding: gzip. Historical bar endpoints require gzip; without it the API returns a 406 with {"error":"gzip_required"} and no data.
Pagination is cursor-based. limit defaults to 1000 and caps at 2000 bars per page, and each response's meta.next_cursor points to the next page, going null on the last one. A full backfill is a loop:
import os
import requests
def backfill(symbol, interval="1d"):
url = f"https://api.sifting.io/v1/hist/commodities/{symbol}/bars"
headers = {"X-API-Key": os.environ["SIFTING_KEY"]}
params = {"interval": interval, "limit": 2000}
bars = []
while True:
resp = requests.get(url, headers=headers, params=params, timeout=30)
resp.raise_for_status()
payload = resp.json()
bars.extend(payload["data"])
cursor = payload["meta"]["next_cursor"]
if cursor is None:
return bars
params["cursor"] = cursor
xau_daily = backfill("XAUUSD")
The requests library negotiates gzip on its own, so no extra header is needed there. Each bar is an OHLCV object with a UTC timestamp; sort by time before computing anything, since page order isn't guaranteed to be chronological.
Real-time commodities prices: snapshot or stream
#
For a dashboard that refreshes every few seconds, the REST snapshots may be all you need:
curl -H "X-API-Key: $SIFTING_KEY" \
"https://api.sifting.io/v1/last/quote/commodities/WTIUSD"
/v1/last/quote returns the best bid and ask, and /v1/last/trade/commodities/{symbol} the latest trade with price, size, and timestamp. Polling fits inside REST rate limits at low symbol counts.
For continuous updates, connect to the WebSocket stream at wss://stream.sifting.io/ws/v1?key=$SIFTING_KEY. The key travels in the query string, or in an {"op":"auth","key":"sft_..."} frame after connecting; the server acks with your tier. Subscribing is one frame. The product code for commodities is com, the same short code that tags commodity ticks; the stream's product codes are not the REST slugs, so it is com here, not commodities:
{ "op": "subscribe", "product": "com", "symbols": ["XAUUSD", "WTIUSD"] }
Updates arrive as tick frames:
{ "f": "tick", "class": "com", "s": "XAUUSD", "p": 3341.2, "b": 3341.0, "a": 3341.4, "t": 1778019852426 }
class is the venue type (com for commodities), p is the aggregated price, b and a are bid and ask, and t is Unix epoch milliseconds. Two protocol details matter. On subscribe, the server first replays the last cached value, so a price shows up before the market next ticks. And it closes any connection that sends no client frames for 90 seconds, so send {"op":"ping"} at least every 60. Update cadence is tier-dependent, from 1 Hz on the free tier to 10 Hz on Ultra; that's a reference-price update rate rather than execution latency.
Stitching history to the live stream
#
The seam between the last historical bar and the first live tick is where integrations quietly go wrong. The order that works: backfill over REST first, then open the stream. The replayed cached value gives you a price immediately, the REST archive stays the source of truth for closed periods, and the only ambiguous region left is the current, still-forming bar.
Treat that bar as mutable. The most recent bar REST returns during an open session covers a period that hasn't closed, so don't persist it as final. Bucket each incoming tick by flooring its timestamp to the interval (for hourly bars, bucket = t // 3_600_000). While a tick's bucket matches the latest bar's bucket, update the bar in place: raise the high, lower the low, set close to p. When a tick lands in a later bucket, seal the old bar and open a new one seeded from that tick.
Deduplication falls out of the same arithmetic: a tick whose bucket is older than your latest bar is history you already hold from REST, so drop it. That rule also absorbs the replayed cached value, whether it predates or postdates the backfill. Reconnects are the same seam again. Subscriptions don't survive a dropped connection, so resubscribe, let the bucket comparison sort the replayed value, and re-run the backfill for the gap if the outage was long.
Common pitfalls
#
Sessions are per product, and none of them run 24/7. Metals and energy pairs trade nearly around the clock on weekdays but stop for the weekend, while agricultural products keep their own shorter sessions. Don't hard-code a schedule. GET /v1/fnd/markets returns the market catalog, /v1/fnd/markets/{market}/status reports open or closed right now, and the /hours and /calendar endpoints publish each market's weekly schedule and holidays. A poller that checks status before hammering /v1/last spends its rate budget only on hours that matter. Related: a /v1/last request during a closed session can return 503 stale_snapshot, which means the newest data is older than the freshness threshold; the body includes last_t and server_now so code can decide whether Friday's close is acceptable.
Units differ by an order of magnitude or three. XAUUSD quotes dollars per troy ounce and WTIUSD dollars per barrel, so a chart that plots both on one axis flattens crude into a line along the bottom. Normalize to percentage change or give each series its own axis, and never mix units when summing exposure across commodities.
Weekend and holiday gaps break naive time-series code. A daily series for CORNUSD has no rows for closed days, so reindexing to a continuous calendar in pandas fills gaps with NaN, and a return computed from Friday's close to the next session's close spans several calendar days. Compute returns on trading days, and when a model needs calendar alignment across commodities, align on each market's own calendar from the API instead of assuming they share one.
That's the whole loop: backfill bars over REST with gzip and cursors, stream ticks with a ping every minute, merge at the bucket boundary, and let the market-hours endpoints tell you when prices should and shouldn't be moving. The endpoint reference, streaming product list, and available historical coverage are in the docs.
Read the docs
Tags
commodities
market data api
ohlcv bars
websocket streaming
historical data
real-time prices
xauusd
wtiusd
backtesting
All posts
Last updated
October 9, 2026
Keep reading
Related posts
Commodities
Oct 21, 2026
7 min read
Gold-silver ratio in Python: calculate it from API prices
Calculate the gold-silver ratio with Python and commodity price data. Match timestamps, preserve missing bars and avoid misleading ratio candles.
SiftingIO Team
Commodities
Aug 14, 2026
5 min read
Commodities data API: coverage, fields, and symbol names for gold, silver, oil, and metals
SiftingIO's commodities data API: real symbols (XAUUSD, XAGUSD, WTIUSD, XCUUSD, NATGAS), OHLCV history, live REST quotes, and WebSocket streaming.
SiftingIO Team
Commodities
Aug 1, 2026
6 min read
XAUUSD explained: gold price per gram, karat, and local currency math with a market data API
What XAUUSD means, the exact troy ounce to gram conversion, 24k/22k/18k karat math, and lira pricing via USDTRY, computed from real SiftingIO market data.
SiftingIO Team
Newsletter
Engineering & methodology updates
Deep-dives on the aggregation pipeline, methodology notes, and product releases. One email per release, no marketing fluff.
Email address
Subscribe
Website (leave empty)
We’ll send a confirmation link before adding you. Unsubscribe in one click.
sifting
/
io
Real-time and historical market data APIs for stocks, FX, crypto, commodities, DEX, and on-chain markets.
99.9% uptime
·
<100ms latency
·
24/7 support
Products
All products
Crypto data API
Forex data API
Stocks data API
DEX & DeFi data API
Developers
Documentation
Market Data Export
SDKs & libraries
Quickstart
WebSocket guide
FIX API
Official GitHub
Status page
Changelog
Integrations
All integrations
Excel
MCP
n8n
Supabase
Resources
All resources
Google Sheets template
Technical evaluation FAQ (PDF)
Guides
Data methodology
Free tools
Market hours
Symbol catalog
Use cases
Education program
Blog
Company
About
Pricing
Enterprise
Partnerships
Brand Assets
Request a dataset
Slack community
Book a call
Contact
Account
Login
Create account
Dashboard
Billing
Important.
SiftingIO (the “Service”) is operated by
SaltingIO LLC
(“SaltingIO”), a Wyoming limited liability company. The Service provides
aggregated
,
derived
, and
calculated
market data computed from multiple third-party sources. SaltingIO is
not an exchange
, broker, dealer, market maker, custodian, liquidity provider, or financial institution, and does not execute, route, match, or settle trades. The Service does
not
redistribute raw primary exchange feeds, official exchange-of-record prices, last-sale data, order books, or any venue’s proprietary data; outputs are synthetic reference values produced by SaltingIO’s aggregation, normalization, and fair-price methodologies and may differ materially from any specific venue, broker, or counterparty. All information is provided
“as is” and “as available”
without warranties of any kind, including accuracy, completeness, timeliness, fitness for a particular purpose, or non-infringement, and may contain errors, gaps, delays, or interruptions. Nothing on this site or in the Service constitutes investment, financial, trading, legal, tax, or accounting advice, an offer or solicitation to buy or sell any asset, or a recommendation of any strategy. Trading and investing involve substantial risk including total loss of capital; any decision or action taken in reliance on the data, including automated or algorithmic trading, is made
solely at the user’s own risk
, and SaltingIO, its parents, subsidiaries, affiliates, officers, directors, employees, agents, and licensors accept no liability for any direct, indirect, incidental, consequential, special, exemplary, or other losses arising from use of, or inability to use, the Service to the maximum extent permitted by law. Use of the Service is governed by the
Terms of Use
; customers are responsible for license compliance, accurate usage reporting, and ensuring the Service is appropriate for their jurisdiction and use case.
Read the full disclosures →
©
2026
SiftingIO · A SaltingIO LLC product · All rights reserved
Terms
Privacy
Cookies
Disclaimer
Cookie preferences
All systems operational

## https://sifting.io/blog/cursor-pagination-market-data-api-years-of-1-minute-ohlcv-bars.md -> 404 (91830 chars, ct=)
<!DOCTYPE html><html id="__next_error__"><head><meta charSet="utf-8"/><meta name="viewport" content="width=device-width, initial-scale=1"/><link rel="preload" as="script" fetchPriority="low" href="/_next/static/chunks/06k-vo3metpau.js"/><script src="/_next/static/chunks/3jr1hjl4vhypk.js" async=""></script><script src="/_next/static/chunks/3waltwpo9xdn6.js" async=""></script><script src="/_next/sta

## https://sifting.io/blog/cursor-pagination-market-data-api-years-of-1-minute-ohlcv-bars -> 200 (220098 chars, ct=text/html; charset=utf-8)
Cursor pagination for market data APIs: pull years of 1-minute OHLCV bars without gaps · SiftingIO
sifting
/
io
Product
Market data APIs
Market data.
One API.
Live and historical data across markets, through REST and WebSocket.
Explore all data APIs
Forex data API
Live currency quotes & price history
US stock data API
Stock quotes, trades & historical bars
Crypto data API
Aggregated prices & historical bars
DEX & DeFi data API
On-chain prices, swaps & liquidity
Commodity data API
Metals, energy & agricultural prices
Fundamentals API
SEC filings, financials & ownership
Need a market or dataset we don't cover?
Request a dataset
API docs
Pricing
Resources
Developers
Market data SDKs
Go, Python & JavaScript clients
Integrations
Excel, workflows & AI agents
Market data tools
Download market data
Historical prices in CSV & Excel
Free market data tools
Downloads, market hours & widgets
Market hours & holidays
Trading schedules & live market status
Market data symbols
Find stock tickers & trading pairs
Guides & research
Market data guides
Understand prices, bars & API concepts
Market data use cases
Trading, research & fintech workflows
Education program
Market data for students & researchers
SiftingIO blog
API tutorials & product updates
Tools, integration guides and practical examples.
Browse all resources
Contact
Log in
Start for free
Dashboard
sifting
/
io
Product
Forex data API
US stock data API
Crypto data API
DEX & DeFi data API
Commodity data API
Fundamentals API
Explore
API docs
Pricing
Resources
Contact
Historical prices in CSV & Excel
Download market data
Request a dataset
Tell us which market, instrument or dataset you need.
Log in
Start for Free
Dashboard
Home
Blog
Developer Tutorials
Developer Tutorials
September 7, 2026
·
7 min read
·
SiftingIO Team
Cursor pagination for market data APIs: pull years of 1-minute OHLCV bars without gaps
How cursor pagination works on a market data API, with request math, a Python loop, and a gap check for multi-year pulls of 1-minute OHLCV bars.
Cursor pagination is how you pull a multi-year run of 1-minute OHLCV bars through a REST API that caps each response at a fixed number of rows. The question comes up the first time a backtest needs more than a screenful of history: how do you page through roughly 98,000 minute bars per year for a single US stock, keep the sequence intact, and stay inside a monthly call budget? This post walks through the mechanics on the SiftingIO historical bars endpoint, using a page size of 1,000 bars in the examples below, but the pattern applies to any cursor-based API.
Why market data APIs use cursors instead of page numbers
#
Offset pagination (
?page=37
or
?offset=7200
) assumes the underlying list doesn't move while you're reading it. Market data moves constantly. New bars land at the end of the series every minute during the session, and occasionally a late print corrects a bar that was already served. With offsets, a row inserted ahead of your position shifts everything by one, so page 38 repeats the last row of page 37, or skips one. You won't see an error. You'll see a duplicate timestamp, or a hole, three weeks later when a strategy behaves oddly.
A cursor fixes this by encoding the position itself. Every bars response from the SiftingIO API carries a
meta
object with four fields:
next_cursor
,
as_of
,
symbol
, and
interval
. The cursor is an opaque string. You pass it back as
?cursor=...
on the next request and the server resumes exactly after the last row it sent. When
next_cursor
is absent or null, you've reached the end. The
as_of
timestamp is the snapshot moment the page reflects, which matters when the final page includes a bar that is still forming.
Page size is controlled with
?limit
. The examples below request 1,000 rows per page; that is the example page size, not a universal API maximum. Check the
per-class historical bars reference
for the current default and maximum on the endpoint you use. Stock bars come back ascending, oldest first, and cover regular-session minutes back to 2000. Prices are as-traded, with no split or dividend adjustment applied.
The request math at 1,000 bars per page
#
A US equity trades about 390 regular-session minutes a day across roughly 252 trading days, so one year of 1-minute bars is close to 98,000 rows for one ticker. At 1,000 rows per page that is 99 requests per ticker-year (98,280 divided by 1,000, rounded up). A three-year pull for ten tickers is under 3,000 requests. That is a small number. It fits inside the free tier's 10,000 calls a month three times over.
Free, Builder, and Pro all include the full available historical archive. Stock bars can reach back to 2000 where listing history supports it. At the example page size, a complete 1-minute backfill can take around 2,600 requests per ticker, so request allowance and loading time matter even when history is included. Builder raises the monthly allowance to 250,000 calls. Check
pricing
for current quotas and rate limits before budgeting.
Rate limits matter more than the quota for this workload. The free tier allows 60 requests a minute, so a full year of one ticker takes just under two minutes of wall clock. Builder allows 100 requests a second, which moves the bottleneck to gzip decoding on your side. Every response includes
X-RateLimit-Limit
(burst capacity) and
X-RateLimit-Remaining
(tokens left). On a 429 the body carries
rate_limit_exceeded
and the
Retry-After
header says how many seconds to wait. A pagination loop should read those headers rather than sleeping a fixed interval.
A Python loop that follows the cursor
#
The historical bars endpoints require gzip. Send
Accept-Encoding: gzip
or you'll get a 406 with the error code
gzip_required
. The
requests
library adds that header by default and decompresses transparently, so the code below is explicit about it only for readers using a lower-level client.
import os, time
import requests
import pandas as pd
BASE = "https://api.sifting.io/v1"
HEADERS = {
"X-API-Key": os.environ["SIFTING_KEY"],
"Accept-Encoding": "gzip",
}
def fetch_bars(ticker, interval="1m", start=None, end=None):
params = {"interval": interval, "limit": 1000}
if start: params["start"] = start   # inclusive, YYYY-MM-DD or RFC 3339
if end:   params["end"] = end       # inclusive, defaults to now
rows, cursor = [], None
while True:
if cursor:
params["cursor"] = cursor
r = requests.get(f"{BASE}/hist/stocks/{ticker}/bars",
headers=HEADERS, params=params, timeout=30)
if r.status_code == 429:
time.sleep(int(r.headers.get("Retry-After", "1")))
continue
r.raise_for_status()
body = r.json()
rows.extend(body["data"])
cursor = body["meta"].get("next_cursor")
if int(r.headers.get("X-RateLimit-Remaining", "1")) < 5:
time.sleep(1)
if not cursor:
return pd.DataFrame(rows), body["meta"]["as_of"]
bars, as_of = fetch_bars("AAPL", start="2025-01-01", end="2025-12-31")
print(len(bars), "bars as of", as_of)
Each row has six fields:
t
(bar open time in Unix epoch milliseconds),
o
,
h
,
l
,
c
, and
v
. Two design choices in the loop are worth naming. First, the cursor is the only state. Nothing is computed from row counts or timestamps to decide where the next request begins. Second, the 429 branch retries the same request with the same cursor. That's safe because the cursor is idempotent: the server resumes after the same row regardless of how many times you ask.
The same function works for the other bar endpoints by swapping the path segment and always passing
start
, which is required there.
/hist/forex/EURUSD/bars
returns OHLCV in UTC;
v
is aggregated traded volume for the bar, summed across contributing sources.
/hist/crypto/BTCUSD/bars
returns base-asset volume. All bar routes accept the same interval set, 1m through 1h intraday plus 1d, 1w, and 1mo. If
start
predates coverage on crypto or DEX bars, the API returns 422 with code
data_unavailable
and an
earliest
field carrying the first date it can serve. Clamp
start
to that value and retry.
Checking the result for gaps and duplicates
#
Once the frame is built, verify it. Cursors protect the transport, but a session boundary, a half-day, or a resumed pull from a saved position can still leave the series in a state you didn't expect.
bars["ts"] = pd.to_datetime(bars["t"], unit="ms", utc=True)
bars = bars.drop_duplicates("ts").set_index("ts").sort_index()
# gaps larger than one interval inside a single trading day
for day, frame in bars.groupby(bars.index.date):
diffs = frame.index.to_series().diff().dropna()
gaps = diffs[diffs > pd.Timedelta(minutes=1)]
if len(gaps):
print(day, len(gaps), "intra-day gaps, largest", gaps.max())
Some gaps are real. A 1-minute bar exists only when there was activity in that minute, and a thinly traded ticker will legitimately skip minutes. Gaps at page boundaries can indicate a pagination problem, but are not proof by themselves. Compare the timestamps on both sides of the boundary and re-request the affected window before attributing the gap to the loop.
If you persist results and resume later, store the last
t
you have rather than the cursor. Cursors are opaque and shouldn't be assumed to stay valid indefinitely. Restarting with
start
set to your last timestamp and dropping the duplicate first row is the durable approach, and it works because
start
accepts a full RFC 3339 timestamp, not only a date.
Common pitfalls
#
A 406 on a request that worked yesterday from a different tool. curl does not send
Accept-Encoding
unless you pass
--compressed
, so a snippet copied from a Python session into a shell script fails with
gzip_required
. The fix is
curl --compressed -H "X-API-Key: $SIFTING_KEY" "https://api.sifting.io/v1/hist/stocks/MSFT/bars?interval=1d"
. The same requirement applies to the XBRL financials and screener endpoints.
Assuming bars use the same page size as the rest of the API. Most list endpoints (filings, insider transactions, ownership) default to 50 rows and cap at 200. The bars examples here use 1,000 rows per page. For a large pull, a generic helper with
limit=200
needs roughly five times as many requests as the same pull at
limit=1000
, with rounding at the final page. Set the limit per endpoint using its documented supported range, or leave it off to use its documented default.
Treating
meta
as a row count. The bars
meta
object has no
total
field. Don't pre-size buffers or progress bars from a count that isn't there. Estimate from the calendar (sessions times 390) and treat the estimate as approximate, because minutes with no activity produce no bar.
Requesting reverse order on stocks.
order=desc
is supported on forex and commodities bars only, where it anchors the first page at the newest bar so the latest N rows cost one request. Stock and crypto bars are ascending-only. To get the most recent session, set
start
to that date instead of paging forward from 2000.
The loop above, plus the gap check, is enough to fill a research database with a year of minute bars per ticker in about 99 requests each.
Read the docs
for the full parameter list on every bars route.
Tags
cursor pagination
ohlcv bars
historical market data
1-minute bars
python
pandas
rate limits
backtesting
All posts
Last updated
October 9, 2026
Keep reading
Related posts
Developer Tutorials
Oct 23, 2026
13 min read
Bollinger Bands in Python: OHLCV Data and Live Prices
Calculate Bollinger Bands in Python with pandas, verify the math, plot OHLCV closes and add live WebSocket prices without confusing ticks with candles.
SiftingIO Team
Developer Tutorials
Oct 22, 2026
6 min read
Candlestick Chart in Python with Plotly and OHLCV Data
Build a Python candlestick chart with Plotly and Bitcoin OHLCV data. Add volume, handle UTC timestamps, and save an interactive HTML chart with SiftingIO.
SiftingIO Team
Developer Tutorials
Oct 20, 2026
7 min read
Forex API JSON: build a multi-pair watchlist in JavaScript
Build a forex watchlist from one SiftingIO JSON snapshot. Use Node.js to request multiple pairs, preserve missing rows and label timestamps correctly.
SiftingIO Team
Newsletter
Engineering & methodology updates
Deep-dives on the aggregation pipeline, methodology notes, and product releases. One email per release, no marketing fluff.
Email address
Subscribe
Website (leave empty)
We’ll send a confirmation link before adding you. Unsubscribe in one click.
sifting
/
io
Real-time and historical market data APIs for stocks, FX, crypto, commodities, DEX, and on-chain markets.
99.9% uptime
·
<100ms latency
·
24/7 support
Products
All products
Crypto data API
Forex data API
Stocks data API
DEX & DeFi data API
Developers
Documentation
Market Data Export
SDKs & libraries
Quickstart
WebSocket guide
FIX API
Official GitHub
Status page
Changelog
Integrations
All integrations
Excel
MCP
n8n
Supabase
Resources
All resources
Google Sheets template
Technical evaluation FAQ (PDF)
Guides
Data methodology
Free tools
Market hours
Symbol catalog
Use cases
Education program
Blog
Company
About
Pricing
Enterprise
Partnerships
Brand Assets
Request a dataset
Slack community
Book a call
Contact
Account
Login
Create account
Dashboard
Billing
Important.
SiftingIO (the “Service”) is operated by
SaltingIO LLC
(“SaltingIO”), a Wyoming limited liability company. The Service provides
aggregated
,
derived
, and
calculated
market data computed from multiple third-party sources. SaltingIO is
not an exchange
, broker, dealer, market maker, custodian, liquidity provider, or financial institution, and does not execute, route, match, or settle trades. The Service does
not
redistribute raw primary exchange feeds, official exchange-of-record prices, last-sale data, order books, or any venue’s proprietary data; outputs are synthetic reference values produced by SaltingIO’s aggregation, normalization, and fair-price methodologies and may differ materially from any specific venue, broker, or counterparty. All information is provided
“as is” and “as available”
without warranties of any kind, including accuracy, completeness, timeliness, fitness for a particular purpose, or non-infringement, and may contain errors, gaps, delays, or interruptions. Nothing on this site or in the Service constitutes investment, financial, trading, legal, tax, or accounting advice, an offer or solicitation to buy or sell any asset, or a recommendation of any strategy. Trading and investing involve substantial risk including total loss of capital; any decision or action taken in reliance on the data, including automated or algorithmic trading, is made
solely at the user’s own risk
, and SaltingIO, its parents, subsidiaries, affiliates, officers, directors, employees, agents, and licensors accept no liability for any direct, indirect, incidental, consequential, special, exemplary, or other losses arising from use of, or inability to use, the Service to the maximum extent permitted by law. Use of the Service is governed by the
Terms of Use
; customers are responsible for license compliance, accurate usage reporting, and ensuring the Service is appropriate for their jurisdiction and use case.
Read the full disclosures →
©
2026
SiftingIO · A SaltingIO LLC product · All rights reserved
Terms
Privacy
Cookies
Disclaimer
Cookie preferences
All systems operational

## https://sifting.io/blog/commodities-data-api-coverage-symbols-gold-silver-oil -> 200 (214509 chars, ct=text/html; charset=utf-8)
Commodities data API: coverage, fields, and symbol names for gold, silver, oil, and metals · SiftingIO
sifting
/
io
Product
Market data APIs
Market data.
One API.
Live and historical data across markets, through REST and WebSocket.
Explore all data APIs
Forex data API
Live currency quotes & price history
US stock data API
Stock quotes, trades & historical bars
Crypto data API
Aggregated prices & historical bars
DEX & DeFi data API
On-chain prices, swaps & liquidity
Commodity data API
Metals, energy & agricultural prices
Fundamentals API
SEC filings, financials & ownership
Need a market or dataset we don't cover?
Request a dataset
API docs
Pricing
Resources
Developers
Market data SDKs
Go, Python & JavaScript clients
Integrations
Excel, workflows & AI agents
Market data tools
Download market data
Historical prices in CSV & Excel
Free market data tools
Downloads, market hours & widgets
Market hours & holidays
Trading schedules & live market status
Market data symbols
Find stock tickers & trading pairs
Guides & research
Market data guides
Understand prices, bars & API concepts
Market data use cases
Trading, research & fintech workflows
Education program
Market data for students & researchers
SiftingIO blog
API tutorials & product updates
Tools, integration guides and practical examples.
Browse all resources
Contact
Log in
Start for free
Dashboard
sifting
/
io
Product
Forex data API
US stock data API
Crypto data API
DEX & DeFi data API
Commodity data API
Fundamentals API
Explore
API docs
Pricing
Resources
Contact
Historical prices in CSV & Excel
Download market data
Request a dataset
Tell us which market, instrument or dataset you need.
Log in
Start for Free
Dashboard
Home
Blog
Commodities
Commodities
August 14, 2026
·
5 min read
·
SiftingIO Team
Commodities data API: coverage, fields, and symbol names for gold, silver, oil, and metals
SiftingIO's commodities data API: real symbols (XAUUSD, XAGUSD, WTIUSD, XCUUSD, NATGAS), OHLCV history, live REST quotes, and WebSocket streaming.
A
commodities market data API
has to answer three questions before you write any code: which commodities it covers, what the symbols are called, and how you pull both historical bars and live prices. Vague answers cost real time. Guess a symbol name and you may request data that is not available. Assume gold trades around the clock like BTC and your staleness alerts fire every weekend.
This post covers SiftingIO's commodities data: the metals, energy benchmarks, and agricultural products it serves, the exact symbol spellings the API expects, the REST endpoints for history and snapshots, and the WebSocket stream for live ticks.
What the commodities data API covers
#
SiftingIO serves commodities as one of six asset classes behind a single API key, alongside US equities, forex, crypto, DEX activity, and on-chain metrics. The commodities coverage spans four groups: precious metals (gold, silver, platinum), industrial metals (copper), energy benchmarks (WTI and Brent crude oil, natural gas), and agricultural products (corn). For each instrument you get spot pricing and OHLCV bars, live over REST and WebSocket, historical through the bars endpoints.
Prices aren't passed through from a single source. Each published tick is an aggregate across multiple independent venues: a volume-and-reputation-weighted median rather than a simple average. A median holds until a majority of venues err in the same direction, which matters for commodities because spot markets are fragmented and quotes can go stale without any obvious signal. Live snapshots include a timestamp in the
t
field. Check that timestamp when assessing freshness; the public response does not expose separate source, ingest, and publish timestamps or a quality flag. The output is a synthetic reference price. It works for research, dashboards, alerting, and validating what another feed is showing you. It is not executable depth, and order routing belongs on an execution venue's own feed.
Commodities symbols: how gold, silver, and oil are spelled
#
Developers search for symbols by name, and the API is strict about spelling. Commodity symbols are uppercase alphanumeric with no separators. Most follow commodity-code-plus-USD form; natural gas is the one exception, spelled NATGAS with no USD suffix:
XAUUSD: gold
XAGUSD: silver
XPTUSD: platinum
XCUUSD: copper
WTIUSD: WTI crude oil
UKOUSD: Brent crude oil
NATGAS: natural gas
CORNUSD: corn
Treat the browsable symbol catalog on the site as the authoritative spelling reference rather than guessing by pattern.
These are spot reference symbols. There are no futures-style contract codes with expiry months, which means there's no contract roll to manage in a backtest: XAUUSD is the continuously aggregated spot gold reference in US dollars, yesterday and today.
Historical bars and live prices over REST
#
Everything authenticates with the X-API-Key header. Live snapshots live under /v1/last, historical bars under /v1/hist:
# Best bid and ask for gold
curl -H "X-API-Key: $SIFTING_KEY" \
"https://api.sifting.io/v1/last/quote/commodities/XAUUSD"
# Last trade for WTI crude: price, size, timestamp
curl -H "X-API-Key: $SIFTING_KEY" \
"https://api.sifting.io/v1/last/trade/commodities/WTIUSD"
# Daily silver bars (historical endpoints require gzip)
curl --compressed -H "X-API-Key: $SIFTING_KEY" \
"https://api.sifting.io/v1/hist/commodities/XAGUSD/bars?interval=1d"
The quote endpoint returns the current best bid and ask. The trade endpoint returns the latest price with size and an epoch-millisecond timestamp. Bars come back as standard OHLCV rows with UTC timestamps, at intervals from one minute upward; the full interval list per asset class is in the docs.
Long historical pulls page with a cursor: pass a limit, then follow meta.next_cursor from each response until it comes back null. Every plan includes the full available commodity history, including Free. Coverage varies by series. Check the
pricing page
for request allowances and rate limits before sizing a backfill job.
Every response also carries rate-limit headers. X-RateLimit-Remaining tells you how many tokens remain in the current window, and a 429 arrives with Retry-After in seconds. A backfill script that honors those two headers won't get itself blocked.
Streaming live commodities prices over WebSocket
#
For dashboards and alerting, polling /v1/last in a loop burns quota on requests that mostly return the same number. The WebSocket stream pushes ticks as they happen. Connect with your key in the query string:
wss://stream.sifting.io/ws/v1?key=$SIFTING_KEY
You can also connect bare and send an explicit auth frame, { "op": "auth", "key": "sft_..." }, and the server replies with an ack that includes your tier. Then subscribe to the symbols you want:
{ "op": "subscribe", "product": "com", "symbols": ["XAUUSD", "WTIUSD"] }
Subscriptions are grouped by product. Commodities use the product code com, which is not the same as the REST venue slug commodities; the product codes for the other asset classes are in the docs.
On subscribe, the server first emits the last cached value for each symbol, so a UI renders immediately instead of waiting for the next trade. After that, ticks arrive as frames discriminated by an f field: f of "tick" with s (symbol), p (price), b and a (bid and ask), and t (epoch milliseconds). Two operational details matter. The server closes any connection that has been silent for 90 seconds, so send { "op": "ping" } at least once a minute. And failures are explicit: an f of "error" with a code such as max_subscriptions when you exceed your tier's symbol allowance, which is far easier to handle than a silent drop.
Common pitfalls
#
Forgetting gzip on historical bars. The bars endpoints require compression. A request without Accept-Encoding: gzip fails with HTTP 406 and the body { "error": "gzip_required" }. With curl, add the, compressed flag. Most full-featured HTTP clients negotiate gzip by default, but minimal clients and some serverless fetch wrappers don't, and the resulting 406 looks like a permissions problem until you read the error code.
Guessing symbol spellings. Copy identifiers from the
commodity symbol catalog
: XCUUSD for copper, UKOUSD for Brent crude oil, and NATGAS for natural gas. The USD suffix is not universal. Passing symbol-format validation does not establish that data is available for the requested symbol.
Treating a closed market as an outage. Metals and energy have trading sessions and weekend gaps, unlike crypto. Check the snapshot timestamp in
t
against the current time and your application's freshness requirements. A successful response alone does not establish that a price is fresh. Account for trading sessions and expected closures before treating an unchanged price as an outage.
Commodities is one market in the catalog, priced per market like every other asset class, and the free tier needs no credit card. Every request in this post runs unchanged on a free key.
Start building free
Tags
commodities data api
commodities market data
gold price api
xauusd
wtiusd
natgas
silver price api
ohlcv bars
websocket streaming
All posts
Last updated
October 9, 2026
Keep reading
Related posts
Commodities
Oct 21, 2026
7 min read
Gold-silver ratio in Python: calculate it from API prices
Calculate the gold-silver ratio with Python and commodity price data. Match timestamps, preserve missing bars and avoid misleading ratio candles.
SiftingIO Team
Commodities
Aug 1, 2026
6 min read
XAUUSD explained: gold price per gram, karat, and local currency math with a market data API
What XAUUSD means, the exact troy ounce to gram conversion, 24k/22k/18k karat math, and lira pricing via USDTRY, computed from real SiftingIO market data.
SiftingIO Team
Commodities
Aug 1, 2026
6 min read
Crude oil prices API: Brent (UKOUSD) and WTI (WTIUSD), historical bars and live quotes
Pull Brent (UKOUSD) and WTI (WTIUSD) crude oil prices from one API: historical OHLCV bars with cursor pagination, REST snapshots, and WebSocket streaming.
SiftingIO Team
Newsletter
Engineering & methodology updates
Deep-dives on the aggregation pipeline, methodology notes, and product releases. One email per release, no marketing fluff.
Email address
Subscribe
Website (leave empty)
We’ll send a confirmation link before adding you. Unsubscribe in one click.
sifting
/
io
Real-time and historical market data APIs for stocks, FX, crypto, commodities, DEX, and on-chain markets.
99.9% uptime
·
<100ms latency
·
24/7 support
Products
All products
Crypto data API
Forex data API
Stocks data API
DEX & DeFi data API
Developers
Documentation
Market Data Export
SDKs & libraries
Quickstart
WebSocket guide
FIX API
Official GitHub
Status page
Changelog
Integrations
All integrations
Excel
MCP
n8n
Supabase
Resources
All resources
Google Sheets template
Technical evaluation FAQ (PDF)
Guides
Data methodology
Free tools
Market hours
Symbol catalog
Use cases
Education program
Blog
Company
About
Pricing
Enterprise
Partnerships
Brand Assets
Request a dataset
Slack community
Book a call
Contact
Account
Login
Create account
Dashboard
Billing
Important.
SiftingIO (the “Service”) is operated by
SaltingIO LLC
(“SaltingIO”), a Wyoming limited liability company. The Service provides
aggregated
,
derived
, and
calculated
market data computed from multiple third-party sources. SaltingIO is
not an exchange
, broker, dealer, market maker, custodian, liquidity provider, or financial institution, and does not execute, route, match, or settle trades. The Service does
not
redistribute raw primary exchange feeds, official exchange-of-record prices, last-sale data, order books, or any venue’s proprietary data; outputs are synthetic reference values produced by SaltingIO’s aggregation, normalization, and fair-price methodologies and may differ materially from any specific venue, broker, or counterparty. All information is provided
“as is” and “as available”
without warranties of any kind, including accuracy, completeness, timeliness, fitness for a particular purpose, or non-infringement, and may contain errors, gaps, delays, or interruptions. Nothing on this site or in the Service constitutes investment, financial, trading, legal, tax, or accounting advice, an offer or solicitation to buy or sell any asset, or a recommendation of any strategy. Trading and investing involve substantial risk including total loss of capital; any decision or action taken in reliance on the data, including automated or algorithmic trading, is made
solely at the user’s own risk
, and SaltingIO, its parents, subsidiaries, affiliates, officers, directors, employees, agents, and licensors accept no liability for any direct, indirect, incidental, consequential, special, exemplary, or other losses arising from use of, or inability to use, the Service to the maximum extent permitted by law. Use of the Service is governed by the
Terms of Use
; customers are responsible for license compliance, accurate usage reporting, and ensuring the Service is appropriate for their jurisdiction and use case.
Read the full disclosures →
©
2026
SiftingIO · A SaltingIO LLC product · All rights reserved
Terms
Privacy
Cookies
Disclaimer
Cookie preferences
All systems operational

## https://sifting.io/blog/streaming-gold-silver-oil-prices-websocket-commodities-api -> 200 (215964 chars, ct=text/html; charset=utf-8)
Streaming gold, silver, and oil prices: a WebSocket commodities price API · SiftingIO
sifting
/
io
Product
Market data APIs
Market data.
One API.
Live and historical data across markets, through REST and WebSocket.
Explore all data APIs
Forex data API
Live currency quotes & price history
US stock data API
Stock quotes, trades & historical bars
Crypto data API
Aggregated prices & historical bars
DEX & DeFi data API
On-chain prices, swaps & liquidity
Commodity data API
Metals, energy & agricultural prices
Fundamentals API
SEC filings, financials & ownership
Need a market or dataset we don't cover?
Request a dataset
API docs
Pricing
Resources
Developers
Market data SDKs
Go, Python & JavaScript clients
Integrations
Excel, workflows & AI agents
Market data tools
Download market data
Historical prices in CSV & Excel
Free market data tools
Downloads, market hours & widgets
Market hours & holidays
Trading schedules & live market status
Market data symbols
Find stock tickers & trading pairs
Guides & research
Market data guides
Understand prices, bars & API concepts
Market data use cases
Trading, research & fintech workflows
Education program
Market data for students & researchers
SiftingIO blog
API tutorials & product updates
Tools, integration guides and practical examples.
Browse all resources
Contact
Log in
Start for free
Dashboard
sifting
/
io
Product
Forex data API
US stock data API
Crypto data API
DEX & DeFi data API
Commodity data API
Fundamentals API
Explore
API docs
Pricing
Resources
Contact
Historical prices in CSV & Excel
Download market data
Request a dataset
Tell us which market, instrument or dataset you need.
Log in
Start for Free
Dashboard
Home
Blog
Commodities
Commodities
July 30, 2026
·
6 min read
·
SiftingIO Team
Streaming gold, silver, and oil prices: a WebSocket commodities price API
Stream live gold, silver, and oil prices over one WebSocket connection instead of polling REST. Subscribe protocol, tick format, and a reconnect-safe client.
How do you get a live gold price into an app without polling a REST endpoint in a loop Most commodities price APIs are REST-only, so the standard workaround is a timer that fetches a spot quote every few seconds. That timer burns through a monthly call quota, adds a request's worth of latency to every reading, and still leaves the displayed price as old as the polling interval. A WebSocket commodities feed removes the loop entirely: open one connection, subscribe to XAUUSD, XAGUSD, and WTIUSD, and the server pushes each new price as it's published.
This post covers
streaming precious metals and energy prices
from SiftingIO's WebSocket API: the subscribe protocol, the exact shape of a tick, and a reconnect-safe client you can drop into a dashboard or an alerting job. For one-off fetches of the same symbols over REST, see
getting gold and oil spot prices via API
. This post is about keeping a price continuously current.
Why polling a commodities price API doesn't scale
#
Suppose a dashboard shows gold, silver, and crude and refreshes once a second. That's three REST calls per second, about 259,000 calls per day, per browser tab if the polling runs client-side. Metered plans price REST by monthly call volume, so a polling loop converts directly into quota: SiftingIO's free tier includes 10,000 REST calls a month, which one-second polling of a single symbol exhausts in under three hours.
Slower polling saves quota but caps freshness. Poll every 30 seconds and every reading is up to 30 seconds stale, which is a long time for an alert that fires when crude crosses a threshold.
A push feed changes the arithmetic. The free tier includes one WebSocket connection with five concurrent symbol subscriptions, and updates arrive as the server publishes them with no per-message metering. The connection is the cost, and there's exactly one.
One connection, gold, silver, and oil
#
Authentication happens in the URL with the
?key=
query parameter (or an explicit
{"op":"auth","key":"..."}
frame after connecting). After auth succeeds the server replies with an ack frame that includes your plan tier. Commodities live under the
com
product; use the codes from the commodity symbol catalog: XAUUSD for gold, XAGUSD for silver, WTIUSD and UKOUSD for crude, NATGAS for natural gas.
import WebSocket from "ws"; // Node 18+; browsers have WebSocket built in
const ws = new WebSocket(
`wss://stream.sifting.io/ws/v1?key=${process.env.SIFTING_KEY}`
);
ws.on("open", () => {
ws.send(JSON.stringify({
op: "subscribe",
product: "com",
symbols: ["XAUUSD", "XAGUSD", "WTIUSD"]
}));
});
ws.on("message", (raw) => {
const msg = JSON.parse(raw);
if (msg.f === "tick") {
console.log(msg.s, msg.p, "bid", msg.b, "ask", msg.a,
new Date(msg.t).toISOString());
}
});
Server frames discriminate on the
f
field. This illustrative update shows the numeric WebSocket fields; the values are placeholders, not a live quote:
{ "f": "tick", "class": "spot", "s": "XAUUSD", "p": 0.0, "P": 0.0, "b": 0.0, "B": 0.0, "a": 0.0, "A": 0.0, "t": 1778019852426 }
p
is the reference price;
P
is its size field. The
b
and
a
fields are bid and ask,
B
and
A
their sizes, and
t
is a Unix timestamp in milliseconds. Immediately after a subscribe the server sends the last cached value for each symbol, then live updates. That first frame lets you paint the UI without a blank state, but treat its timestamp with suspicion (more on that below).
A reconnect-safe client
#
Long-lived connections drop. Wi-Fi changes, laptops sleep, load balancers rotate. A production client needs three behaviors: a keepalive ping, resubscription on reconnect, and stale-tick rejection so the cached value replayed after a reconnect can't overwrite a newer price.
import WebSocket from "ws";
const SYMBOLS = ["XAUUSD", "XAGUSD", "WTIUSD", "UKOUSD", "NATGAS"];
const last = new Map();
function handleTick(tick) {
const prev = last.get(tick.s);
if (prev && tick.t <= prev.t) return; // replayed cache or out-of-order frame
last.set(tick.s, tick);
// update the UI, evaluate alert rules, append to your store
}
function connect(attempt = 0) {
const ws = new WebSocket(
`wss://stream.sifting.io/ws/v1?key=${process.env.SIFTING_KEY}`
);
let pingTimer;
ws.on("open", () => {
attempt = 0;
ws.send(JSON.stringify({ op: "subscribe", product: "com", symbols: SYMBOLS }));
pingTimer = setInterval(
() => ws.send(JSON.stringify({ op: "ping" })),
30_000
);
});
ws.on("message", (raw) => {
const msg = JSON.parse(raw);
if (msg.f === "tick") handleTick(msg);
if (msg.f === "error") console.error("stream error:", msg.code);
});
ws.on("close", () => {
clearInterval(pingTimer);
const delay = Math.min(30_000, 1_000 * 2 ** attempt);
setTimeout(() => connect(attempt + 1), delay);
});
ws.on("error", () => ws.close());
}
connect();
Five symbols is a deliberate choice: it fits exactly inside the free tier's five-subscription limit, so this snippet runs on a key from a free account. A sixth symbol gets an error frame with code
max_subscriptions
and the subscribe is rejected.
What the price you're streaming actually is
#
Each tick carries a consensus price aggregated from multiple independent venues, formed as a volume- and reputation-weighted median with staleness and outlier filtering in front of it. One venue printing a bad gold price doesn't move the published value; a majority of venues would have to err in the same direction. The method is documented at
/data-methodology
.
Updates arrive as the server publishes new observations. Use each tick's timestamp to assess freshness rather than assuming a fixed update rate. The stream is a reference feed for display, alerting, and analytics; it isn't an execution feed and shouldn't be treated as executable depth.
Common pitfalls
#
The gateway counts both client messages and successfully delivered data as activity. A connection with no application traffic can close after the default 90-second idle window. Send
{"op":"ping"}
every 30 seconds to keep quiet sessions active, including outside market hours; the client above does this.
The first frame after subscribe can be hours old. Metals and energy trade roughly around the clock on weekdays but pause over the weekend. Subscribe on a Saturday and the cached XAUUSD value you receive dates from the Friday close. An alerting job that treats that replayed frame as a fresh print can fire on stale data, so always check
t
before acting on a tick and gate alerts on recency.
Reconnect loops can strand your only connection slot. The free tier allows one concurrent connection, and an implementation that opens a new socket before the old one has fully closed can hit an error frame with code
max_connections
. A crash-looping process can lock itself out until the server reaps the dead connection. Reconnect from the
close
event, never from
error
alone, and back off exponentially as the client above does.
One smaller trap: the product name is
com
, and sending
"product": "commodities"
returns an
unknown_product
error frame.
Streaming needs a key but no card.
Start building free
: the free tier includes one WebSocket connection and five symbol subscriptions.
Tags
commodities
websocket
gold
silver
oil
xauusd
streaming
real-time prices
javascript
market data api
All posts
Last updated
October 20, 2026
Keep reading
Related posts
Commodities
Oct 21, 2026
7 min read
Gold-silver ratio in Python: calculate it from API prices
Calculate the gold-silver ratio with Python and commodity price data. Match timestamps, preserve missing bars and avoid misleading ratio candles.
SiftingIO Team
Commodities
Aug 14, 2026
5 min read
Commodities data API: coverage, fields, and symbol names for gold, silver, oil, and metals
SiftingIO's commodities data API: real symbols (XAUUSD, XAGUSD, WTIUSD, XCUUSD, NATGAS), OHLCV history, live REST quotes, and WebSocket streaming.
SiftingIO Team
Commodities
Aug 1, 2026
6 min read
XAUUSD explained: gold price per gram, karat, and local currency math with a market data API
What XAUUSD means, the exact troy ounce to gram conversion, 24k/22k/18k karat math, and lira pricing via USDTRY, computed from real SiftingIO market data.
SiftingIO Team
Newsletter
Engineering & methodology updates
Deep-dives on the aggregation pipeline, methodology notes, and product releases. One email per release, no marketing fluff.
Email address
Subscribe
Website (leave empty)
We’ll send a confirmation link before adding you. Unsubscribe in one click.
sifting
/
io
Real-time and historical market data APIs for stocks, FX, crypto, commodities, DEX, and on-chain markets.
99.9% uptime
·
<100ms latency
·
24/7 support
Products
All products
Crypto data API
Forex data API
Stocks data API
DEX & DeFi data API
Developers
Documentation
Market Data Export
SDKs & libraries
Quickstart
WebSocket guide
FIX API
Official GitHub
Status page
Changelog
Integrations
All integrations
Excel
MCP
n8n
Supabase
Resources
All resources
Google Sheets template
Technical evaluation FAQ (PDF)
Guides
Data methodology
Free tools
Market hours
Symbol catalog
Use cases
Education program
Blog
Company
About
Pricing
Enterprise
Partnerships
Brand Assets
Request a dataset
Slack community
Book a call
Contact
Account
Login
Create account
Dashboard
Billing
Important.
SiftingIO (the “Service”) is operated by
SaltingIO LLC
(“SaltingIO”), a Wyoming limited liability company. The Service provides
aggregated
,
derived
, and
calculated
market data computed from multiple third-party sources. SaltingIO is
not an exchange
, broker, dealer, market maker, custodian, liquidity provider, or financial institution, and does not execute, route, match, or settle trades. The Service does
not
redistribute raw primary exchange feeds, official exchange-of-record prices, last-sale data, order books, or any venue’s proprietary data; outputs are synthetic reference values produced by SaltingIO’s aggregation, normalization, and fair-price methodologies and may differ materially from any specific venue, broker, or counterparty. All information is provided
“as is” and “as available”
without warranties of any kind, including accuracy, completeness, timeliness, fitness for a particular purpose, or non-infringement, and may contain errors, gaps, delays, or interruptions. Nothing on this site or in the Service constitutes investment, financial, trading, legal, tax, or accounting advice, an offer or solicitation to buy or sell any asset, or a recommendation of any strategy. Trading and investing involve substantial risk including total loss of capital; any decision or action taken in reliance on the data, including automated or algorithmic trading, is made
solely at the user’s own risk
, and SaltingIO, its parents, subsidiaries, affiliates, officers, directors, employees, agents, and licensors accept no liability for any direct, indirect, incidental, consequential, special, exemplary, or other losses arising from use of, or inability to use, the Service to the maximum extent permitted by law. Use of the Service is governed by the
Terms of Use
; customers are responsible for license compliance, accurate usage reporting, and ensuring the Service is appropriate for their jurisdiction and use case.
Read the full disclosures →
©
2026
SiftingIO · A SaltingIO LLC product · All rights reserved
Terms
Privacy
Cookies
Disclaimer
Cookie preferences
All systems operational

