# Deriv options probe — `frxXAUUSD`

Recorded by `.github/workflows/deriv-probe.yml` on 2026-10-05 19:12 UTC.

```

=== current API - public WebSocket (same schema as the OTP sockets)
    url: wss://api.derivws.com/trading/v1/options/ws/public
    contracts_for: 16 entries
      - CALL        expiry=daily     barriers=1  duration=1d..365d market=commodities
      - PUT         expiry=daily     barriers=1  duration=1d..365d market=commodities
      - CALL        expiry=intraday  barriers=1  duration=5m..1d market=commodities
      - PUT         expiry=intraday  barriers=1  duration=5m..1d market=commodities
      - EXPIRYMISS  expiry=daily     barriers=2  duration=7d..365d market=commodities
      - EXPIRYRANGE expiry=daily     barriers=2  duration=7d..365d market=commodities
      - EXPIRYMISSE expiry=daily     barriers=2  duration=7d..365d market=commodities
      - EXPIRYRANGEE expiry=daily     barriers=2  duration=7d..365d market=commodities
      - HIGHER      expiry=daily     barriers=1  duration=1d..365d market=commodities
      - LOWER       expiry=daily     barriers=1  duration=1d..365d market=commodities
      - MULTUP      expiry=no_expiry barriers=0  duration=0..0 market=commodities
      - MULTDOWN    expiry=no_expiry barriers=0  duration=0..0 market=commodities
      - RANGE       expiry=daily     barriers=2  duration=7d..365d market=commodities
      - UPORDOWN    expiry=daily     barriers=2  duration=7d..365d market=commodities
      - ONETOUCH    expiry=daily     barriers=1  duration=1d..365d market=commodities
      - NOTOUCH     expiry=daily     barriers=1  duration=1d..365d market=commodities
    raw entries Node 3 can trade intraday (all fields Deriv returns):
      {"barrier": "+2.20", "barriers": 1, "contract_category": "callput", "contract_type": "CALL", "default_stake": 2, "expiry_type": "intraday", "market": "commodities", "max_contract_duration": "1d", "min_contract_duration": "5m", "sentiment": "up", "submarket": "metals", "underlying_symbol": "frxXAUUSD"}
      {"barrier": "+2.20", "barriers": 1, "contract_category": "callput", "contract_type": "PUT", "default_stake": 2, "expiry_type": "intraday", "market": "commodities", "max_contract_duration": "1d", "min_contract_duration": "5m", "sentiment": "down", "submarket": "metals", "underlying_symbol": "frxXAUUSD"}
    proposal sweep at stake 0.5 USD, 5m:
    [CALL, no barrier                          ] OK id=441f93d2-a545-c705-4863-f1eb36631b76 ask_price=0.5 payout=0.87 spot=4140.54 barrier=-
        Win payout if Gold/USD is strictly higher than entry spot at 5 minutes after contract start time.
    [CALL barrier +0.01                        ] REJECTED ContractBuyValidationError: Invalid barrier. (subcode InvalidBarrier)
    [CALL barrier +0.05                        ] REJECTED ContractBuyValidationError: Invalid barrier. (subcode InvalidBarrier)
    [CALL barrier +0.10                        ] REJECTED ContractBuyValidationError: Invalid barrier. (subcode InvalidBarrier)
    [CALL barrier +0.50                        ] REJECTED ContractBuyValidationError: Invalid barrier. (subcode InvalidBarrier)
    [CALL barrier +1.00                        ] REJECTED ContractBuyValidationError: Invalid barrier. (subcode InvalidBarrier)
    [CALL barrier +2.50 (production shape)     ] REJECTED ContractBuyValidationError: Invalid barrier. (subcode InvalidBarrier)
    [CALL barrier +6.00 (TP distance on gold)  ] REJECTED ContractBuyValidationError: Invalid barrier. (subcode InvalidBarrier)
    [CALL barrier +15.00                       ] REJECTED ContractBuyValidationError: Invalid barrier. (subcode InvalidBarrier)
    [CALL barrier -2.50 (wrong sign)           ] REJECTED ContractBuyValidationError: Invalid barrier. (subcode InvalidBarrier)
    [PUT, no barrier                           ] OK id=1511a4a1-3582-b47d-a3ea-4891a23c544a ask_price=0.5 payout=0.85 spot=4140.58 barrier=-
        Win payout if Gold/USD is strictly lower than entry spot at 5 minutes after contract start time.
    [PUT barrier -0.01                         ] REJECTED ContractBuyValidationError: Invalid barrier. (subcode InvalidBarrier)
    [PUT barrier -0.10                         ] REJECTED ContractBuyValidationError: Invalid barrier. (subcode InvalidBarrier)
    [PUT barrier -0.50                         ] REJECTED ContractBuyValidationError: Invalid barrier. (subcode InvalidBarrier)
    [PUT barrier -2.50 (production shape)      ] REJECTED ContractBuyValidationError: Invalid barrier. (subcode InvalidBarrier)
    [PUT barrier -6.00 (TP distance on gold)   ] REJECTED ContractBuyValidationError: Invalid barrier. (subcode InvalidBarrier)
    sweep CALL: accepted barriers = none; rejected = +0.01, +0.05, +0.10, +0.50, +1.00, +2.50, +6.00, +15.00, -2.50
    sweep PUT: accepted barriers = none; rejected = -0.01, -0.10, -0.50, -2.50, -6.00

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
