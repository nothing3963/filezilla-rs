//! Minimal HTTP CONNECT and SOCKS5 proxy support.
//!
//! Only tunnelling is implemented; FTP active mode and implicit FTPS through a
//! proxy are intentionally unsupported for now.

use crate::error::{AppError, AppResult};
use crate::models::SiteConfig;
use base64::Engine;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub async fn connect_tcp(
    config: &SiteConfig,
    proxy_password: Option<&str>,
    host: &str,
    port: u16,
    timeout: Duration,
) -> AppResult<TcpStream> {
    let proxy_type = config.proxy_type.trim().to_ascii_lowercase();
    if proxy_type.is_empty() || proxy_type == "none" {
        return tokio::time::timeout(timeout, TcpStream::connect((host, port)))
            .await
            .map_err(|_| {
                AppError::new("TIMEOUT", format!("Timed out connecting to {host}:{port}"))
            })?
            .map_err(|e| {
                AppError::new("NETWORK", format!("Cannot connect to {host}:{port}: {e}"))
            });
    }

    if config.proxy_host.trim().is_empty() {
        return Err(AppError::invalid("Proxy host is required"));
    }
    let proxy_endpoint = format!(
        "{}:{}",
        config.proxy_host.trim(),
        if config.proxy_port == 0 {
            if proxy_type == "http" {
                8080
            } else {
                1080
            }
        } else {
            config.proxy_port
        }
    );
    let stream = tokio::time::timeout(timeout, TcpStream::connect(&proxy_endpoint))
        .await
        .map_err(|_| {
            AppError::new(
                "TIMEOUT",
                format!("Timed out connecting to proxy {proxy_endpoint}"),
            )
        })?
        .map_err(|e| {
            AppError::new(
                "PROXY",
                format!("Cannot connect to proxy {proxy_endpoint}: {e}"),
            )
        })?;

    let handshake = async {
        match proxy_type.as_str() {
            "http" => http_connect(stream, config, proxy_password, host, port).await,
            "socks5" => socks5_connect(stream, config, proxy_password, host, port).await,
            other => Err(AppError::new(
                "PROXY",
                format!("Unsupported proxy type: {other}"),
            )),
        }
    };
    tokio::time::timeout(timeout, handshake)
        .await
        .map_err(|_| AppError::new("TIMEOUT", "Proxy handshake timed out"))?
}

async fn http_connect(
    mut stream: TcpStream,
    config: &SiteConfig,
    password: Option<&str>,
    host: &str,
    port: u16,
) -> AppResult<TcpStream> {
    let authority = format!("{host}:{port}");
    let mut request = format!(
        "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\nProxy-Connection: Keep-Alive\r\n"
    );
    if !config.proxy_username.is_empty() {
        let credentials = format!("{}:{}", config.proxy_username, password.unwrap_or_default());
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes());
        request.push_str(&format!("Proxy-Authorization: Basic {encoded}\r\n"));
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| AppError::new("PROXY", format!("Proxy request failed: {e}")))?;

    // Read the header byte by byte: a proxy may send the first tunnelled
    // payload bytes in the same TCP segment, and TcpStream has no push-back.
    let mut response = Vec::with_capacity(256);
    let mut byte = [0u8; 1];
    loop {
        stream
            .read_exact(&mut byte)
            .await
            .map_err(|e| AppError::new("PROXY", format!("Proxy response failed: {e}")))?;
        response.push(byte[0]);
        if response.ends_with(b"\r\n\r\n") {
            break;
        }
        if response.len() > 16 * 1024 {
            return Err(AppError::new("PROXY", "Proxy response is too large"));
        }
    }
    let text = String::from_utf8_lossy(&response);
    let status = text.lines().next().unwrap_or_default();
    let code = status
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(0);
    if code != 200 {
        return Err(AppError::new(
            "PROXY",
            format!("HTTP proxy refused the tunnel: {status}"),
        ));
    }
    Ok(stream)
}

async fn socks5_connect(
    mut stream: TcpStream,
    config: &SiteConfig,
    password: Option<&str>,
    host: &str,
    port: u16,
) -> AppResult<TcpStream> {
    let auth = !config.proxy_username.is_empty();
    let methods: &[u8] = if auth { &[0x00, 0x02] } else { &[0x00] };
    stream
        .write_all(&[&[0x05u8, methods.len() as u8], methods].concat())
        .await
        .map_err(|e| AppError::new("PROXY", format!("SOCKS5 greeting failed: {e}")))?;
    let mut selected = [0u8; 2];
    stream
        .read_exact(&mut selected)
        .await
        .map_err(|e| AppError::new("PROXY", format!("SOCKS5 greeting failed: {e}")))?;
    if selected[0] != 0x05 {
        return Err(AppError::new("PROXY", "Invalid SOCKS5 proxy response"));
    }
    match selected[1] {
        0x00 => {}
        0x02 => {
            let user = config.proxy_username.as_bytes();
            let pass = password.unwrap_or_default().as_bytes();
            if user.len() > 255 || pass.len() > 255 {
                return Err(AppError::new("PROXY", "SOCKS5 credentials are too long"));
            }
            let mut auth_request = vec![0x01, user.len() as u8];
            auth_request.extend_from_slice(user);
            auth_request.push(pass.len() as u8);
            auth_request.extend_from_slice(pass);
            stream
                .write_all(&auth_request)
                .await
                .map_err(|e| AppError::new("PROXY", format!("SOCKS5 auth failed: {e}")))?;
            let mut auth_response = [0u8; 2];
            stream
                .read_exact(&mut auth_response)
                .await
                .map_err(|e| AppError::new("PROXY", format!("SOCKS5 auth failed: {e}")))?;
            if auth_response[1] != 0 {
                return Err(AppError::new("PROXY", "SOCKS5 authentication failed"));
            }
        }
        0xFF => {
            return Err(AppError::new(
                "PROXY",
                "SOCKS5 proxy rejected all authentication methods",
            ))
        }
        method => {
            return Err(AppError::new(
                "PROXY",
                format!("SOCKS5 proxy selected unsupported auth method {method}"),
            ))
        }
    }

    let host_bytes = host.as_bytes();
    if host_bytes.len() > 255 {
        return Err(AppError::new("PROXY", "Hostname is too long for SOCKS5"));
    }
    let mut request = vec![0x05, 0x01, 0x00, 0x03, host_bytes.len() as u8];
    request.extend_from_slice(host_bytes);
    request.extend_from_slice(&port.to_be_bytes());
    stream
        .write_all(&request)
        .await
        .map_err(|e| AppError::new("PROXY", format!("SOCKS5 connect failed: {e}")))?;

    let mut header = [0u8; 4];
    stream
        .read_exact(&mut header)
        .await
        .map_err(|e| AppError::new("PROXY", format!("SOCKS5 connect failed: {e}")))?;
    if header[1] != 0 {
        return Err(AppError::new(
            "PROXY",
            format!("SOCKS5 proxy refused the connection (code {})", header[1]),
        ));
    }
    let address_len = match header[3] {
        0x01 => 4,
        0x04 => 16,
        0x03 => {
            let mut length = [0u8; 1];
            stream
                .read_exact(&mut length)
                .await
                .map_err(|e| AppError::new("PROXY", format!("SOCKS5 reply failed: {e}")))?;
            length[0] as usize
        }
        _ => return Err(AppError::new("PROXY", "Invalid SOCKS5 address type")),
    };
    let mut discard = vec![0u8; address_len + 2];
    stream
        .read_exact(&mut discard)
        .await
        .map_err(|e| AppError::new("PROXY", format!("SOCKS5 reply failed: {e}")))?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn http_connect_tunnel() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 1024];
            let read = socket.read(&mut buffer).await.unwrap();
            let request = String::from_utf8_lossy(&buffer[..read]);
            assert!(request.starts_with("CONNECT example.com:21"));
            socket
                .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                .await
                .unwrap();
            socket.write_all(b"220 hello\r\n").await.unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
        });

        let mut config = SiteConfig {
            protocol: crate::models::Protocol::Ftp,
            host: "example.com".into(),
            port: 21,
            username: String::new(),
            auth_method: crate::models::AuthMethod::Password,
            private_key_path: None,
            initial_remote_dir: None,
            local_dir: None,
            passive_mode: true,
            verify_certificate: true,
            connect_timeout_secs: 5,
            keepalive_secs: 0,
            transfer_mode: crate::models::TransferMode::Auto,
            encoding: "UTF-8".into(),
            max_connections: 1,
            notes: None,
            proxy_type: "http".into(),
            proxy_host: "127.0.0.1".into(),
            proxy_port: addr.port(),
            proxy_username: String::new(),
        };
        let mut stream = connect_tcp(&config, None, "example.com", 21, Duration::from_secs(2))
            .await
            .unwrap();
        let mut line = [0u8; 7];
        stream.read_exact(&mut line).await.unwrap();
        assert_eq!(&line, b"220 hel");
        config.proxy_type.clear();
    }
}
