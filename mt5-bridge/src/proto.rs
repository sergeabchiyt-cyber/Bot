//! Local EA ⇄ bridge line protocol.
//!
//! # Why the EA dials in
//!
//! MQL5's built-in socket API can only create **client** sockets
//! (`SocketCreate` / `SocketConnect`); it has no `SocketBind` / `SocketListen` /
//! `SocketAccept`. A Rust process therefore cannot connect to the terminal —
//! the terminal must connect to something. The bridge listens on
//! `MT5_EA_BIND_ADDR:MT5_EA_PORT` (loopback by default) and the
//! `Mt5BridgeEA.mq5` expert advisor dials in and keeps the connection open.
//!
//! # Framing
//!
//! One message per `\n`-terminated ASCII line. Every value is percent-encoded
//! (space, `%`, `=`, tab, CR, LF) so values can never break tokenisation, and
//! `msg`/`comment` text can carry spaces safely.
//!
//! ```text
//! EA     -> bridge : HELLO token=.. build=.. login=.. server=.. mode=demo company=.. currency=USD ea=1.0.0
//! bridge -> EA     : HELLOOK proto=1
//! bridge -> EA     : REQ 7 ORDER_SEND intent=N4-1 symbol=XAUUSD side=buy volume=0.01 sl=2647.5 tp=2656.0 deviation=20 magic=330033 comment=N4-1
//! EA     -> bridge : RESP 7 OK status=filled retcode=10009 order=123 deal=456 position=789 price=2650.12 volume=0.01
//! EA     -> bridge : ITEM 7 ticket=789 symbol=XAUUSD side=buy volume=0.01
//! EA     -> bridge : END 7 count=1
//! EA     -> bridge : HB mode=demo connected=1 trade_allowed=1 login=123456 ts=1700000000000
//! EA     -> bridge : EVT TRADE kind=position_closed position=789 profit=12.5
//! ```
//!
//! Requests carry a monotonically increasing id; responses and list payloads
//! reference the same id. List responses are `RESP <id> OK count=N` followed by
//! `N` `ITEM <id> ...` lines and a terminating `END <id> count=N`.

use std::collections::BTreeMap;

/// Methods the bridge can send to the EA. Kept in one place so the Rust and
/// MQL5 sides can be diffed by `ci/mt5/protocol_lint.py`.
pub mod method {
    pub const ACCOUNT: &str = "ACCOUNT";
    pub const SYMBOL: &str = "SYMBOL";
    pub const QUOTE: &str = "QUOTE";
    pub const POSITIONS: &str = "POSITIONS";
    pub const ORDERS: &str = "ORDERS";
    pub const HISTORY: &str = "HISTORY";
    pub const FIND: &str = "FIND";
    pub const ORDER_SEND: &str = "ORDER_SEND";
    pub const POS_MODIFY: &str = "POS_MODIFY";
    pub const POS_CLOSE: &str = "POS_CLOSE";
    pub const CLOSE_ALL: &str = "CLOSE_ALL";
    pub const ORDER_CANCEL: &str = "ORDER_CANCEL";
    pub const PING: &str = "PING";

    /// Every method, for the protocol lint and for the EA's dispatch check.
    pub const ALL: &[&str] = &[
        ACCOUNT,
        SYMBOL,
        QUOTE,
        POSITIONS,
        ORDERS,
        HISTORY,
        FIND,
        ORDER_SEND,
        POS_MODIFY,
        POS_CLOSE,
        CLOSE_ALL,
        ORDER_CANCEL,
        PING,
    ];
}

/// Percent-encode a value for the wire.
pub fn enc(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            ' ' => out.push_str("%20"),
            '%' => out.push_str("%25"),
            '=' => out.push_str("%3D"),
            '\t' => out.push_str("%09"),
            '\r' => out.push_str("%0D"),
            '\n' => out.push_str("%0A"),
            _ => out.push(ch),
        }
    }
    out
}

/// Reverse of [`enc`]. Unknown/incomplete escapes are passed through verbatim
/// so a malformed frame degrades into a readable string instead of panicking.
pub fn dec(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = String::with_capacity(value.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &value[i + 1..i + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte as char);
                i += 3;
                continue;
            }
        }
        // Multi-byte UTF-8 is copied byte-wise; pushing `char` from a byte
        // keeps ASCII intact, which is all the EA link is specified to carry.
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EaKind {
    Hello,
    Resp,
    Item,
    End,
    Hb,
    Evt,
    /// Unrecognised line; the bridge logs and ignores it rather than
    /// desynchronising the stream.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EaMessage {
    pub kind: EaKind,
    pub id: Option<u64>,
    /// `OK` / `ERR` for responses, the event name for `EVT`, else the keyword.
    pub name: String,
    pub fields: BTreeMap<String, String>,
}

impl EaMessage {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(|s| s.as_str())
    }

    pub fn get_str(&self, key: &str) -> Option<String> {
        self.get(key).map(|s| s.to_string())
    }

    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(|v| v.parse::<f64>().ok())
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.get(key)
            .and_then(|v| v.parse::<i64>().ok())
            .or_else(|| self.get(key).and_then(|v| v.parse::<f64>().ok().map(|f| f as i64)))
    }

    pub fn get_u64(&self, key: &str) -> Option<u64> {
        self.get_i64(key).and_then(|v| u64::try_from(v).ok())
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).map(|v| {
            matches!(
                v.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "ok"
            )
        })
    }
}

/// Encode a bridge → EA request line.
pub fn encode_req(id: u64, method: &str, params: &[(&str, String)]) -> String {
    let mut line = format!("REQ {id} {method}");
    for (key, value) in params {
        line.push(' ');
        line.push_str(key);
        line.push('=');
        line.push_str(&enc(value));
    }
    line
}

/// Encode the bridge's `HELLOOK`/`HELLOERR` answer to the EA's greeting.
pub fn encode_hello_reply(ok: bool, reason: &str) -> String {
    if ok {
        "HELLOOK proto=1".to_string()
    } else {
        format!("HELLOERR reason={}", enc(reason))
    }
}

fn parse_fields(tokens: &[&str]) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    for token in tokens {
        if let Some((key, value)) = token.split_once('=') {
            fields.insert(key.to_string(), dec(value));
        } else if !token.is_empty() {
            fields.insert(token.to_string(), String::new());
        }
    }
    fields
}

/// Parse one EA → bridge line.
pub fn parse_line(line: &str) -> Option<EaMessage> {
    let line = line.trim_end_matches(['\r', '\n']).trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let tokens: Vec<&str> = line.split(' ').filter(|t| !t.is_empty()).collect();
    let head = tokens.first()?.to_ascii_uppercase();

    match head.as_str() {
        "HELLO" => Some(EaMessage {
            kind: EaKind::Hello,
            id: None,
            name: "HELLO".into(),
            fields: parse_fields(&tokens[1..]),
        }),
        "HB" => Some(EaMessage {
            kind: EaKind::Hb,
            id: None,
            name: "HB".into(),
            fields: parse_fields(&tokens[1..]),
        }),
        "EVT" => Some(EaMessage {
            kind: EaKind::Evt,
            id: None,
            name: tokens
                .get(1)
                .map(|s| s.to_ascii_uppercase())
                .unwrap_or_else(|| "UNKNOWN".into()),
            fields: parse_fields(&tokens[2.min(tokens.len())..]),
        }),
        "RESP" => {
            let id = tokens.get(1).and_then(|t| t.parse::<u64>().ok())?;
            let name = tokens
                .get(2)
                .map(|s| s.to_ascii_uppercase())
                .unwrap_or_else(|| "ERR".into());
            Some(EaMessage {
                kind: EaKind::Resp,
                id: Some(id),
                name,
                fields: parse_fields(&tokens[3.min(tokens.len())..]),
            })
        }
        "ITEM" => {
            let id = tokens.get(1).and_then(|t| t.parse::<u64>().ok())?;
            Some(EaMessage {
                kind: EaKind::Item,
                id: Some(id),
                name: "ITEM".into(),
                fields: parse_fields(&tokens[2.min(tokens.len())..]),
            })
        }
        "END" => {
            let id = tokens.get(1).and_then(|t| t.parse::<u64>().ok())?;
            Some(EaMessage {
                kind: EaKind::End,
                id: Some(id),
                name: "END".into(),
                fields: parse_fields(&tokens[2.min(tokens.len())..]),
            })
        }
        _ => Some(EaMessage {
            kind: EaKind::Unknown,
            id: None,
            name: head,
            fields: parse_fields(&tokens[1.min(tokens.len())..]),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_codec_round_trips_spaces_and_separators() {
        let raw = "N4 order 42=50% done";
        let encoded = enc(raw);
        assert!(!encoded.contains(' '));
        assert!(!encoded.contains('='));
        assert_eq!(dec(&encoded), raw);
    }

    #[test]
    fn encodes_requests_without_raw_spaces() {
        let line = encode_req(
            7,
            method::ORDER_SEND,
            &[
                ("intent", "N4-1".into()),
                ("comment", "PW PoC retest".into()),
                ("volume", "0.01".into()),
            ],
        );
        assert_eq!(line.split(' ').count(), 3 + 3);
        assert!(line.contains("comment=PW%20PoC%20retest"));
        assert!(parse_line(&line).is_none() || parse_line(&line).unwrap().kind == EaKind::Unknown);
    }

    #[test]
    fn parses_hello_heartbeat_and_events() {
        let hello = parse_line(
            "HELLO token=t build=4755 login=123456 server=Deriv-Demo mode=demo company=Deriv currency=USD ea=1.0.0",
        )
        .unwrap();
        assert_eq!(hello.kind, EaKind::Hello);
        assert_eq!(hello.get("mode"), Some("demo"));
        assert_eq!(hello.get_i64("login"), Some(123_456));

        let hb = parse_line("HB mode=demo connected=1 trade_allowed=1 login=123456 ts=1700").unwrap();
        assert_eq!(hb.kind, EaKind::Hb);
        assert_eq!(hb.get_bool("connected"), Some(true));
        assert_eq!(hb.get_bool("trade_allowed"), Some(true));

        let evt = parse_line("EVT TRADE kind=position_closed position=789 profit=12.5").unwrap();
        assert_eq!(evt.kind, EaKind::Evt);
        assert_eq!(evt.name, "TRADE");
        assert_eq!(evt.get("kind"), Some("position_closed"));
        assert_eq!(evt.get_f64("profit"), Some(12.5));
    }

    #[test]
    fn parses_responses_and_list_frames() {
        let resp = parse_line(
            "RESP 12 OK status=filled retcode=10009 order=1 deal=2 position=3 price=2650.12 volume=0.01",
        )
        .unwrap();
        assert_eq!(resp.kind, EaKind::Resp);
        assert_eq!(resp.id, Some(12));
        assert_eq!(resp.name, "OK");
        assert_eq!(resp.get_i64("position"), Some(3));
        assert_eq!(resp.get_f64("price"), Some(2650.12));

        let err = parse_line("RESP 12 ERR code=10016 msg=invalid%20stops").unwrap();
        assert_eq!(err.name, "ERR");
        assert_eq!(err.get_i64("code"), Some(10016));
        assert_eq!(err.get("msg"), Some("invalid stops"));

        let item = parse_line("ITEM 12 ticket=3 symbol=XAUUSD volume=0.01").unwrap();
        assert_eq!(item.kind, EaKind::Item);
        assert_eq!(item.id, Some(12));

        let end = parse_line("END 12 count=1").unwrap();
        assert_eq!(end.kind, EaKind::End);
        assert_eq!(end.get_u64("count"), Some(1));
    }

    #[test]
    fn tolerates_garbage_without_losing_the_stream() {
        assert!(parse_line("").is_none());
        assert!(parse_line("   ").is_none());
        assert!(parse_line("# comment").is_none());
        assert_eq!(parse_line("WHAT is this").unwrap().kind, EaKind::Unknown);
        // A malformed RESP id cannot panic.
        assert!(parse_line("RESP notanumber OK").is_none());
    }
}
