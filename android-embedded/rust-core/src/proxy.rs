//! 本地 MITM 代理：客户端侧用本地 CA 终止 TLS，服务器侧用 ECH 加密 SNI 回源 Cloudflare。
//! 精简为 HTTP/1.1 实现（新闻 App 的 latest.json 为普通 GET，已足够）；
//! 连接复用/HTTP2 可在此基础上扩展。

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use rustls::client::{EchConfig, EchMode};
use rustls::pki_types::{EchConfigListBytes, ServerName};
use rustls::{ClientConfig, ServerConfig};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsConnector;

use crate::cert::LocalCa;
use crate::doh::{Doh, Resolved};

pub struct Proxy {
    pub ca: Arc<LocalCa>,
    pub doh: Arc<Doh>,
}

impl Proxy {
    pub async fn run(self: Arc<Self>, port: u16) -> Result<()> {
        let listener = TcpListener::bind(("127.0.0.1", port)).await?;
        loop {
            let (socket, _) = listener.accept().await?;
            let proxy = self.clone();
            tokio::spawn(async move {
                if let Err(e) = proxy.handle(socket).await {
                    eprintln!("连接处理失败: {e}");
                }
            });
        }
    }

    async fn handle(&self, socket: TcpStream) -> Result<()> {
        let mut reader = BufReader::new(socket);
        let mut first = String::new();
        reader.read_line(&mut first).await?;
        // 读完剩余请求头
        loop {
            let mut h = String::new();
            let n = reader.read_line(&mut h).await?;
            if n == 0 || h.trim().is_empty() {
                break;
            }
        }

        let parts: Vec<&str> = first.split_whitespace().collect();
        if parts.len() >= 3 && parts[0].eq_ignore_ascii_case("CONNECT") {
            let target = parts[1].to_string();
            let (host, port) = split_host_port(&target, 443);
            // 建立隧道
            use tokio::io::AsyncWriteExt;
            reader
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await?;

            let certified = self.ca.certified_key_for(&host).await?;
            let resolver = Arc::new(StaticCert(certified));
            let mut server = ServerConfig::builder()
                .with_no_client_auth()
                .with_cert_resolver(resolver);
            server.alpn_protocols = vec![b"http/1.1".to_vec()];
            let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server));
            let tls = acceptor.accept(reader).await.context("本地 MITM TLS 接受失败")?;

            let io = TokioIo::new(tls);
            let doh = self.doh.clone();
            let svc = hyper::service::service_fn(move |req| {
                let doh = doh.clone();
                let host = host.clone();
                let port = port;
                async move { forward(req, host, port, doh).await }
            });
            if let Err(e) = hyper::server::conn::http1::Builder::new()
                .keep_alive(true)
                .serve_connection(io, svc)
                .await
            {
                eprintln!("h1 服务连接结束: {e}");
            }
        } else {
            // 明文 HTTP（新闻 App 一般用不到，留作兜底）
            let io = TokioIo::new(reader);
            let doh = self.doh.clone();
            let svc = hyper::service::service_fn(move |req| {
                let doh = doh.clone();
                async move { forward(req, "linux.do".to_string(), 80, doh).await }
            });
            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, svc)
                .await;
        }
        Ok(())
    }
}

async fn forward(
    req: Request<Incoming>,
    host: String,
    port: u16,
    doh: Arc<Doh>,
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    match forward_inner(req, host, port, doh).await {
        Ok(r) => Ok(r),
        Err(e) => {
            eprintln!("转发失败: {e}");
            Ok(Response::builder()
                .status(502)
                .body(Full::new(Bytes::from(format!("ECH proxy error: {e}"))))
                .unwrap())
        }
    }
}

async fn forward_inner(
    req: Request<Incoming>,
    host: String,
    port: u16,
    doh: Arc<Doh>,
) -> Result<Response<Full<Bytes>>> {
    let (parts, body) = req.into_parts();
    let body_bytes = body.collect().await?.to_bytes();

    let resolved: Resolved = doh.resolve(&host).await?;
    let mut last_err = None;
    for ip in &resolved.ips {
        match send_once(
            SocketAddr::new(*ip, port),
            &host,
            &resolved.ech_list,
            &parts,
            &body_bytes,
        )
        .await
        {
            Ok(resp) => return Ok(resp),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("无可用上游")))
}

async fn send_once(
    addr: SocketAddr,
    host: &str,
    ech_list: &Option<Vec<u8>>,
    parts: &hyper::http::request::Parts,
    body: &Bytes,
) -> Result<Response<Full<Bytes>>> {
    let tcp = TcpStream::connect(addr)
        .await
        .with_context(|| format!("TCP 连接 {addr} 失败"))?;
    let config = build_ech_client(ech_list)?;
    let tls = TlsConnector::from(Arc::new(config))
        .connect(ServerName::try_from(host.to_string())?, tcp)
        .await
        .context("ECH TLS 握手失败")?;
    let io = TokioIo::new(tls);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
        .await
        .context("上游 h1 握手失败")?;
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let mut builder = Request::builder()
        .method(parts.method.clone())
        .uri(parts.uri.clone())
        .version(parts.version);
    for (k, v) in &parts.headers {
        builder = builder.header(k, v);
    }
    let outgoing = builder.body(Full::new(body.clone()))?;
    let resp = sender.send_request(outgoing).await?;
    let (rparts, rbody) = resp.into_parts();
    let bytes = rbody.collect().await?.to_bytes();
    Ok(Response::from_parts(rparts, Full::new(bytes)))
}

fn build_ech_client(ech_list: &Option<Vec<u8>>) -> Result<ClientConfig> {
    let mut root = rustls::RootCertStore::empty();
    root.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let mut builder = ClientConfig::builder_with_provider(
        rustls::crypto::aws_lc_rs::default_provider().into(),
    )
    .with_safe_default_protocol_versions()?;

    let config = if let Some(bytes) = ech_list {
        let ech = EchConfig::new(
            EchConfigListBytes::from(bytes.clone()),
            rustls::crypto::aws_lc_rs::hpke::ALL_SUPPORTED_SUITES,
        )?;
        builder
            .with_ech(EchMode::Enable(ech))?
            .with_root_certificates(root)
            .with_no_client_auth()
    } else {
        builder.with_root_certificates(root).with_no_client_auth()
    };
    let mut config = config;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(config)
}

fn split_host_port(target: &str, default_port: u16) -> (String, u16) {
    if let Some(idx) = target.rfind(':') {
        let host = &target[..idx];
        let port = target[idx + 1..].parse().unwrap_or(default_port);
        (host.to_string(), port)
    } else {
        (target.to_string(), default_port)
    }
}

/// 固定证书解析器：每个 MITM 连接已知主机，直接返回对应叶子证书。
struct StaticCert(Arc<rustls::sign::CertifiedKey>);
impl rustls::server::ResolvesServerCert for StaticCert {
    fn resolve(
        &self,
        _root_hint: Option<&[rustls::pki_types::TrustAnchor]>,
        _sigschemes: &[rustls::SignatureScheme],
        _sni: Option<&str>,
        _alpn: Option<&[u8]>,
        _ocsp: rustls::server::OcspStapling,
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        Some(self.0.clone())
    }
}
