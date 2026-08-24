//! Local CA management for the built-in proxy's HTTPS MITM.
//!
//! Resolution order:
//! 1. `<config>/mitm/ca.pem` + `ca-key.pem` if present (previously imported).
//! 2. Import the 2018 Charles Proxy CA from
//!    `~/Downloads/new-charles-ssl-proxying 1.p12` (password `intigral123`)
//!    using the system `openssl` CLI. Jawwy/Intigral TV builds trust this CA
//!    as a system certificate (`bed1ef8f.0`), so host certs signed with its
//!    private key are accepted by the production app.
//! 3. Generate a fresh self-signed RSA CA. NOTE: a generated CA will NOT
//!    decrypt the production TV app — the device will fail the TLS handshake
//!    (certificate_unknown) until this CA is installed in the TV system store.

use rand::rngs::OsRng;
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DnType, IsCa, KeyPair,
};
use rsa::pkcs8::EncodePrivateKey;
use rsa::RsaPrivateKey;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;

const DEFAULT_P12: &str = "new-charles-ssl-proxying 1.p12";
const DEFAULT_P12_PASSWORD: &str = "intigral123";

#[derive(Debug, Clone, PartialEq)]
pub enum CaSource {
    /// Imported from the user's Charles p12 — trusted by Jawwy/Intigral builds.
    Charles2018P12(PathBuf),
    /// Freshly generated — NOT trusted by production TV apps until installed.
    Generated(PathBuf),
}

impl CaSource {
    pub fn label(&self) -> &'static str {
        match self {
            CaSource::Charles2018P12(_) => "charles-2018-p12",
            CaSource::Generated(_) => "generated-untrusted",
        }
    }
}

pub struct MitmCa {
    pub source: CaSource,
    /// The real CA certificate in DER — appended after every leaf so the TV
    /// can chain up to its system trust store.
    ca_cert_der: Vec<u8>,
    /// RSA CA private key, wrapped for rcgen signing.
    ca_key_pair: Arc<KeyPair>,
    /// rcgen `Certificate` handle carrying the CA's subject/SKI, used only as
    /// the `issuer` argument when signing leaves.
    issuer_cert: Arc<Certificate>,
    leaf_cache: Mutex<HashMap<String, Option<Arc<ServerConfig>>>>,
}

impl MitmCa {
    /// Resolve a CA: cached PEMs → installer resources → p12 import → generate.
    ///
    /// `extra_dirs` is typically the Tauri resource dir (and the exe directory)
    /// so a Windows NSIS build can ship the 2018 CA without openssl or Downloads.
    pub fn resolve(extra_dirs: &[PathBuf]) -> Result<Arc<Self>, String> {
        let dir = ca_dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("create mitm dir: {e}"))?;

        if let Some(ca) = Self::load_dir(&dir) {
            return Ok(Arc::new(ca));
        }

        for bundled in bundled_mitm_dirs(extra_dirs) {
            if Self::load_dir(&bundled).is_some() {
                let _ = std::fs::copy(bundled.join("ca.pem"), dir.join("ca.pem"));
                let _ = std::fs::copy(bundled.join("ca-key.pem"), dir.join("ca-key.pem"));
                let src_marker = bundled.join("source");
                if src_marker.exists() {
                    let _ = std::fs::copy(&src_marker, dir.join("source"));
                }
                if let Some(ca) = Self::load_dir(&dir) {
                    println!(
                        "[mitm] loaded bundled CA from {} (source: {})",
                        bundled.display(),
                        ca.source.label()
                    );
                    return Ok(Arc::new(ca));
                }
            }
        }

        for p12 in p12_candidates() {
            if !p12.exists() {
                continue;
            }
            match Self::import_p12(&p12, DEFAULT_P12_PASSWORD, &dir) {
                Ok(()) => {
                    if let Some(ca) = Self::load_dir(&dir) {
                        println!(
                            "[mitm] imported Charles 2018 CA from {} (source: {})",
                            p12.display(),
                            ca.source.label()
                        );
                        return Ok(Arc::new(ca));
                    }
                    eprintln!("[mitm] p12 import produced unreadable CA files");
                }
                Err(e) => eprintln!("[mitm] p12 import failed from {}: {e}", p12.display()),
            }
        }
        let ca = Self::generate(&dir)?;
        println!(
            "[mitm] WARNING: using a GENERATED CA at {} — production TV apps will fail TLS (certificate_unknown) until it is installed on the device",
            dir.display()
        );
        Ok(Arc::new(ca))
    }

    fn load_dir(dir: &std::path::Path) -> Option<Self> {
        let cert_pem = std::fs::read_to_string(dir.join("ca.pem")).ok()?;
        let key_pem = std::fs::read_to_string(dir.join("ca-key.pem")).ok()?;
        let cert_der = first_pem_block(&cert_pem, "CERTIFICATE")?;
        let key_pair = Arc::new(parse_rsa_keypair(&key_pem)?);
        let generated = std::fs::read_to_string(dir.join("source"))
            .map(|s| s.trim() == "generated")
            .unwrap_or(false);
        let source = if generated {
            CaSource::Generated(dir.join("ca.pem"))
        } else {
            CaSource::Charles2018P12(dir.join("ca.pem"))
        };
        let issuer_cert = Arc::new(imported_issuer_cert(&cert_der, &key_pair)?);
        Some(Self {
            source,
            ca_cert_der: cert_der,
            ca_key_pair: key_pair,
            issuer_cert,
            leaf_cache: Mutex::new(HashMap::new()),
        })
    }

    /// Convert a PKCS#12 bundle to PEM CA cert + key.
    /// Prefers a pure-Rust parser (works on Windows with no openssl).
    fn import_p12(
        p12: &std::path::Path,
        password: &str,
        dir: &std::path::Path,
    ) -> Result<(), String> {
        match Self::import_p12_rust(p12, password, dir) {
            Ok(()) => return Ok(()),
            Err(e) => eprintln!("[mitm] rust p12 parse failed: {e}; trying openssl"),
        }
        Self::import_p12_openssl(p12, password, dir)
    }

    fn import_p12_rust(
        p12: &std::path::Path,
        password: &str,
        dir: &std::path::Path,
    ) -> Result<(), String> {
        let data = std::fs::read(p12).map_err(|e| format!("read p12: {e}"))?;
        let ks = p12_keystore::KeyStore::from_pkcs12(
            &data,
            password,
            p12_keystore::Pkcs12ImportPolicy::Relaxed,
        )
        .map_err(|e| format!("parse p12: {e}"))?;
        let (_, chain) = ks
            .private_key_chain()
            .ok_or_else(|| "p12 has no private key".to_string())?;
        let cert_der = chain
            .certs()
            .first()
            .ok_or_else(|| "p12 has no certificate".to_string())?
            .as_der();
        let key_der = chain.key().as_der();
        std::fs::write(dir.join("ca.pem"), pem_encode("CERTIFICATE", cert_der))
            .map_err(|e| e.to_string())?;
        std::fs::write(dir.join("ca-key.pem"), pem_encode("PRIVATE KEY", key_der))
            .map_err(|e| e.to_string())?;
        let _ = std::fs::write(dir.join("source"), "charles-2018-p12");
        Ok(())
    }

    fn import_p12_openssl(
        p12: &std::path::Path,
        password: &str,
        dir: &std::path::Path,
    ) -> Result<(), String> {
        for bin in openssl_bins() {
            let out_ca = dir.join("ca.pem");
            let out_key = dir.join("ca-key.pem");
            let ok = run_quiet(
                bin,
                &[
                    "pkcs12",
                    "-in",
                    &p12.to_string_lossy(),
                    "-passin",
                    &format!("pass:{password}"),
                    "-nodes",
                    "-nokeys",
                    "-out",
                    &out_ca.to_string_lossy(),
                ],
            )
            .and_then(|_| {
                run_quiet(
                    bin,
                    &[
                        "pkcs12",
                        "-in",
                        &p12.to_string_lossy(),
                        "-passin",
                        &format!("pass:{password}"),
                        "-nodes",
                        "-nocerts",
                        "-out",
                        &out_key.to_string_lossy(),
                    ],
                )
            });
            if ok.is_ok() {
                // Strip PKCS#12 "Bag Attributes" preamble before the PEM block.
                for f in [&out_ca, &out_key] {
                    if let Ok(text) = std::fs::read_to_string(f) {
                        let _ = std::fs::write(f, strip_before_pem(&text));
                    }
                }
                if std::fs::read_to_string(&out_ca)
                    .map(|t| first_pem_block(&t, "CERTIFICATE").is_some())
                    .unwrap_or(false)
                {
                    let _ = std::fs::write(dir.join("source"), "charles-2018-p12");
                    return Ok(());
                }
            }
        }
        Err("no usable openssl found to convert the p12".into())
    }

    /// Generate a self-signed RSA-2048 CA (fallback; untrusted by TV apps).
    pub(crate) fn generate(dir: &std::path::Path) -> Result<Self, String> {
        let key =
            RsaPrivateKey::new(&mut OsRng, 2048).map_err(|e| format!("generate CA key: {e}"))?;
        let key_der = key
            .to_pkcs8_der()
            .map_err(|e| format!("encode CA key: {e}"))?;
        std::fs::write(
            dir.join("ca-key.pem"),
            pem_encode("PRIVATE KEY", key_der.as_bytes()),
        )
        .map_err(|e| e.to_string())?;

        let key_pair = Arc::new(
            KeyPair::from_der_and_sign_algo(
                &PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(
                    key_der.as_bytes(),
                )),
                &rcgen::PKCS_RSA_SHA256,
            )
            .map_err(|e| format!("wrap CA key: {e}"))?,
        );

        let mut ca_params =
            CertificateParams::new(Vec::<String>::new()).map_err(|e| e.to_string())?;
        ca_params
            .distinguished_name
            .push(DnType::CommonName, "Android TV Remote MITM CA");
        ca_params
            .distinguished_name
            .push(DnType::OrganizationName, "androidtv-remote");
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca_cert = ca_params
            .self_signed(&*key_pair)
            .map_err(|e| format!("self-sign CA: {e}"))?;
        let ca_cert_der = ca_cert.der().to_vec();

        std::fs::write(dir.join("ca.pem"), ca_cert.pem()).map_err(|e| e.to_string())?;
        let _ = std::fs::write(dir.join("source"), "generated");

        Ok(Self {
            source: CaSource::Generated(dir.join("ca.pem")),
            ca_cert_der,
            ca_key_pair: key_pair,
            issuer_cert: Arc::new(ca_cert),
            leaf_cache: Mutex::new(HashMap::new()),
        })
    }

    /// Build (and cache) the rustls server config presenting a fresh leaf
    /// certificate for `host`, signed by this CA.
    pub fn server_config(self: &Arc<Self>, host: &str) -> Option<Arc<ServerConfig>> {
        if let Some(cached) = self.leaf_cache.lock().unwrap().get(host) {
            return cached.clone();
        }
        let config = self.build_leaf(host);
        self.leaf_cache
            .lock()
            .unwrap()
            .insert(host.to_string(), config.clone());
        config
    }

    fn build_leaf(&self, host: &str) -> Option<Arc<ServerConfig>> {
        let leaf_key = KeyPair::generate().ok()?;
        let mut params = CertificateParams::new(vec![host.to_string()]).ok()?;
        params.distinguished_name.push(DnType::CommonName, host);
        let now = time::OffsetDateTime::now_utc();
        params.not_before = now - time::Duration::hours(25);
        params.not_after = now + time::Duration::days(90);
        let cert = params
            .signed_by(&leaf_key, &self.issuer_cert, &self.ca_key_pair)
            .ok()?;

        // Present [leaf, real CA]. For an imported CA the TV already trusts
        // the original anchor, so chaining to our copy is belt-and-braces.
        let chain = vec![
            CertificateDer::from(cert.der().to_vec()),
            CertificateDer::from(self.ca_cert_der.clone()),
        ];
        let key_der = rustls::pki_types::PrivatePkcs8KeyDer::from(leaf_key.serialize_der());
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(chain, PrivateKeyDer::Pkcs8(key_der))
            .ok()?;
        Some(Arc::new(config))
    }
}

/// Build an rcgen issuer `Certificate` from an existing CA cert's DER. The
/// returned object carries the CA's subject and key identifier so issued
/// leaves chain correctly; the presented chain still uses the ORIGINAL CA
/// bytes (`ca_cert_der`), never this reconstruction.
fn imported_issuer_cert(cert_der: &[u8], key_pair: &KeyPair) -> Option<Certificate> {
    let mut params = CertificateParams::from_ca_cert_der(&CertificateDer::from(cert_der.to_vec()))
        .ok()?;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.self_signed(key_pair).ok()
}

fn parse_rsa_keypair(key_pem: &str) -> Option<KeyPair> {
    let mut reader = std::io::BufReader::new(key_pem.as_bytes());
    let key = rustls_pemfile::private_key(&mut reader).ok()??;
    KeyPair::from_der_and_sign_algo(&key, &rcgen::PKCS_RSA_SHA256).ok()
}

/// Return the base64-decoded contents of the first PEM block with `tag`.
fn first_pem_block(text: &str, tag: &str) -> Option<Vec<u8>> {
    let begin = format!("-----BEGIN {tag}-----");
    let end = format!("-----END {tag}-----");
    let start = text.find(&begin)? + begin.len();
    let stop = text[start..].find(&end)? + start;
    let b64: String = text[start..stop]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(b64).ok()
}

/// Drop everything before the first PEM block (openssl prints Bag Attributes).
fn strip_before_pem(text: &str) -> String {
    match text.find("-----BEGIN") {
        Some(idx) => text[idx..].to_string(),
        None => text.to_string(),
    }
}

fn pem_encode(tag: &str, der: &[u8]) -> String {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(der);
    let mut out = format!("-----BEGIN {tag}-----\n");
    for chunk in b64.as_bytes().chunks(64) {
        out.push_str(std::str::from_utf8(chunk).unwrap());
        out.push('\n');
    }
    out.push_str(&format!("-----END {tag}-----\n"));
    out
}

fn run_quiet(bin: &str, args: &[&str]) -> Result<(), String> {
    let mut cmd = std::process::Command::new(bin);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.output()
        .map(|o| {
            if o.status.success() {
                Ok(())
            } else {
                Err(format!("{bin} failed"))
            }
        })
        .map_err(|e| e.to_string())?
}

fn openssl_bins() -> Vec<&'static str> {
    vec![
        "openssl",
        "/usr/bin/openssl",
        "/opt/homebrew/bin/openssl",
        "/usr/local/bin/openssl",
        r"C:\Program Files\Git\usr\bin\openssl.exe",
        r"C:\Program Files\OpenSSL-Win64\bin\openssl.exe",
    ]
}

fn bundled_mitm_dirs(extra_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for extra in extra_dirs {
        out.push(extra.join("mitm"));
        out.push(extra.join("resources").join("mitm"));
        out.push(extra.clone());
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            out.push(parent.join("mitm"));
            out.push(parent.join("resources").join("mitm"));
        }
    }
    out
}

fn p12_candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(d) = dirs::download_dir() {
        v.push(d.join(DEFAULT_P12));
    }
    if let Some(h) = dirs::home_dir() {
        v.push(h.join("Downloads").join(DEFAULT_P12));
        v.push(h.join(DEFAULT_P12));
        v.push(h.join(".androidtv-remote").join("mitm").join(DEFAULT_P12));
    }
    v.push(ca_dir().join(DEFAULT_P12));
    v
}

fn ca_dir() -> PathBuf {
    crate::paths::config_dir().join("mitm")
}

static CLIENT_CONFIG: OnceLock<rustls::ClientConfig> = OnceLock::new();

/// TLS client config for upstream connections (public roots).
pub fn upstream_client_config() -> &'static rustls::ClientConfig {
    CLIENT_CONFIG.get_or_init(|| {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("atv-mitm-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn generated_ca_signs_leaves() {
        let dir = temp_dir("generated");
        let ca = Arc::new(MitmCa::generate(&dir).expect("generate CA"));
        assert!(matches!(ca.source, CaSource::Generated(_)));
        let config = ca.server_config("example.com").expect("leaf server config");
        // Cached lookup returns the same handle.
        assert!(Arc::ptr_eq(&config, &ca.server_config("example.com").unwrap()));
        assert!(ca.server_config("other.example").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn charles_p12_import_produces_working_issuer() {
        let p12 = dirs::download_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(DEFAULT_P12);
        if !p12.exists() {
            return; // p12 not present on this machine
        }
        let dir = temp_dir("p12");
        MitmCa::import_p12(&p12, DEFAULT_P12_PASSWORD, &dir).expect("p12 import");
        let ca = Arc::new(MitmCa::load_dir(&dir).expect("load imported CA"));
        assert!(matches!(ca.source, CaSource::Charles2018P12(_)));
        assert!(ca.server_config("api.jawwy.tv").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rust_p12_import_does_not_need_openssl() {
        let p12 = dirs::download_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(DEFAULT_P12);
        if !p12.exists() {
            return;
        }
        let dir = temp_dir("p12-rust");
        MitmCa::import_p12_rust(&p12, DEFAULT_P12_PASSWORD, &dir).expect("pure-rust p12");
        let pem = std::fs::read_to_string(dir.join("ca.pem")).unwrap();
        assert!(pem.contains("BEGIN CERTIFICATE"));
        let key = std::fs::read_to_string(dir.join("ca-key.pem")).unwrap();
        assert!(key.contains("BEGIN PRIVATE KEY") || key.contains("BEGIN RSA PRIVATE KEY"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
impl MitmCa {
    pub fn ca_cert_der_for_test(&self) -> &Vec<u8> {
        &self.ca_cert_der
    }
}
