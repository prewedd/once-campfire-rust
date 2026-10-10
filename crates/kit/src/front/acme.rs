//! Automatic certificates, as Thruster gets them from `golang.org/x/crypto/acme/autocert`
//! (`internal/server.go` `certManager`): on the first TLS handshake for a TLS_DOMAIN name, an
//! ECDSA P-256 certificate is ordered from ACME_DIRECTORY (Let's Encrypt), answering TLS-ALPN-01
//! on the HTTPS port, or HTTP-01 on the HTTP port when that fails; it's renewed 30 days before
//! it expires.
//!
//! Certificates and the account key live in STORAGE_PATH (`./storage/thruster`) in autocert's
//! `DirCache` layout, so an install that ran Thruster keeps its certificates and account:
//! `<domain>` holds the PEM private key followed by the PEM certificate chain, and
//! `acme_account+key` the account's PEM private key.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime};

use instant_acme::{
    Account, AuthorizationStatus, ChallengeType, ExternalAccountKey, Identifier, Key, NewAccount, NewOrder, OrderStatus, RetryPolicy,
};
use rustls::crypto::ring::sign::any_supported_type;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, PrivateSec1KeyDer};
use rustls::sign::CertifiedKey;

use super::config::FrontConfig;

const ACCOUNT_KEY: &str = "acme_account+key";
/// `autocert.Manager.RenewBefore` default.
const RENEW_BEFORE: Duration = Duration::from_secs(30 * 24 * 3600);
/// autocert's `renewJitter`.
const RENEW_JITTER: Duration = Duration::from_secs(3600);
/// autocert's timeout for obtaining a certificate during a handshake.
const ISSUE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

pub type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone)]
pub struct AcmeOptions {
    pub directory_url: String,
    /// `EAB_KID` and the decoded `EAB_HMAC_KEY`.
    pub external_account: Option<(String, Vec<u8>)>,
    pub storage_path: PathBuf,
    pub domains: Vec<String>,
    /// Challenge types in order of preference (autocert: TLS-ALPN-01, then HTTP-01).
    pub challenge_types: Vec<ChallengeType>,
    /// A root certificate (PEM file) to trust for the ACME directory, for test CAs.
    pub directory_root: Option<PathBuf>,
}

impl AcmeOptions {
    pub fn from_config(config: &FrontConfig) -> Self {
        Self {
            directory_url: config.acme_directory_url.clone(),
            external_account: external_account(&config.eab_kid, &config.eab_hmac_key),
            storage_path: config.storage_path.clone(),
            domains: config.tls_domains.clone(),
            challenge_types: vec![ChallengeType::TlsAlpn01, ChallengeType::Http01],
            directory_root: None,
        }
    }
}

/// `externalAccountBinding`: both set, and the key is unpadded URL-safe base64.
fn external_account(kid: &str, hmac_key: &str) -> Option<(String, Vec<u8>)> {
    use base64::Engine;
    if kid.is_empty() || hmac_key.is_empty() {
        return None;
    }
    match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(hmac_key) {
        Ok(key) => Some((kid.to_string(), key)),
        Err(error) => {
            tracing::error!(%error, "Error decoding EAB_HMACKey");
            None
        }
    }
}

pub struct CertManager {
    options: AcmeOptions,
    /// `HostWhitelist`: the TLS_DOMAIN names, lowercased and in ASCII.
    allowed: HashSet<String>,
    certificates: RwLock<HashMap<String, Arc<CertifiedKey>>>,
    /// TLS-ALPN-01 challenge certificates by domain.
    challenge_certificates: RwLock<HashMap<String, Arc<CertifiedKey>>>,
    /// HTTP-01 responses by request path.
    http_tokens: RwLock<HashMap<String, String>>,
    obtaining: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    renewing: Mutex<HashSet<String>>,
    account: tokio::sync::Mutex<Option<Account>>,
}

impl CertManager {
    pub fn new(options: AcmeOptions) -> Arc<Self> {
        let allowed = options.domains.iter().filter_map(|d| to_ascii(d)).collect();
        Arc::new(Self {
            options,
            allowed,
            certificates: RwLock::default(),
            challenge_certificates: RwLock::default(),
            http_tokens: RwLock::default(),
            obtaining: Mutex::default(),
            renewing: Mutex::default(),
            account: tokio::sync::Mutex::new(None),
        })
    }

    /// `HostWhitelist(domains...)(ctx, host)`
    pub fn host_allowed(&self, host: &str) -> bool {
        self.allowed.contains(host)
    }

    /// The certificate for a TLS handshake's server name (`GetCertificate`), obtaining one if
    /// there's none in memory or in the cache.
    pub async fn certificate(self: &Arc<Self>, server_name: Option<&str>) -> Result<Arc<CertifiedKey>, Error> {
        let name = server_name_to_domain(server_name)?;
        if let Some(certificate) = self.loaded(&name) {
            return Ok(certificate);
        }
        // A cached certificate is used even for a name no longer in TLS_DOMAIN, as autocert does.
        // Any other name is turned away here, before it costs a task or a place in `obtaining`.
        if !self.host_allowed(&name) && !self.has_cache_file(&name).await {
            return Err(format!("acme/autocert: host {name:?} not configured in HostWhitelist").into());
        }
        // In a task of its own, so a client that gives up doesn't abandon the order.
        let this = self.clone();
        let task = tokio::spawn(async move { this.obtain(name).await });
        match tokio::time::timeout(ISSUE_TIMEOUT, task).await {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => Err(error.into()),
            Err(_) => Err("acme/autocert: timed out obtaining a certificate".into()),
        }
    }

    /// The certificate a TLS-ALPN-01 validation handshake for `server_name` gets.
    pub fn challenge_certificate(&self, server_name: Option<&str>) -> Option<Arc<CertifiedKey>> {
        let name = server_name_to_domain(server_name).ok()?;
        self.challenge_certificates.read().unwrap().get(&name).cloned()
    }

    /// The loaded certificate for a normalized name, for the TLS resolver.
    pub fn loaded(&self, name: &str) -> Option<Arc<CertifiedKey>> {
        self.certificates.read().unwrap().get(name).cloned()
    }

    /// The HTTP-01 response for a `/.well-known/acme-challenge/` path.
    pub fn http_token(&self, path: &str) -> Option<String> {
        self.http_tokens.read().unwrap().get(path).cloned()
    }

    /// Obtains the certificate for `name` once, however many handshakes ask for it meanwhile.
    async fn obtain(self: Arc<Self>, name: String) -> Result<Arc<CertifiedKey>, Error> {
        let lock = self.obtaining.lock().unwrap().entry(name.clone()).or_default().clone();
        let result = {
            let _obtaining = lock.lock().await;
            self.obtain_now(&name).await
        };
        self.finish_obtaining(&name, lock);
        result
    }

    /// Forgets `name`'s lock unless another `obtain` holds it too. Clones are only taken with
    /// `obtaining` locked, so under that lock a count of two (the map's and ours) is final.
    fn finish_obtaining(&self, name: &str, lock: Arc<tokio::sync::Mutex<()>>) {
        let mut obtaining = self.obtaining.lock().unwrap();
        if obtaining.get(name).is_some_and(|current| Arc::ptr_eq(current, &lock)) && Arc::strong_count(&lock) == 2 {
            obtaining.remove(name);
        }
    }

    async fn obtain_now(self: &Arc<Self>, name: &str) -> Result<Arc<CertifiedKey>, Error> {
        if let Some(certificate) = self.loaded(name) {
            return Ok(certificate);
        }
        if let Some((certificate, not_after)) = self.read_cached(name).await {
            self.install(name, certificate.clone(), not_after);
            return Ok(certificate);
        }
        if !self.host_allowed(name) {
            return Err(format!("acme/autocert: host {name:?} not configured in HostWhitelist").into());
        }
        let (certificate, not_after) = self.issue(name).await?;
        self.install(name, certificate.clone(), not_after);
        Ok(certificate)
    }

    fn install(self: &Arc<Self>, name: &str, certificate: Arc<CertifiedKey>, not_after: SystemTime) {
        self.certificates.write().unwrap().insert(name.to_string(), certificate);
        if self.renewing.lock().unwrap().insert(name.to_string()) {
            let this = self.clone();
            let name = name.to_string();
            tokio::spawn(async move { this.renew_forever(name, not_after).await });
        }
    }

    /// autocert's `domainRenewal`: renew `RenewBefore` (minus jitter) ahead of expiry; after a
    /// failure, try again in 30 to 60 minutes.
    async fn renew_forever(self: Arc<Self>, name: String, mut not_after: SystemTime) {
        loop {
            let jitter = RENEW_JITTER.mul_f64(rand::random::<f64>());
            let wait = not_after.duration_since(SystemTime::now()).unwrap_or_default().saturating_sub(RENEW_BEFORE).saturating_sub(jitter);
            tokio::time::sleep(wait).await;
            match self.issue(&name).await {
                Ok((certificate, expires)) => {
                    self.certificates.write().unwrap().insert(name.clone(), certificate);
                    not_after = expires;
                }
                Err(error) => {
                    tracing::error!(domain = %name, %error, "TLS: certificate renewal failed");
                    let half = RENEW_JITTER / 2;
                    tokio::time::sleep(half + half.mul_f64(rand::random::<f64>())).await;
                }
            }
        }
    }

    async fn has_cache_file(&self, name: &str) -> bool {
        tokio::fs::try_exists(self.options.storage_path.join(name)).await.unwrap_or(false)
    }

    /// `cacheGet`: a cached certificate that's current, covers `name` and matches its key.
    async fn read_cached(&self, name: &str) -> Option<(Arc<CertifiedKey>, SystemTime)> {
        let pem = tokio::fs::read(self.options.storage_path.join(name)).await.ok()?;
        match parse_cached(&pem, name) {
            Ok(parsed) => Some(parsed),
            Err(error) => {
                tracing::info!(domain = %name, %error, "TLS: ignoring cached certificate");
                None
            }
        }
    }

    /// Orders a certificate, trying each challenge type in turn (`verifiedOrder`).
    async fn issue(&self, name: &str) -> Result<(Arc<CertifiedKey>, SystemTime), Error> {
        let account = self.account().await?;
        let mut last_error: Error = "no supported challenge type".into();
        for challenge_type in &self.options.challenge_types {
            match self.order(&account, name, challenge_type.clone()).await {
                Ok(issued) => {
                    tracing::info!(domain = %name, "TLS: obtained certificate");
                    return Ok(issued);
                }
                Err(error) => {
                    tracing::info!(domain = %name, challenge = ?challenge_type, %error, "TLS: order failed");
                    last_error = error;
                }
            }
        }
        Err(last_error)
    }

    async fn order(&self, account: &Account, name: &str, challenge_type: ChallengeType) -> Result<(Arc<CertifiedKey>, SystemTime), Error> {
        let identifiers = [Identifier::Dns(name.to_string())];
        let mut order = account.new_order(&NewOrder::new(&identifiers)).await?;
        let mut provisioned = Provisioned { manager: self, http_paths: Vec::new(), alpn_domains: Vec::new() };
        {
            let mut authorizations = order.authorizations();
            while let Some(authorization) = authorizations.next().await {
                let mut authorization = authorization?;
                match authorization.status {
                    AuthorizationStatus::Valid => continue,
                    AuthorizationStatus::Pending => {}
                    status => return Err(format!("authorization is {status:?}").into()),
                }
                let domain = authorization.identifier().to_string();
                let mut challenge = authorization.challenge(challenge_type.clone()).ok_or("challenge type not offered")?;
                let key_authorization = challenge.key_authorization();
                match challenge_type {
                    ChallengeType::TlsAlpn01 => {
                        let certificate = challenge_certificate(&domain, key_authorization.digest().as_ref())?;
                        self.challenge_certificates.write().unwrap().insert(domain.clone(), certificate);
                        provisioned.alpn_domains.push(domain);
                    }
                    ChallengeType::Http01 => {
                        let path = format!("/.well-known/acme-challenge/{}", challenge.token);
                        self.http_tokens.write().unwrap().insert(path.clone(), key_authorization.as_str().to_string());
                        provisioned.http_paths.push(path);
                    }
                    _ => return Err("unsupported challenge type".into()),
                }
                challenge.set_ready().await?;
            }
        }
        let status = order.poll_ready(&RetryPolicy::new().timeout(Duration::from_secs(120))).await?;
        drop(provisioned);
        if status != OrderStatus::Ready {
            return Err(format!("order is {status:?}").into());
        }

        let key = rcgen::KeyPair::generate()?;
        let mut params = rcgen::CertificateParams::new(vec![name.to_string()])?;
        params.distinguished_name = rcgen::DistinguishedName::new();
        params.distinguished_name.push(rcgen::DnType::CommonName, name);
        let csr = params.serialize_request(&key)?;
        order.finalize_csr(csr.der()).await?;
        let chain = order.poll_certificate(&RetryPolicy::new().timeout(Duration::from_secs(120))).await?;

        let pem = cache_entry(&key.serialize_der(), &chain)?;
        let parsed = parse_cached(pem.as_bytes(), name)?;
        self.write_cache_file(name, pem.into_bytes()).await?;
        Ok(parsed)
    }

    /// The ACME account, registered with the cached account key (or a new one, cached first).
    async fn account(&self) -> Result<Account, Error> {
        let mut account = self.account.lock().await;
        if let Some(account) = account.as_ref() {
            return Ok(account.clone());
        }
        let builder = || match &self.options.directory_root {
            Some(root) => Account::builder_with_root(root),
            None => Account::builder(),
        };
        let directory = self.options.directory_url.clone();
        let key_path = self.options.storage_path.join(ACCOUNT_KEY);
        let registered = match tokio::fs::read(&key_path).await.ok() {
            Some(pem) => {
                let pkcs8 = parse_private_key(&pem)?;
                let key = Key::from_pkcs8_der(pkcs8.clone_key())?;
                builder()?.create_from_key((key, PrivateKeyDer::Pkcs8(pkcs8)), directory).await?.0
            }
            None => match &self.options.external_account {
                Some((kid, hmac)) => {
                    let new_account = NewAccount { contact: &[], terms_of_service_agreed: true, only_return_existing: false };
                    let (account, credentials) =
                        builder()?.create(&new_account, directory, Some(&ExternalAccountKey::new(kid.clone(), hmac))).await?;
                    self.write_cache_file(ACCOUNT_KEY, private_key_pem(credentials.private_key().secret_pkcs8_der())?.into_bytes()).await?;
                    account
                }
                None => {
                    let (key, pkcs8) = Key::generate_pkcs8()?;
                    self.write_cache_file(ACCOUNT_KEY, private_key_pem(pkcs8.secret_pkcs8_der())?.into_bytes()).await?;
                    builder()?.create_from_key((key, PrivateKeyDer::Pkcs8(pkcs8)), directory).await?.0
                }
            },
        };
        *account = Some(registered.clone());
        Ok(registered)
    }

    async fn write_cache_file(&self, name: &str, data: Vec<u8>) -> Result<(), Error> {
        let (dir, name) = (self.options.storage_path.clone(), name.to_string());
        tokio::task::spawn_blocking(move || write_cache_file(&dir, &name, &data)).await?
    }
}

/// Removes an order's challenge responses once it's done with them.
struct Provisioned<'a> {
    manager: &'a CertManager,
    http_paths: Vec<String>,
    alpn_domains: Vec<String>,
}

impl Drop for Provisioned<'_> {
    fn drop(&mut self) {
        let mut tokens = self.manager.http_tokens.write().unwrap();
        for path in &self.http_paths {
            tokens.remove(path);
        }
        let mut certificates = self.manager.challenge_certificates.write().unwrap();
        for domain in &self.alpn_domains {
            certificates.remove(domain);
        }
    }
}

/// autocert's server name checks and normalization.
fn server_name_to_domain(server_name: Option<&str>) -> Result<String, Error> {
    let name = server_name.unwrap_or("");
    if name.is_empty() {
        return Err("acme/autocert: missing server name".into());
    }
    if !name.trim_matches('.').contains('.') {
        return Err("acme/autocert: server name component count invalid".into());
    }
    if name.contains(['/', '\\']) {
        return Err("acme/autocert: server name contains invalid character".into());
    }
    to_ascii(name.trim_end_matches('.')).ok_or_else(|| "acme/autocert: invalid server name".into())
}

/// `idna.Lookup.ToASCII`
pub fn to_ascii(host: &str) -> Option<String> {
    match url::Host::parse(host).ok()? {
        url::Host::Domain(domain) => Some(domain),
        _ => None,
    }
}

/// A TLS-ALPN-01 challenge certificate (RFC 8737).
fn challenge_certificate(domain: &str, digest: &[u8]) -> Result<Arc<CertifiedKey>, Error> {
    let key = rcgen::KeyPair::generate()?;
    let mut params = rcgen::CertificateParams::new(vec![domain.to_string()])?;
    params.custom_extensions = vec![rcgen::CustomExtension::new_acme_identifier(digest)];
    let certificate = params.self_signed(&key)?;
    let signing_key = any_supported_type(&PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der())))?;
    Ok(Arc::new(CertifiedKey::new(vec![certificate.der().clone()], signing_key)))
}

/// A cache file's contents: the private key (as autocert writes it, SEC1 "EC PRIVATE KEY"), then
/// the chain.
fn cache_entry(pkcs8: &[u8], chain_pem: &str) -> Result<String, Error> {
    let mut pem = private_key_pem(pkcs8)?;
    for certificate in CertificateDer::pem_slice_iter(chain_pem.as_bytes()) {
        pem.push_str(&pem_block("CERTIFICATE", certificate?.as_ref()));
    }
    Ok(pem)
}

/// A P-256 key as autocert writes it (`x509.MarshalECPrivateKey`): SEC1 with the curve named,
/// which Go needs to read it back (`ParseECPrivateKey`).
fn private_key_pem(pkcs8: &[u8]) -> Result<String, Error> {
    use p256::elliptic_curve::sec1::ToEncodedPoint;
    use p256::pkcs8::DecodePrivateKey;
    let key = p256::SecretKey::from_pkcs8_der(pkcs8).map_err(|e| format!("private key: {e}"))?;
    let public = key.public_key().to_encoded_point(false);
    let mut der = vec![0x30, 0x77, 0x02, 0x01, 0x01, 0x04, 0x20];
    der.extend_from_slice(&key.to_bytes());
    // [0] namedCurve prime256v1
    der.extend_from_slice(&[0xa0, 0x0a, 0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07]);
    // [1] publicKey BIT STRING, uncompressed point
    der.extend_from_slice(&[0xa1, 0x44, 0x03, 0x42, 0x00]);
    der.extend_from_slice(public.as_bytes());
    Ok(pem_block("EC PRIVATE KEY", &der))
}

/// The first private key in a cache file, as PKCS#8 (autocert's `parsePrivateKey` accepts SEC1
/// EC keys and PKCS#8).
fn parse_private_key(pem: &[u8]) -> Result<PrivatePkcs8KeyDer<'static>, Error> {
    use p256::pkcs8::EncodePrivateKey;
    match PrivateKeyDer::from_pem_slice(pem)? {
        PrivateKeyDer::Pkcs8(key) => Ok(key),
        PrivateKeyDer::Sec1(key) => {
            let key = p256::SecretKey::from_sec1_der(key.secret_sec1_der()).map_err(|e| format!("private key: {e}"))?;
            let der = key.to_pkcs8_der().map_err(|e| format!("private key: {e}"))?;
            Ok(PrivatePkcs8KeyDer::from(der.as_bytes().to_vec()))
        }
        _ => Err("unsupported private key type".into()),
    }
}

/// Parses and checks a cache file for `domain` (autocert's `validCert`): its leaf covers the
/// domain, is current, and matches the key.
fn parse_cached(pem: &[u8], domain: &str) -> Result<(Arc<CertifiedKey>, SystemTime), Error> {
    let key = match PrivateKeyDer::from_pem_slice(pem)? {
        PrivateKeyDer::Sec1(key) => PrivateKeyDer::Sec1(PrivateSec1KeyDer::from(key.secret_sec1_der().to_vec())),
        key => key,
    };
    let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(pem).collect::<Result<_, _>>()?;
    let leaf = chain.first().ok_or("no certificate")?;
    let (_, parsed) = x509_parser::parse_x509_certificate(leaf.as_ref()).map_err(|e| format!("certificate: {e}"))?;
    let now = x509_parser::time::ASN1Time::now();
    if now < parsed.validity().not_before || now > parsed.validity().not_after {
        return Err("certificate is expired or not yet valid".into());
    }
    if !covers(&parsed, domain) {
        return Err(format!("certificate is not valid for {domain}").into());
    }
    let not_after = SystemTime::UNIX_EPOCH + Duration::from_secs(parsed.validity().not_after.timestamp().max(0) as u64);
    let certified = CertifiedKey::new(chain.clone(), any_supported_type(&key)?);
    certified.keys_match()?;
    Ok((Arc::new(certified), not_after))
}

/// `x509.Certificate.VerifyHostname` for DNS names, including wildcards.
fn covers(certificate: &x509_parser::certificate::X509Certificate<'_>, domain: &str) -> bool {
    let Ok(Some(names)) = certificate.subject_alternative_name() else { return false };
    names.value.general_names.iter().any(|name| match name {
        x509_parser::extensions::GeneralName::DNSName(pattern) => {
            let pattern = pattern.to_ascii_lowercase();
            match pattern.strip_prefix("*.") {
                Some(suffix) => domain.split_once('.').is_some_and(|(_, rest)| rest == suffix),
                None => pattern == domain,
            }
        }
        _ => false,
    })
}

fn pem_block(label: &str, der: &[u8]) -> String {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(der);
    let mut pem = format!("-----BEGIN {label}-----\n");
    for line in encoded.as_bytes().chunks(64) {
        pem.push_str(std::str::from_utf8(line).unwrap());
        pem.push('\n');
    }
    pem.push_str(&format!("-----END {label}-----\n"));
    pem
}

/// `DirCache.Put`: the directory 0700, the file 0600, written through a temporary file.
fn write_cache_file(dir: &Path, name: &str, data: &[u8]) -> Result<(), Error> {
    #[cfg(unix)]
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(dir)?;
    let temporary = dir.join(format!("{name}.tmp{}", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&temporary)?;
    file.write_all(data)?;
    file.sync_all()?;
    std::fs::rename(&temporary, dir.join(name))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn self_signed(names: &[&str]) -> (String, rcgen::KeyPair) {
        let key = rcgen::KeyPair::generate().unwrap();
        let params = rcgen::CertificateParams::new(names.iter().map(|n| n.to_string()).collect::<Vec<_>>()).unwrap();
        let certificate = params.self_signed(&key).unwrap();
        (certificate.pem(), key)
    }

    #[test]
    fn server_names_are_normalized_and_checked() {
        assert_eq!(server_name_to_domain(Some("Chat.Example.COM.")).unwrap(), "chat.example.com");
        assert!(server_name_to_domain(None).is_err());
        assert!(server_name_to_domain(Some("localhost")).is_err());
        assert!(server_name_to_domain(Some("a/b.com")).is_err());
        assert_eq!(server_name_to_domain(Some("bücher.example")).unwrap(), "xn--bcher-kva.example");
    }

    #[test]
    fn cache_entries_round_trip_in_autocert_format() {
        let (chain, key) = self_signed(&["chat.example.com"]);
        let pem = cache_entry(&key.serialize_der(), &chain).unwrap();
        assert!(pem.starts_with("-----BEGIN EC PRIVATE KEY-----\n"));
        assert!(pem.contains("-----BEGIN CERTIFICATE-----\n"));
        let (certified, not_after) = parse_cached(pem.as_bytes(), "chat.example.com").unwrap();
        assert_eq!(certified.cert.len(), 1);
        assert!(not_after > SystemTime::now());
        assert!(parse_cached(pem.as_bytes(), "other.example.com").is_err());
        let wildcard = cache_entry(&key.serialize_der(), &self_signed(&["*.example.com"]).0);
        assert!(parse_cached(wildcard.unwrap().as_bytes(), "chat.example.com").is_err(), "a key that doesn't match its certificate");
    }

    #[test]
    fn reads_sec1_keys_as_go_and_openssl_write_them() {
        // `openssl ecparam -name prime256v1 -genkey -noout`: the form autocert writes.
        let sec1 = "-----BEGIN EC PRIVATE KEY-----\nMHcCAQEEIG5OGupNC0FxH3AXSkAGwIJDK3cGjGtRRWYgHs4ovnTzoAoGCCqGSM49\nAwEHoUQDQgAEx4Ph0lvWp/mtK6ehtSouzqyPEbXzgtcp59tMiQeiHuF+lPGyklEW\nOt6HbebraKMCA2Nm3v6HWJwkCMUTklFv5Q==\n-----END EC PRIVATE KEY-----\n";
        let pkcs8 = parse_private_key(sec1.as_bytes()).unwrap();
        assert!(Key::from_pkcs8_der(pkcs8.clone_key()).is_ok());
        // And writes them back byte for byte (Go can't read SEC1 keys without the curve named).
        assert_eq!(private_key_pem(pkcs8.secret_pkcs8_der()).unwrap(), sec1);
    }

    #[test]
    fn writes_cache_files_privately() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("thruster");
        write_cache_file(&path, "chat.example.com", b"data").unwrap();
        assert_eq!(std::fs::read(path.join("chat.example.com")).unwrap(), b"data");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(path.join("chat.example.com")).unwrap().permissions().mode() & 0o777, 0o600);
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o700);
        }
    }

    fn manager(storage: &Path) -> Arc<CertManager> {
        let options = AcmeOptions {
            directory_url: "https://acme.invalid/directory".into(),
            external_account: None,
            storage_path: storage.to_path_buf(),
            domains: vec!["chat.example.com".into()],
            challenge_types: vec![ChallengeType::TlsAlpn01],
            directory_root: None,
        };
        CertManager::new(options)
    }

    #[tokio::test]
    async fn names_outside_tls_domain_leave_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let certs = manager(dir.path());
        for n in 0..100 {
            assert!(certs.certificate(Some(&format!("x{n}.example.com"))).await.is_err());
        }
        assert!(certs.obtaining.lock().unwrap().is_empty());
        assert!(certs.certificates.read().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cached_certificates_load_and_are_forgotten_by_obtaining() {
        let dir = tempfile::tempdir().unwrap();
        let (chain, key) = self_signed(&["chat.example.com", "old.example.com"]);
        let pem = cache_entry(&key.serialize_der(), &chain).unwrap();
        write_cache_file(dir.path(), "chat.example.com", pem.as_bytes()).unwrap();
        write_cache_file(dir.path(), "old.example.com", pem.as_bytes()).unwrap();
        let certs = manager(dir.path());
        assert!(certs.certificate(Some("chat.example.com")).await.is_ok());
        // No longer in TLS_DOMAIN, but cached: served, as autocert does.
        assert!(certs.certificate(Some("old.example.com")).await.is_ok());
        assert!(certs.loaded("old.example.com").is_some());
        assert!(certs.obtaining.lock().unwrap().is_empty());
    }

    #[test]
    fn external_account_keys_are_url_safe_base64() {
        assert_eq!(external_account("kid", "aGVsbG8"), Some(("kid".into(), b"hello".to_vec())));
        assert_eq!(external_account("", "aGVsbG8"), None);
        assert_eq!(external_account("kid", "not base64!"), None);
    }
}
