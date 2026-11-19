// Install stage: sign in with an Apple ID, sign SideStore/AltStore, install over the
// on-device tunnel (needs LocalDevVPN on and the pairing file from `pairkit`).

use std::io::{BufRead, Write};
use std::net::IpAddr;
use std::path::PathBuf;

use idevice::remote_pairing::{
    connect_tls_psk_tunnel_native, RemotePairingClient, RpPairingFile, RpPairingSocket,
};
use idevice::rsd::RsdHandshake;
use idevice::tcp::adapter::Adapter;
use isideload::auth::apple_account::{
    TwoFactorCallbackParams, TwoFactorCallbackResponse,
};
use isideload::auth::builder::AppleAccountBuilder;
use isideload::dev::certificates::DevelopmentCertificate;
use isideload::dev::developer_session::DeveloperSession;
use isideload::dev::devices::DevicesApi;
use isideload::dev::teams::TeamsApi;
use isideload::sideload::builder::{ExtensionsBehavior, ExtensionsBehaviorSelection, MaxCertsBehavior, SideloaderBuilder};
use isideload::sideload::bundle::Bundle;
use isideload::util::fs_storage::FsStorage;
use rootcause::Report;
use tokio::net::TcpStream;

type Err = Box<dyn std::error::Error>;

const DEVICE_IP: &str = "10.7.0.1";
const DEVICE_PORT: u16 = 49152;

fn home_dir() -> PathBuf {
    std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."))
}

fn ask(prompt: &str) -> String {
    print!("{prompt}");
    let _ = std::io::stdout().flush();
    let mut s = String::new();
    let _ = std::io::stdin().lock().read_line(&mut s);
    s.trim().to_string()
}

fn ask_secret(prompt: &str) -> String {
    let _ = std::process::Command::new("stty").arg("-echo").status();
    let s = ask(prompt);
    let _ = std::process::Command::new("stty").arg("echo").status();
    println!();
    s
}

fn e<T: std::fmt::Debug>(x: T) -> Err {
    format!("{x:?}").into()
}

type ExtCb = fn(&[Bundle]) -> std::future::Ready<Result<ExtensionsBehavior, Report>>;

pub async fn run(which: &str) -> Result<(), Err> {
    let (name, url) = match which {
        "altstore" => ("AltStore", "https://cdn.altstore.io/file/altstore/altstore.ipa"),
        _ => ("SideStore", "https://github.com/SideStore/SideStore/releases/latest/download/SideStore.ipa"),
    };
    let dir = home_dir().join(".pairkit");
    tokio::fs::create_dir_all(&dir).await?;
    let pf_path = dir.join("rp_pairing_file.plist");
    if !pf_path.exists() {
        return Err("No pairing file yet. Run ./pairkit first to pair.".into());
    }
    let mut pairing_file = RpPairingFile::read_from_file(&pf_path).await.map_err(e)?;
    let pairing_bytes = pairing_file.to_bytes();

    // 1. Apple ID first, so you can see problems before the tunnel work.
    println!("== Apple ID ==");
    let email = ask("Apple ID email: ");
    let password = ask_secret("Password (hidden, never saved): ");

    let two_factor = |p: TwoFactorCallbackParams| async move {
        if let Some(err) = &p.last_error {
            println!("2FA problem: {err}");
        }
        let code = ask("Enter the 6-digit Apple verification code (or 'r' to resend): ");
        if code.eq_ignore_ascii_case("r") {
            Ok(TwoFactorCallbackResponse::ResendCode)
        } else if code.is_empty() {
            Ok(TwoFactorCallbackResponse::Abort)
        } else {
            Ok::<_, Report>(TwoFactorCallbackResponse::SubmitCode(code))
        }
    };
    println!("Signing in...");
    let mut account = AppleAccountBuilder::new(&email).login(&password, two_factor).await.map_err(e)?;
    let mut dev_session = DeveloperSession::from_account(&mut account).await.map_err(e)?;
    drop(password);

    // 2. Tunnel to the phone.
    println!("== Connecting to this iPhone (LocalDevVPN must be ON) ==");
    let stream = TcpStream::connect((DEVICE_IP, DEVICE_PORT)).await?;
    let mut client = RemotePairingClient::new(RpPairingSocket::new(stream), "pairkit");
    client.attempt_pair_verify().await.map_err(e)?;
    client.validate_pairing(&mut pairing_file).await.map_err(|x| {
        format!("Pairing was rejected ({x:?}). Delete the pair from Settings and run ./pairkit again.")
    })?;
    let port = client.create_tcp_listener().await.map_err(e)?;
    let stream = TcpStream::connect((DEVICE_IP, port)).await?;
    let tunnel = connect_tls_psk_tunnel_native(stream, client.encryption_key()).await.map_err(e)?;
    let info = tunnel.info.clone();
    let host_ip: IpAddr = info.client_address.parse()?;
    let peer_ip: IpAddr = info.server_address.parse()?;
    let adapter = Adapter::new(Box::new(tunnel.into_inner()), host_ip, peer_ip);
    let mut handle = adapter.to_async_handle();
    let rsd_stream = handle.connect(info.server_rsd_port).await?;
    let mut handshake = RsdHandshake::new(rsd_stream).await.map_err(e)?;
    let udid = handshake
        .properties
        .get("UniqueDeviceID")
        .and_then(|v| v.as_string())
        .map(|s| s.to_string())
        .ok_or("The phone did not report its UDID")?;
    println!("Connected. Device {udid}");

    // 3. Register the phone on your account.
    let teams = dev_session.list_teams().await.map_err(e)?;
    let team = teams.into_iter().next().ok_or("No developer team on this Apple ID")?;
    dev_session
        .ensure_device_registered(&team, "iPhone", &udid, None)
        .await
        .map_err(e)?;

    // 4. Download the app.
    let ipa = dir.join(format!("{name}.ipa"));
    println!("== Downloading {name} ==");
    let st = tokio::process::Command::new("wget").arg("-q").arg("-O").arg(&ipa).arg(url).status().await?;
    if !st.success() {
        return Err(format!("Download failed. Try: wget -O {} {url}", ipa.display()).into());
    }

    // 5. Sign. Reuses the certificate pairkit made before; if your account is full, it asks
    //    which certificate to revoke (an old certificate can't be reused without its private key).
    println!("== Signing ==");
    let max_certs = |certs: Vec<DevelopmentCertificate>| async move {
        println!("Your Apple ID has the maximum number of development certificates:");
        for (i, c) in certs.iter().enumerate() {
            println!("  {}: {} ({})", i + 1, c.name.clone().unwrap_or_default(), c.machine_name.clone().unwrap_or_default());
        }
        let a = ask("Type the numbers to revoke (e.g. 1 or 1,2), or just Enter to cancel: ");
        let mut ids = Vec::new();
        for part in a.split(',') {
            if let Ok(n) = part.trim().parse::<usize>() {
                if let Some(c) = certs.get(n.wrapping_sub(1)) {
                    if let Some(id) = &c.certificate_id {
                        ids.push(id.clone());
                    }
                }
            }
        }
        if ids.is_empty() { Ok::<_, Report>(None) } else { Ok(Some(ids)) }
    };
    let ext: ExtensionsBehaviorSelection<ExtCb> =
        ExtensionsBehaviorSelection::Fixed(ExtensionsBehavior::RegisterAll);
    let mut sideloader = SideloaderBuilder::new(dev_session, email.clone())
        .storage(Box::new(FsStorage::new(dir.join("keys"))))
        .machine_name("pairkit".to_string())
        .max_certs_behavior(MaxCertsBehavior::Prompt(max_certs))
        .extensions_behavior(ext)
        .delete_app_after_install(false)
        .build();
    let (signed, _special) = sideloader
        .sign_app(
            ipa.clone(),
            Some(team),
            false,
            Some(|p: f32| async move { println!("  signing {:.0}%", p * 100.0) }),
            Some(&pairing_bytes),
            Some(&udid),
        )
        .await
        .map_err(e)?;

    // 6. Install.
    println!("== Installing ==");
    isideload::sideload::install::install_app_rsd(&mut handle, &mut handshake, &signed, |p| {
        println!("  installing {p}%")
    })
    .await
    .map_err(e)?;
    println!("\nDone. Open Settings > General > VPN & Device Management and trust your Apple ID if {name} won't open.");
    Ok(())
}
