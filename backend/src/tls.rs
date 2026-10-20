//! HTTPS/TLS 支持：优先加载 `data/certs/server.crt` 与 `data/certs/server.key`；
//! 两者都不存在时自动生成自签名证书（含 localhost、127.0.0.1、::1 与本机局域网 IPv4）。
//!
//! 路径固定为当前工作目录下的 `data/certs/`，不通过环境变量配置，随部署目录自动推导。
//! 若需使用受信任证书（如 Let's Encrypt），把证书与私钥分别覆盖为上述文件名后重启即可；
//! 删除这两个文件会在下次启动时重新生成自签名证书。

use std::error::Error;
use std::fs;
use std::future::Future;
use std::io;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum_server::tls_rustls::RustlsConfig;
use chrono::Datelike;
use rcgen::{CertificateParams, DnType, ExtendedKeyUsagePurpose, KeyPair, KeyUsagePurpose};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{server::TlsStream, TlsAcceptor};

const CERT_FILE: &str = "server.crt";
const KEY_FILE: &str = "server.key";

/// TLS 配置与证书路径（启动日志使用）。
pub struct Tls {
    pub config: RustlsConfig,
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

/// 加载或生成当前工作目录 `data/certs/` 下的证书。
pub async fn load_or_generate() -> Result<Tls, Box<dyn Error>> {
    let dir = PathBuf::from("data").join("certs");
    load_or_generate_in(&dir).await
}

async fn load_or_generate_in(dir: &Path) -> Result<Tls, Box<dyn Error>> {
    let cert_path = dir.join(CERT_FILE);
    let key_path = dir.join(KEY_FILE);

    let has_cert = cert_path.is_file();
    let has_key = key_path.is_file();
    if !has_cert && !has_key {
        eprintln!(
            "[tls] 未找到证书，自动生成自签名证书：{}",
            cert_path.display()
        );
        generate_self_signed(&cert_path, &key_path)?;
    } else if has_cert != has_key {
        return Err(format!(
            "TLS 证书不完整：{}（cert={has_cert}, key={has_key}）。请同时提供证书和私钥，或同时删除后让服务自动重新生成。",
            dir.display()
        )
        .into());
    }

    let config = RustlsConfig::from_pem_file(&cert_path, &key_path)
        .await
        .map_err(|e| {
            format!(
                "无法加载 TLS 证书 {} / {}：{e}",
                cert_path.display(),
                key_path.display()
            )
        })?;

    Ok(Tls {
        config,
        cert_path,
        key_path,
    })
}

/// 生成自签名证书与私钥；私钥权限设为 0600（Unix）。
fn generate_self_signed(cert_path: &Path, key_path: &Path) -> Result<(), Box<dyn Error>> {
    if let Some(dir) = cert_path.parent() {
        fs::create_dir_all(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        }
    }

    let key_pair = KeyPair::generate()?;

    let mut sans = vec![
        "localhost".to_owned(),
        "127.0.0.1".to_owned(),
        "::1".to_owned(),
    ];
    if let Some(ip) = local_ipv4() {
        let ip = ip.to_string();
        if !sans.contains(&ip) {
            sans.push(ip);
        }
    }

    let mut params = CertificateParams::new(sans)?;
    params
        .distinguished_name
        .push(DnType::CommonName, "Snack Warehouse");
    // 有效期 10 年，避免每次启动都因证书过期报错；同时声明服务器身份用途。
    let year = chrono::Utc::now().year() + 10;
    params.not_before = rcgen::date_time_ymd(2024, 1, 1);
    params.not_after = rcgen::date_time_ymd(year, 1, 1);
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];

    let cert = params.self_signed(&key_pair)?;
    fs::write(cert_path, cert.pem())?;
    fs::write(key_path, key_pair.serialize_pem())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(key_path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// 探测本机出口局域网 IPv4（UDP connect 只设置路由，不发送数据）。
fn local_ipv4() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    matches!(ip, IpAddr::V4(_)).then_some(ip)
}

/// 每个连接的传输属性：`tls` 由协议探测结果写入，供中间件判断是否需要跳转。
#[derive(Clone, Debug)]
pub struct HybridAddr {
    pub tls: Arc<AtomicBool>,
    pub peer: SocketAddr,
}

/// 同端口同时识别 TLS 与明文 HTTP 的监听器。
///
/// 协议探测延迟到连接被读取时进行，因此慢客户端不会阻塞 accept 循环。
pub struct HybridListener {
    listener: TcpListener,
    acceptor: TlsAcceptor,
}

impl HybridListener {
    pub fn new(listener: TcpListener, config: RustlsConfig) -> Self {
        Self {
            listener,
            acceptor: TlsAcceptor::from(config.get_inner()),
        }
    }
}

impl<'a> axum::extract::connect_info::Connected<axum::serve::IncomingStream<'a, HybridListener>>
    for HybridAddr
{
    fn connect_info(stream: axum::serve::IncomingStream<'a, HybridListener>) -> Self {
        stream.remote_addr().clone()
    }
}

impl axum::serve::Listener for HybridListener {
    type Io = HybridStream;
    type Addr = HybridAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match self.listener.accept().await {
                Ok((stream, peer)) => {
                    let _ = stream.set_nodelay(true);
                    let tls = Arc::new(AtomicBool::new(false));
                    let io = HybridStream::new(stream, self.acceptor.clone(), tls.clone());
                    return (io, HybridAddr { tls, peer });
                }
                Err(err) => {
                    eprintln!("[server] accept error: {err}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        Ok(HybridAddr {
            tls: Arc::new(AtomicBool::new(false)),
            peer: self.listener.local_addr()?,
        })
    }
}

type Handshake = Pin<Box<dyn Future<Output = io::Result<TlsStream<TcpStream>>> + Send + 'static>>;

enum HybridState {
    Detecting(TcpStream),
    Handshaking(Handshake),
    Tls(Box<TlsStream<TcpStream>>),
    Plain(TcpStream),
    Failed,
}

/// 首字节为 0x16（TLS handshake）则按 TLS 处理，否则按明文 HTTP 处理。
pub struct HybridStream {
    state: HybridState,
    acceptor: TlsAcceptor,
    tls: Arc<AtomicBool>,
}

impl HybridStream {
    fn new(stream: TcpStream, acceptor: TlsAcceptor, tls: Arc<AtomicBool>) -> Self {
        Self {
            state: HybridState::Detecting(stream),
            acceptor,
            tls,
        }
    }

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        loop {
            if matches!(self.state, HybridState::Detecting(_)) {
                let mut byte = [0u8; 1];
                let mut read_buf = ReadBuf::new(&mut byte);
                let peek = match &self.state {
                    HybridState::Detecting(stream) => stream.poll_peek(cx, &mut read_buf),
                    _ => unreachable!(),
                };
                match peek {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(err)) => {
                        self.state = HybridState::Failed;
                        return Poll::Ready(Err(err));
                    }
                    Poll::Ready(Ok(0)) => {
                        self.state = HybridState::Failed;
                        return Poll::Ready(Err(io::ErrorKind::UnexpectedEof.into()));
                    }
                    Poll::Ready(Ok(_)) => {
                        let stream = match std::mem::replace(&mut self.state, HybridState::Failed) {
                            HybridState::Detecting(stream) => stream,
                            _ => unreachable!(),
                        };
                        if byte[0] == 0x16 {
                            let acceptor = self.acceptor.clone();
                            self.state = HybridState::Handshaking(Box::pin(async move {
                                tokio::time::timeout(
                                    Duration::from_secs(10),
                                    acceptor.accept(stream),
                                )
                                .await
                                .map_err(|_| {
                                    io::Error::new(
                                        io::ErrorKind::TimedOut,
                                        "TLS handshake timed out",
                                    )
                                })?
                            }));
                        } else {
                            self.state = HybridState::Plain(stream);
                        }
                    }
                }
            } else if matches!(self.state, HybridState::Handshaking(_)) {
                let result = match &mut self.state {
                    HybridState::Handshaking(fut) => fut.as_mut().poll(cx),
                    _ => unreachable!(),
                };
                match result {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Ok(stream)) => {
                        self.tls.store(true, Ordering::Relaxed);
                        self.state = HybridState::Tls(Box::new(stream));
                    }
                    Poll::Ready(Err(err)) => {
                        self.state = HybridState::Failed;
                        return Poll::Ready(Err(err));
                    }
                }
            } else {
                break;
            }
        }

        match self.state {
            HybridState::Tls(_) | HybridState::Plain(_) => Poll::Ready(Ok(())),
            _ => Poll::Ready(Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "connection is not ready",
            ))),
        }
    }
}

impl AsyncRead for HybridStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match this.poll_ready(cx) {
            Poll::Ready(Ok(())) => match &mut this.state {
                HybridState::Plain(stream) => Pin::new(stream).poll_read(cx, buf),
                HybridState::Tls(stream) => Pin::new(stream.as_mut()).poll_read(cx, buf),
                _ => unreachable!(),
            },
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncWrite for HybridStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        match this.poll_ready(cx) {
            Poll::Ready(Ok(())) => match &mut this.state {
                HybridState::Plain(stream) => Pin::new(stream).poll_write(cx, buf),
                HybridState::Tls(stream) => Pin::new(stream.as_mut()).poll_write(cx, buf),
                _ => unreachable!(),
            },
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match this.poll_ready(cx) {
            Poll::Ready(Ok(())) => match &mut this.state {
                HybridState::Plain(stream) => Pin::new(stream).poll_flush(cx),
                HybridState::Tls(stream) => Pin::new(stream.as_mut()).poll_flush(cx),
                _ => unreachable!(),
            },
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match this.poll_ready(cx) {
            Poll::Ready(Ok(())) => match &mut this.state {
                HybridState::Plain(stream) => Pin::new(stream).poll_shutdown(cx),
                HybridState::Tls(stream) => Pin::new(stream.as_mut()).poll_shutdown(cx),
                _ => unreachable!(),
            },
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// 中间件状态：HTTPS 监听端口，用于拼出跳转地址。
#[derive(Clone, Copy)]
pub struct HttpsPort(pub u16);

/// 明文 HTTP 请求返回 308 跳转到 HTTPS；TLS 响应附加 HSTS。
pub async fn redirect_plain_http(
    State(https_port): State<HttpsPort>,
    ConnectInfo(addr): ConnectInfo<HybridAddr>,
    req: Request,
    next: Next,
) -> Response {
    if addr.tls.load(Ordering::Relaxed) {
        let mut response = next.run(req).await;
        response.headers_mut().insert(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000"),
        );
        return response;
    }

    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .or_else(|| req.uri().authority().map(ToString::to_string))
        .unwrap_or_else(|| "localhost".to_owned());
    let path = req
        .uri()
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or("/");
    let port = if https_port.0 == 443 {
        String::new()
    } else {
        format!(":{}", https_port.0)
    };
    let location = format!("https://{}{}{}", host_without_port(&host), port, path);

    (
        StatusCode::PERMANENT_REDIRECT,
        [(header::LOCATION, location)],
    )
        .into_response()
}

/// 去掉 Host 头里的端口，保留 IPv6 的方括号。
fn host_without_port(host: &str) -> &str {
    if let Some(rest) = host.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            return &host[..end + 2];
        }
    }
    match host.rsplit_once(':') {
        Some((name, port))
            if !name.contains(':')
                && !port.is_empty()
                && port.chars().all(|c| c.is_ascii_digit()) =>
        {
            name
        }
        _ => host,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("snackwh-tls-{}-{name}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn generates_cert_and_key() {
        let dir = temp_dir("generate");
        let cert = dir.join("server.crt");
        let key = dir.join("server.key");

        generate_self_signed(&cert, &key).unwrap();

        let cert_pem = fs::read_to_string(&cert).unwrap();
        let key_pem = fs::read_to_string(&key).unwrap();
        assert!(cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(key_pem.contains("BEGIN PRIVATE KEY"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&key).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        // 再次生成会覆盖为新的有效证书（幂等可用）。
        generate_self_signed(&cert, &key).unwrap();
        assert!(fs::read_to_string(&cert)
            .unwrap()
            .contains("BEGIN CERTIFICATE"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn host_without_port_strips_port() {
        assert_eq!(host_without_port("example.com:3000"), "example.com");
        assert_eq!(host_without_port("example.com"), "example.com");
        assert_eq!(host_without_port("[::1]:3000"), "[::1]");
        assert_eq!(host_without_port("[::1]"), "[::1]");
        assert_eq!(host_without_port("::1"), "::1");
        assert_eq!(host_without_port("127.0.0.1:8080"), "127.0.0.1");
    }

    #[tokio::test]
    async fn loads_generated_certificate() {
        let dir = temp_dir("load");
        let tls = load_or_generate_in(&dir).await.unwrap();
        assert!(tls.cert_path.is_file());
        assert!(tls.key_path.is_file());
        drop(tls);

        // 已存在时直接加载，不重新生成。
        let again = load_or_generate_in(&dir).await.unwrap();
        assert_eq!(
            fs::read_to_string(&again.cert_path).unwrap(),
            fs::read_to_string(dir.join(CERT_FILE)).unwrap()
        );

        fs::remove_dir_all(&dir).ok();
    }
}
