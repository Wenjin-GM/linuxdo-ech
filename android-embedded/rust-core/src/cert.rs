//! 本地 MITM 证书颁发机构。
//! 仅在 App 内通过 network_security_config 信任，不写入系统证书库，无需 root。

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, KeyPair, KeyUsage};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::sync::RwLock;

pub struct LocalCa {
    ca_cert: rcgen::Certificate,
    ca_key: KeyPair,
    leaf_cache: RwLock<HashMap<String, Arc<rustls::sign::CertifiedKey>>>,
}

impl LocalCa {
    pub fn new() -> Result<Self> {
        let ca_key = KeyPair::generate()?;
        let mut ca_params = CertificateParams::new(vec![])?;
        ca_params
            .distinguished_name
            .push(DnType::CommonName, "Linuxdo ECH Local CA");
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![KeyUsage::KeyCertSign, KeyUsage::CrlSign];
        let ca_cert = ca_params.self_signed(&ca_key)?;
        Ok(Self {
            ca_cert,
            ca_key,
            leaf_cache: RwLock::new(HashMap::new()),
        })
    }

    /// CA 证书 DER，供 Kotlin 端写入 App 信任列表。
    pub fn ca_der(&self) -> Vec<u8> {
        self.ca_cert.der().to_vec()
    }

    /// 为指定主机签发（或取缓存的）叶子证书，返回 rustls 可直接使用的 CertifiedKey。
    pub async fn certified_key_for(
        &self,
        host: &str,
    ) -> Result<Arc<rustls::sign::CertifiedKey>> {
        if let Some(v) = self.leaf_cache.read().await.get(host) {
            return Ok(v.clone());
        }
        let leaf_key = KeyPair::generate()?;
        let mut params = CertificateParams::new(vec![host.to_string()])?;
        params
            .distinguished_name
            .push(DnType::CommonName, host);
        let leaf_cert = params.signed_by(&leaf_key, &self.ca_cert, &self.ca_key)?;

        let key_der = PrivateKeyDer::try_from(leaf_key.serialize_der())?;
        let cert_der = CertificateDer::from(leaf_cert.der().to_vec());
        let signing_key = rustls::crypto::aws_lc_rs::sign::any_supported_type(&key_der)?;
        let certified = rustls::sign::CertifiedKey {
            cert: vec![cert_der],
            key: signing_key,
            ocsp: None,
        };
        let arc = Arc::new(certified);
        self.leaf_cache
            .write()
            .await
            .insert(host.to_string(), arc.clone());
        Ok(arc)
    }
}
