# Deriv options probe — `frxXAUUSD`

Recorded by `.github/workflows/deriv-probe.yml` on 2026-10-05 19:10 UTC.

```

=== current API - public WebSocket (same schema as the OTP sockets)
    url: wss://api.derivws.com/trading/v1/options/ws/public
    contracts_for: 16 entries
      - CALL    expiry=daily     barriers=1  duration=1d..365d market=commodities
      - PUT     expiry=daily     barriers=1  duration=1d..365d market=commodities
      - CALL    expiry=intraday  barriers=1  duration=5m..1d market=commodities
      - PUT     expiry=intraday  barriers=1  duration=5m..1d market=commodities
      - EXPIRYMISS expiry=daily     barriers=2  duration=7d..365d market=commodities
      - EXPIRYRANGE expiry=daily     barriers=2  duration=7d..365d market=commodities
      - EXPIRYMISSE expiry=daily     barriers=2  duration=7d..365d market=commodities
      - EXPIRYRANGEE expiry=daily     barriers=2  duration=7d..365d market=commodities
      - HIGHER  expiry=daily     barriers=1  duration=1d..365d market=commodities
      - LOWER   expiry=daily     barriers=1  duration=1d..365d market=commodities
      - MULTUP  expiry=no_expiry barriers=0  duration=0..0 market=commodities
      - MULTDOWN expiry=no_expiry barriers=0  duration=0..0 market=commodities
      - RANGE   expiry=daily     barriers=2  duration=7d..365d market=commodities
      - UPORDOWN expiry=daily     barriers=2  duration=7d..365d market=commodities
      - ONETOUCH expiry=daily     barriers=1  duration=1d..365d market=commodities
      - NOTOUCH expiry=daily     barriers=1  duration=1d..365d market=commodities
    [ATM rise, no barrier (Node 3 fix shape)] REJECTED ContractBuyValidationError: Please enter a stake amount that's at least 0.50. (subcode InvalidMinStake)
    [ATM fall, no barrier (Node 3 fix shape)] REJECTED ContractBuyValidationError: Please enter a stake amount that's at least 0.50. (subcode InvalidMinStake)
    [CALL + relative barrier (production failure shape)] REJECTED ContractBuyValidationError: Please enter a stake amount that's at least 0.50. (subcode InvalidMinStake)
    [PUT + relative barrier (production failure shape)] REJECTED ContractBuyValidationError: Please enter a stake amount that's at least 0.50. (subcode InvalidMinStake)
    summary: 0 priced, 4 rejected

=== legacy /websockets/v3 (a1-... token failover flow)
    url: wss://ws.derivws.com/websockets/v3?app_id=1089
    UNREACHABLE: InvalidStatus: server rejected WebSocket connection: HTTP 520

=== legacy /websockets/v3 (failover host)
    url: wss://ws.binaryws.com/websockets/v3?app_id=1089
    UNREACHABLE: InvalidStatus: server rejected WebSocket connection: HTTP 520

=== result
    current API public socket: ok
    legacy socket:             unreachable
```
