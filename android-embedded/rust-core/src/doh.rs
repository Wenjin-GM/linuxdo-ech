//! DoH 解析：多端点冗余 + 重试，取真实 IP 与 ECH 配置。
//! 走 DoH JSON（application/dns-json）接口，HTTPS(type=65) 记录以展示文本返回，
//! 直接从文本中提取 ech="..." 的 base64，避免实现完整二进制 SVCB 解析。

use std::net::IpAddr;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Resolved {
    pub ips: Vec<IpAddr>,
    /// ECHConfigList 的原始字节（可直接喂给 rustls EchConfig::new）。
    pub ech_list: Option<Vec<u8>>,
}

#[derive(Deserialize)]
struct DohJson {
    #[serde(rename = "Answer", default)]
    answer: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "type")]
    rtype: u16,
    data: String,
}

pub struct Doh {
    http: reqwest::Client,
    endpoints: Vec<String>,
}

impl Doh {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(3))
            .build()
            .expect("build doh client");
        // 顺序即优先级：国内可达的 DNSPod 优先，备用私人端点与 IP 直连。
        let endpoints = vec![
            "https://doh.pub/dns-query".to_string(),
            "https://1.12.12.12/dns-query".to_string(),
            "https://aaa.ddd.oaifree.com/query-dns".to_string(),
            "https://1.1.1.1/dns-query".to_string(),
            "https://8.8.8.8/resolve".to_string(),
        ];
        Self { http, endpoints }
    }

    async fn query_once(&self, endpoint: &str, host: &str, rtype: &str) -> Result<Vec<Entry>> {
        let resp = self
            .http
            .get(endpoint)
            .query(&[("name", host), ("type", rtype)])
            .header("accept", "application/dns-json")
            .send()
            .await
            .with_context(|| format!("DoH 请求失败 {endpoint}"))?
            .error_for_status()?;
        let parsed: DohJson = resp.json().await.context("DoH 响应解析失败")?;
        Ok(parsed.answer)
    }

    /// 在所有端点上依次尝试，任一成功即返回；每个端点最多重试 2 次。
    async fn query_all(&self, host: &str, rtype: &str) -> Result<Vec<Entry>> {
        let mut last = None;
        for ep in &self.endpoints {
            for _ in 0..2 {
                match self.query_once(ep, host, rtype).await {
                    Ok(entries) => return Ok(entries),
                    Err(e) => {
                        eprintln!("DoH {rtype} {host} via {ep} 失败: {e}");
                        last = Some(e);
                    }
                }
            }
        }
        Err(last.unwrap_or_else(|| anyhow!("所有 DoH 端点均失败")))
    }

    pub async fn resolve(&self, host: &str) -> Result<Resolved> {
        let a_entries = self.query_all(host, "A").await?;
        let mut ips = Vec::new();
        for e in a_entries {
            if e.rtype == 1 {
                if let Ok(ip) = e.data.trim().parse::<IpAddr>() {
                    ips.push(ip);
                }
            }
        }
        let https = self.query_all(host, "HTTPS").await;
        let mut ech_list = None;
        if let Ok(entries) = https {
            for e in entries {
                if e.rtype == 65 {
                    if let Some(b64) = extract_ech_field(&e.data) {
                        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
                            ech_list = Some(bytes);
                            break;
                        }
                    }
                }
            }
        }
        if ips.is_empty() {
            return Err(anyhow!("{host} 未解析到可用 IP"));
        }
        Ok(Resolved { ips, ech_list })
    }
}

/// 从 SVCB/HTTPS 展示文本中提取 ech="..." 的 base64 内容。
fn extract_ech_field(data: &str) -> Option<&str> {
    let key = "ech=\"";
    let start = data.find(key)? + key.len();
    let rest = &data[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}
