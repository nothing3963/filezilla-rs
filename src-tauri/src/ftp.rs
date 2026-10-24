//! FTP / FTPS adapter built on `suppaftp` + rustls.

use crate::error::{AppError, AppResult};
use crate::list_parser::{parse_list, parse_mlsd, parse_mlsd_line};
use crate::models::{Protocol, RemoteEntry};
use crate::remote::{pump, ProgressCallback, RemoteFs, ResolvedConfig, TransferControl};
use crate::remote_path::{file_name, mode_to_string, normalize, parent};
use async_trait::async_trait;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::ring;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use suppaftp::tokio::{AsyncRustlsConnector, AsyncRustlsFtpStream};
use suppaftp::types::FileType;
use suppaftp::{Encoding as FtpEncoding, FtpError, Status};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio_rustls::TlsConnector;

/// Adapter between `suppaftp`'s byte-oriented encoding trait and encoding_rs.
#[derive(Debug)]
struct LegacyEncoding(&'static encoding_rs::Encoding);

impl FtpEncoding for LegacyEncoding {
    fn decode(&self, bytes: &[u8]) -> String {
        let (text, _, _) = self.0.decode(bytes);
        text.into_owned()
    }

    fn encode(&self, text: &str) -> std::io::Result<Vec<u8>> {
        let (bytes, _, had_errors) = self.0.encode(text);
        if had_errors {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("text is not representable in {}", self.0.name()),
            ))
        } else {
            Ok(bytes.into_owned())
        }
    }
}

fn encoding_by_name(name: &str) -> &'static encoding_rs::Encoding {
    let normalized = name.to_ascii_lowercase().replace(['-', '_', ' '], "");
    match normalized.as_str() {
        "gbk" | "gb2312" | "cp936" => encoding_rs::GBK,
        "gb18030" => encoding_rs::GB18030,
        "big5" | "cp950" => encoding_rs::BIG5,
        "shiftjis" | "sjis" | "cp932" => encoding_rs::SHIFT_JIS,
        "euckr" | "cp949" => encoding_rs::EUC_KR,
        "windows1252" | "cp1252" | "iso88591" | "latin1" => encoding_rs::WINDOWS_1252,
        _ => encoding_rs::UTF_8,
    }
}

#[allow(dead_code)]
pub struct FtpRemote {
    client: AsyncRustlsFtpStream,
    protocol: Protocol,
    banner: Option<String>,
    features: Vec<String>,
    home: String,
    endpoint: String,
    /// True while the active encoding is a lossy UTF-8 decode that may need a
    /// legacy fallback (GB18030) for servers with non-UTF-8 file names.
    auto_encoding: bool,
    /// Encoding currently used for commands and replies.
    encoding: String,
}

impl FtpRemote {
    pub async fn connect(resolved: &ResolvedConfig) -> AppResult<Box<dyn RemoteFs>> {
        let cfg = &resolved.config;
        let port = cfg.effective_port();
        let timeout = Duration::from_secs(cfg.connect_timeout_secs.max(3));
        let endpoint = format!("{}:{}", cfg.host, port);
        if cfg.uses_proxy() && !cfg.passive_mode {
            return Err(AppError::unsupported(
                "Active mode cannot be used through a proxy; enable passive mode",
            ));
        }
        if cfg.uses_proxy() && cfg.protocol == Protocol::FtpsImplicit {
            return Err(AppError::unsupported(
                "Implicit FTPS through a proxy is not supported; use explicit FTPS",
            ));
        }
        let mut connector = if cfg.protocol.uses_tls() {
            Some(tls_connector(cfg.verify_certificate)?)
        } else {
            None
        };
        let proxy_stream = if cfg.uses_proxy() {
            Some(
                crate::proxy::connect_tcp(
                    cfg,
                    resolved.proxy_password.as_deref(),
                    &cfg.host,
                    port,
                    timeout,
                )
                .await?,
            )
        } else {
            None
        };

        let connect_fut = async {
            let client = match proxy_stream {
                Some(stream) => {
                    let plain = AsyncRustlsFtpStream::connect_with_stream(stream).await?;
                    match cfg.protocol {
                        Protocol::Ftps => {
                            plain
                                .into_secure(connector.take().expect("TLS protocol"), &cfg.host)
                                .await?
                        }
                        _ => plain,
                    }
                }
                None => match cfg.protocol {
                    Protocol::FtpsImplicit => {
                        AsyncRustlsFtpStream::connect_secure_implicit(
                            (cfg.host.as_str(), port),
                            connector.take().expect("TLS protocol"),
                            &cfg.host,
                        )
                        .await?
                    }
                    Protocol::Ftps => {
                        let plain =
                            AsyncRustlsFtpStream::connect((cfg.host.as_str(), port)).await?;
                        plain
                            .into_secure(connector.take().expect("TLS protocol"), &cfg.host)
                            .await?
                    }
                    _ => AsyncRustlsFtpStream::connect((cfg.host.as_str(), port)).await?,
                },
            };
            let mut client = if cfg.passive_mode {
                client
            } else {
                client.active_mode(Duration::from_secs(cfg.connect_timeout_secs.max(10)))
            };
            let user = if cfg.username.trim().is_empty() {
                "anonymous"
            } else {
                cfg.username.trim()
            };
            let password = resolved
                .password
                .as_deref()
                .unwrap_or(if user == "anonymous" {
                    "anonymous@filezilla.modern"
                } else {
                    ""
                });
            client.login(user, password).await?;
            let auto_encoding = cfg.encoding.eq_ignore_ascii_case("utf-8");
            if !auto_encoding {
                client
                    .set_encoding(LegacyEncoding(encoding_by_name(&cfg.encoding)))
                    .await;
            }
            client.transfer_type(FileType::Binary).await?;

            let features = match client.feat().await {
                Ok(map) => {
                    let mut list: Vec<String> = map
                        .into_iter()
                        .map(|(name, value)| match value {
                            Some(v) => format!("{name} {v}"),
                            None => name,
                        })
                        .collect();
                    list.sort();
                    list
                }
                Err(_) => Vec::new(),
            };
            let banner = client
                .get_welcome_msg()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let home = client.pwd().await?;
            Ok::<_, FtpError>((client, features, banner, home, auto_encoding))
        };

        let (client, features, banner, home, auto_encoding) =
            tokio::time::timeout(timeout, connect_fut)
                .await
                .map_err(|_| {
                    AppError::new(
                        "TIMEOUT",
                        format!(
                            "Timed out connecting to {endpoint} after {}s",
                            timeout.as_secs()
                        ),
                    )
                })?
                .map_err(|e| map_ftp_error(e, &endpoint, &cfg.encoding))?;

        Ok(Box::new(Self {
            client,
            protocol: cfg.protocol,
            banner,
            features,
            home,
            endpoint,
            auto_encoding,
            encoding: cfg.encoding.clone(),
        }))
    }

    fn has_mlsd(&self) -> bool {
        self.features
            .iter()
            .any(|f| f.to_ascii_uppercase().starts_with("MLST"))
            || self
                .features
                .iter()
                .any(|f| f.to_ascii_uppercase().starts_with("MLSD"))
    }

    fn endpoint(&self) -> String {
        self.endpoint.clone()
    }

    async fn fetch_entries(&mut self, path: &str) -> AppResult<Vec<RemoteEntry>> {
        let path = normalize(path);
        if self.has_mlsd() {
            if let Ok(lines) = self.client.mlsd(Some(&path)).await {
                let entries = parse_mlsd(&lines, &path);
                if !entries.is_empty() {
                    return Ok(entries);
                }
            }
        }
        let lines = self
            .client
            .list(Some(&path))
            .await
            .map_err(|e| map_ftp_error(e, &self.endpoint(), &self.encoding))?;
        let entries = parse_list(&lines, &path);
        if entries.is_empty() && !lines.is_empty() {
            // Some servers answer with a single line for the directory itself.
            return Ok(Vec::new());
        }
        Ok(entries)
    }

    /// Some servers reject absolute, non-ASCII paths in `DELE`/`RMD` even
    /// though `CWD` into the same directory works.  Retry once relative to the
    /// parent directory before surfacing the server's error.
    async fn remove_path(&mut self, path: &str, is_dir: bool) -> AppResult<()> {
        let normalized = normalize(path);
        let first = if is_dir {
            self.client.rmdir(&normalized).await
        } else {
            self.client.rm(&normalized).await
        };
        if first.is_ok() {
            return Ok(());
        }
        let first_error = first.expect_err("checked above");
        let Some(parent) = parent(&normalized) else {
            return Err(map_ftp_error(first_error, &self.endpoint(), &self.encoding));
        };
        let name = file_name(&normalized);
        if name.is_empty() || parent == normalized {
            return Err(map_ftp_error(first_error, &self.endpoint(), &self.encoding));
        }
        let original = self.client.pwd().await.ok();
        if self.client.cwd(&parent).await.is_err() {
            return Err(map_ftp_error(first_error, &self.endpoint(), &self.encoding));
        }
        let second = if is_dir {
            self.client.rmdir(&name).await
        } else {
            self.client.rm(&name).await
        };
        if let Some(original) = original {
            let _ = self.client.cwd(original).await;
        }
        match second {
            Ok(()) => Ok(()),
            Err(second_error) => Err(map_ftp_error(
                second_error,
                &self.endpoint(),
                &self.encoding,
            )),
        }
    }
}

#[async_trait]
impl RemoteFs for FtpRemote {
    fn protocol(&self) -> Protocol {
        self.protocol
    }

    fn banner(&self) -> Option<String> {
        self.banner.clone()
    }

    fn features(&self) -> Vec<String> {
        self.features.clone()
    }

    fn encoding(&self) -> Option<String> {
        Some(self.encoding.clone())
    }

    async fn list(&mut self, path: &str) -> AppResult<Vec<RemoteEntry>> {
        let path = normalize(path);
        let mut entries = self.fetch_entries(&path).await?;
        // Legacy servers often ignore UTF-8 and send GBK/GB18030 bytes.  Lossy
        // UTF-8 decoding leaves U+FFFD, which is a safe signal to retry.
        if self.auto_encoding && entries.iter().any(|entry| entry.name.contains('\u{FFFD}')) {
            self.auto_encoding = false;
            self.encoding = "GB18030".to_string();
            self.client
                .set_encoding(LegacyEncoding(encoding_rs::GB18030))
                .await;
            entries = self.fetch_entries(&path).await?;
        }
        Ok(entries)
    }

    async fn stat(&mut self, path: &str) -> AppResult<RemoteEntry> {
        let path = normalize(path);
        if path == "/" {
            return Ok(RemoteEntry {
                name: "/".into(),
                path,
                is_dir: true,
                is_symlink: false,
                size: 0,
                modified: None,
                permissions: Some(mode_to_string(0o755, true)),
                mode: Some(0o755),
                owner: None,
                group: None,
            });
        }
        if self.has_mlsd() {
            if let Ok(line) = self.client.mlst(Some(&path)).await {
                let candidate = line.trim();
                if let Some(entry) =
                    parse_mlsd_line(candidate, parent(&path).as_deref().unwrap_or("/"))
                {
                    return Ok(entry);
                }
            }
        }
        let dir = parent(&path).unwrap_or_else(|| "/".into());
        let name = file_name(&path);
        let entries = self.list(&dir).await?;
        entries
            .into_iter()
            .find(|e| e.name == name)
            .ok_or_else(|| AppError::not_found(format!("{path} not found")))
    }

    async fn mkdir(&mut self, path: &str) -> AppResult<()> {
        self.client
            .mkdir(path)
            .await
            .map_err(|e| map_ftp_error(e, &self.endpoint(), &self.encoding))
    }

    async fn remove_file(&mut self, path: &str) -> AppResult<()> {
        self.remove_path(path, false).await
    }

    async fn remove_dir(&mut self, path: &str) -> AppResult<()> {
        self.remove_path(path, true).await
    }

    async fn rename(&mut self, from: &str, to: &str) -> AppResult<()> {
        self.client
            .rename(from, to)
            .await
            .map_err(|e| map_ftp_error(e, &self.endpoint(), &self.encoding))
    }
    async fn chmod(&mut self, path: &str, mode: u32) -> AppResult<()> {
        self.client
            .site(format!("CHMOD {mode:04o} {path}"))
            .await
            .map(|_| ())
            .map_err(|e| map_ftp_error(e, &self.endpoint(), &self.encoding))
    }

    async fn set_mtime(&mut self, path: &str, timestamp: i64) -> AppResult<()> {
        let dt = chrono::DateTime::from_timestamp(timestamp, 0)
            .ok_or_else(|| AppError::invalid("Invalid modification timestamp"))?;
        let stamp = dt.format("%Y%m%d%H%M%S").to_string();
        self.client
            .custom_command(
                format!("MFMT {stamp} {path}"),
                &[Status::File, Status::CommandOk],
            )
            .await
            .map(|_| ())
            .map_err(|e| map_ftp_error(e, &self.endpoint(), &self.encoding))
    }

    async fn realpath(&mut self, path: &str) -> AppResult<String> {
        if path.is_empty() || path == "." {
            return self
                .client
                .pwd()
                .await
                .map_err(|e| map_ftp_error(e, &self.endpoint(), &self.encoding));
        }
        if path.starts_with('/') {
            return Ok(normalize(path));
        }
        let pwd = self
            .client
            .pwd()
            .await
            .map_err(|e| map_ftp_error(e, &self.endpoint(), &self.encoding))?;
        Ok(normalize(&format!("{pwd}/{path}")))
    }

    async fn keepalive(&mut self) -> AppResult<()> {
        self.client
            .noop()
            .await
            .map_err(|e| map_ftp_error(e, &self.endpoint(), &self.encoding))
    }

    async fn close(&mut self) -> AppResult<()> {
        let _ = self.client.quit().await;
        Ok(())
    }

    async fn download(
        &mut self,
        remote: &str,
        local: &Path,
        resume_from: u64,
        ctl: &TransferControl,
        progress: ProgressCallback,
    ) -> AppResult<u64> {
        if let Some(parent) = local.parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
        let mut file = if resume_from > 0 {
            let mut file = OpenOptions::new()
                .append(true)
                .open(local)
                .await
                .map_err(|e| {
                    AppError::new("LOCAL_IO", format!("Cannot open {}: {e}", local.display()))
                })?;
            // `append` already positions at EOF, but keep the handle honest.
            file.seek(std::io::SeekFrom::End(0)).await.ok();
            file
        } else {
            File::create(local).await.map_err(|e| {
                AppError::new(
                    "LOCAL_IO",
                    format!("Cannot create {}: {e}", local.display()),
                )
            })?
        };

        if resume_from > 0 {
            self.client
                .resume_transfer(resume_from as usize)
                .await
                .map_err(|e| map_ftp_error(e, &self.endpoint(), &self.encoding))?;
        }
        let mut stream = self
            .client
            .retr_as_stream(remote)
            .await
            .map_err(|e| map_ftp_error(e, &self.endpoint(), &self.encoding))?;
        let result = pump(&mut stream, &mut file, resume_from, ctl, &progress).await;
        match result {
            Ok(total) => {
                stream
                    .finish()
                    .await
                    .map_err(|e| map_ftp_error(e, &self.endpoint(), &self.encoding))?;
                file.flush().await.ok();
                Ok(total)
            }
            Err(err) => {
                let _ = self.client.abort(stream).await;
                Err(err)
            }
        }
    }

    async fn upload(
        &mut self,
        local: &Path,
        remote: &str,
        resume_from: u64,
        ctl: &TransferControl,
        progress: ProgressCallback,
    ) -> AppResult<u64> {
        let mut file = File::open(local).await.map_err(|e| {
            AppError::new("LOCAL_IO", format!("Cannot read {}: {e}", local.display()))
        })?;
        if resume_from > 0 {
            file.seek(std::io::SeekFrom::Start(resume_from)).await?;
        }
        // RFC 3659 allows REST before STOR/RETR, but not before APPE. Servers
        // that do not advertise "REST STREAM" answer `REST n` + `APPE` with
        // "550 Cannot RESTart at any position while APPEnding to a file", so
        // only send REST together with STOR. Without the capability, fall back
        // to plain APPE, which continues at the remote EOF (== resume_from).
        let rest_stream = self
            .features
            .iter()
            .any(|f| f.to_ascii_uppercase().starts_with("REST STREAM"));
        let mut stream = if resume_from > 0 && !rest_stream {
            self.client.append_with_stream(remote).await
        } else {
            if resume_from > 0 {
                self.client
                    .resume_transfer(resume_from as usize)
                    .await
                    .map_err(|e| map_ftp_error(e, &self.endpoint(), &self.encoding))?;
            }
            self.client.put_with_stream(remote).await
        }
        .map_err(|e| map_ftp_error(e, &self.endpoint(), &self.encoding))?;
        let result = pump(&mut file, &mut stream, resume_from, ctl, &progress).await;
        match result {
            Ok(total) => {
                stream
                    .finish()
                    .await
                    .map_err(|e| map_ftp_error(e, &self.endpoint(), &self.encoding))?;
                Ok(total)
            }
            Err(err) => {
                let _ = self.client.abort(stream).await;
                Err(err)
            }
        }
    }
}

fn tls_connector(verify: bool) -> AppResult<AsyncRustlsConnector> {
    let config = if verify {
        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth()
    } else {
        ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(InsecureVerifier))
            .with_no_client_auth()
    };
    let connector = TlsConnector::from(Arc::new(config));
    Ok(AsyncRustlsConnector::from(connector))
}

#[derive(Debug)]
struct InsecureVerifier;

impl ServerCertVerifier for InsecureVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

fn decode_reply_text(bytes: &[u8], encoding: &str) -> String {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.trim().to_string();
    }
    // A server configured for a legacy codepage may still send UTF-8 for
    // some replies.  Valid UTF-8 wins above; otherwise decode with the
    // session encoding.  If the session is UTF-8 but the bytes are not,
    // try GB18030 before giving up (common on Chinese FTP servers).
    let name = if encoding.eq_ignore_ascii_case("utf-8") {
        "GB18030"
    } else {
        encoding
    };
    let (text, _, _) = encoding_by_name(name).decode(bytes);
    text.trim().to_string()
}

fn map_ftp_error(error: FtpError, endpoint: &str, encoding: &str) -> AppError {
    match error {
        FtpError::ConnectionError(e) => {
            AppError::new("NETWORK", format!("Cannot connect to {endpoint}: {e}"))
        }
        FtpError::SecureError(e) => AppError::new("TLS", format!("TLS negotiation failed: {e}")),
        FtpError::UnexpectedResponse(response) => {
            let code = response.status.code();
            let body = decode_reply_text(&response.body, encoding);
            let message = if body.is_empty() {
                format!("Server replied {code}")
            } else {
                body
            };
            match code {
                530 | 531 | 532 => AppError::new("AUTH", message),
                550 | 551 | 552 | 553 => AppError::new("REMOTE", message),
                _ => AppError::new("PROTOCOL", message).with_detail(format!("FTP status {code}")),
            }
        }
        FtpError::BadResponse => AppError::new("PROTOCOL", "Malformed response from server"),
        FtpError::InvalidAddress(e) => AppError::new("NETWORK", format!("Invalid address: {e}")),
        FtpError::DataConnectionAlreadyOpen => {
            AppError::new("PROTOCOL", "A data connection is already open")
        }
    }
}

#[allow(dead_code)]
fn is_timeout(status: Status) -> bool {
    status.code() == 421
}
#[cfg(test)]
mod tests {
    use super::decode_reply_text;

    #[test]
    fn decodes_utf8_and_gbk_server_replies() {
        assert_eq!(decode_reply_text(b"550 not found", "GBK"), "550 not found");
        assert_eq!(
            decode_reply_text("550 中文错误".as_bytes(), "GBK"),
            "550 中文错误"
        );
        assert_eq!(
            decode_reply_text(b"550 \xd6\xd0\xce\xc4", "GBK"),
            "550 中文"
        );
        assert_eq!(
            decode_reply_text(b"550 \xd6\xd0\xce\xc4", "UTF-8"),
            "550 中文"
        );
    }
}
