//! HTTPS/TLS 支持：优先加载 `data/certs/server.crt` 与 `data/certs/server.key`；
//! 两者都不存在时自动生成自签名证书（含 localhost、127.0.0.1、::1 与本机局域网 IPv4）。
//!
//! 路径固定为当前工作目录下的 `data/certs/`，不通过环境变量配置，随部署目录自动推导。
//! 若需使用受信任证书（如 Let's Encrypt），把证书与私钥分别覆盖为上述文件名后重启即可；
//! 删除这两个文件会在下次启动时重新生成自签名证书。

use std::error::Error;
use std::fs;

use chrono::Datelike;
use std::net::{IpAddr, UdpSocket};
use std::path::{Path, PathBuf};

use axum_server::tls_rustls::RustlsConfig;
use rcgen::{CertificateParams, DnType, ExtendedKeyUsagePurpose, KeyPair, KeyUsagePurpose};

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
