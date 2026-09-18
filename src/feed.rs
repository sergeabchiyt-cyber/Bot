use chrono::{DateTime, Utc}:
use crate::config::FeedCfg;
use crate::types::Candle;

pub type FResult = Result<Vec<Candle>, Box<dyn std::error::Error + Send + Sync>>;

pub enum Feed { Oanda(Oanda), Csv(Csv) }

impl Feed {
    pub fn from_cfg(c: &FeedCfg) -> Self {
        match c.kind.as_str() {
            "csv" => Feed::Csv(Csv { dir: c.csv_dir.clone(), tz: -4 }),
            _ => Feed::Oanda(Oanda {
                client: reqwest::Client::new(),
                url: c.oanda_url.clone(), inst: c.instrument.clone(), price: c.price.clone(),
                token: std::env::var("OANDA_TOKEN").unwrap_or_default(),
            }),
        }
    }
    pub async fn candles(&self, tf: &str, count: u32) -> FResult {
        match self { Feed::Oanda(f) => f.candles(tf, count).await, Feed::Csv(f) => f.candles(tf, count).await }
    }
}

pub struct Oanda { pub client: reqwest::Client, pub url: String, pub inst: String, pub price: String, pub token: String }

impl Oanda {
    // VERIFY against the OANDA v20 reference before first live run (do not trust this comment):
    //   path /v3/instruments/{instrument}/candles, granularity tokens M15|H1|D,
    //   price=M returns "mid" (B/A return "bid"/"ask"), "volume" = tick count,
    //   filter complete==true for closed candles.
    pub async fn candles(&self, tf: &str, count: u32) -> FResult {
        let url = format!("{}/v3/instruments/{}/candles?granularity={tf}&count={count}&price={}",
                          self.url, self.inst, self.price);
        let v: serde_json::Value = self.client.get(&url)
            .bearer_auth(&self.token).send().await?.error_for_status()?.json().await?;
        let key = match self.price.as_str() { "B" => "bid", "A" => "ask", _ => "mid" };
        let mut out = Vec::new();
        for c in v["candles"].as_array().unwrap_or(&Vec::new()) {
            if c["complete"].as_bool() == Some(false) { continue; }
            let t = DateTime::parse_from_rfc3339(c["time"].as_str().unwrap_or(""))?.with_timezone(&Utc);
            let m = &c[key];
            out.push(Candle {
                t: t.timestamp(),
                o: m["o"].as_str().unwrap_or("0").parse()?, h: m["h"].as_str().unwrap_or("0").parse()?,
                l: m["l"].as_str().unwrap_or("0").parse()?, c: m["c"].as_str().unwrap_or("0").parse()?,
                v: c["volume"].as_f64().unwrap_or(1.0),
            });
        }
        Ok(out)
    }
}

pub struct Csv { pub dir: String, pub tz: i32 }

impl Csv {
    /// files: data/M15.csv, data/H1.csv, data/D.csv — TradingView export, chart TZ = self.tz
    pub async fn candles(&self, tf: &str, _count: u32) -> FResult {
        use chrono::{FixedOffset, NaiveDateTime, TimeZone};
        let off = FixedOffset::east_opt(self.tz * 3600).unwrap();
        let path = format!("{}/{}.csv", self.dir, tf.to_lowercase());
        let mut out = Vec::new();
        for (i, line) in std::fs::read_to_string(&path)?.lines().enumerate() {
            if i == 0 || line.trim().is_empty() { continue; }
            let f: Vec<&str> = line.split(',').collect();
            let lt = NaiveDateTime::parse_from_str(f[0].trim(), "%Y-%m-%d %H:%M:%S")?;
            let t = off.from_local_datetime(&lt).single().unwrap().with_timezone(&Utc);
            out.push(Candle { t: t.timestamp(), o: f[1].parse()?, h: f[2].parse()?,
                              l: f[3].parse()?, c: f[4].parse()?,
                              v: f.get(5).and_then(|v| v.parse().ok()).unwrap_or(1.0) });
        }
        out.sort_by_key(|k| k.t);
        Ok(out)
    }
}
