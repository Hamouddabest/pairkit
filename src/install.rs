// Install stage: sign in with an Apple ID, sign SideStore/AltStore, install over the
// on-device tunnel (needs LocalDevVPN on and the pairing file from `pairkit`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::ui::{self, Ui};
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
use isideload::dev::certificates::CertificatesApi;
use isideload::dev::device_type::DeveloperDeviceType;
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

fn e<T: std::fmt::Debug>(x: T) -> Err {
    format!("{x:?}").into()
}

async fn ask_choice(ui: &Arc<Ui>) -> String {
    loop {
        let a = ui.ask("Type 1 to USE the saved certificate, or 2 to REVOKE and make a new one", false).await;
        if a == "1" || a == "2" {
            return a;
        }
    }
}

async fn wget_text(url: &str) -> Option<String> {
    for _ in 0..3 {
        let out = tokio::process::Command::new("wget").args(["-q", "-O", "-", url]).output().await.ok()?;
        if out.status.success() && !out.stdout.is_empty() {
            return String::from_utf8(out.stdout).ok();
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    None
}

// Latest release asset from GitHub: the asset named `prefer`, else any .ipa.
async fn github_ipa(repo: &str, prefer: &str) -> Option<String> {
    let txt = wget_text(&format!("https://api.github.com/repos/{repo}/releases/latest")).await;
    if let Some(txt) = txt {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
            let assets = v.get("assets")?.as_array()?;
            let pick = assets
                .iter()
                .find(|a| a.get("name").and_then(|n| n.as_str()) == Some(prefer))
                .or_else(|| assets.iter().find(|a| a.get("name").and_then(|n| n.as_str()).map_or(false, |n| n.ends_with(".ipa"))));
            if let Some(u) = pick.and_then(|a| a.get("browser_download_url")).and_then(|u| u.as_str()) {
                return Some(u.to_string());
            }
        }
    }
    Some(format!("https://github.com/{repo}/releases/latest/download/{prefer}"))
}

// AltStore's official source lists versions newest first.
async fn altstore_ipa() -> Option<String> {
    let txt = wget_text("https://cdn.altstore.io/file/altstore/apps.json").await;
    if let Some(txt) = txt {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
            let app = v
                .get("apps")?
                .as_array()?
                .iter()
                .find(|a| a.get("bundleIdentifier").and_then(|b| b.as_str()) == Some("com.rileytestut.AltStore"));
            let u = app
                .and_then(|a| a.get("versions"))
                .and_then(|vs| vs.as_array())
                .and_then(|vs| vs.first())
                .and_then(|x| x.get("downloadURL"))
                .and_then(|u| u.as_str());
            if let Some(u) = u {
                return Some(u.to_string());
            }
            let u = app.and_then(|a| a.get("downloadURL")).and_then(|u| u.as_str());
            if let Some(u) = u {
                return Some(u.to_string());
            }
        }
    }
    Some("https://cdn.altstore.io/file/altstore/altstore.ipa".to_string())
}

type ExtCb = fn(&[Bundle]) -> std::future::Ready<Result<ExtensionsBehavior, Report>>;

/// Options used by the iOS app (the CLI uses the defaults).
#[derive(Clone)]
pub struct Opts {
    /// Already-downloaded IPA to install (skips the app question and the download).
    pub ipa: Option<PathBuf>,
    /// "ask", "reuse" or "new"
    pub cert_mode: String,
    /// true = extensions share the main app ID (uses fewer app IDs)
    pub reuse_main_ext: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts { ipa: None, cert_mode: "ask".into(), reuse_main_ext: true }
    }
}

pub async fn run(which: &str) -> Result<(), Err> {
    // reqwest (used by isideload) needs a rustls crypto provider to be installed first.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let ui = Arc::new(Ui::default());
    let port = ui::serve(ui.clone()).await?;
    let url = format!("http://127.0.0.1:{port}/");
    ui.log(format!("Opening the pairkit page: {url}"));
    let opened = tokio::process::Command::new("minis-open").arg(&url).status().await.map(|s| s.success()).unwrap_or(false);
    if !opened {
        ui.log(format!("Could not open it automatically. Open this address in a browser: {url}"));
    }
    let res = run_inner(which, ui.clone(), Opts::default()).await;
    match &res {
        Ok(()) => ui.log("All done."),
        Err(err) => ui.log(format!("ERROR: {err}")),
    }
    ui.finish();
    tokio::time::sleep(std::time::Duration::from_secs(15)).await;
    res
}

pub async fn run_with(ui: Arc<Ui>, opts: Opts) -> Result<(), Err> {
    run_inner("", ui, opts).await
}

async fn run_inner(which: &str, ui: Arc<Ui>, opts: Opts) -> Result<(), Err> {
    let preset = opts.ipa.clone();
    let choice = match which {
        _ if preset.is_some() => "5".to_string(),
        "sidestore" => "1".to_string(),
        "altstore" => "2".to_string(),
        "livecontainer" => "3".to_string(),
        _ => loop {
            let a = ui
                .ask("Which app? Type 1 = SideStore, 2 = AltStore, 3 = LiveContainer, 4 = custom IPA", false)
                .await;
            if ["1", "2", "3", "4"].contains(&a.as_str()) {
                break a;
            }
        },
    };
    let mut custom = String::new();
    if choice == "4" {
        custom = ui.ask("Paste the IPA link (https://...) or a file path", false).await;
    }
    let name = match choice.as_str() {
        "1" => "SideStore",
        "2" => "AltStore",
        "3" => "LiveContainer",
        _ => "CustomApp",
    }
    .to_string();
    let name = match &preset {
        Some(p) => p.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "App".to_string()),
        None => name,
    };
    let dir = home_dir().join(".pairkit");
    tokio::fs::create_dir_all(&dir).await?;
    let pf_path = dir.join("rp_pairing_file.plist");
    if !pf_path.exists() {
        return Err("No pairing file yet. Run ./pairkit first to pair.".into());
    }
    let mut pairing_file = RpPairingFile::read_from_file(&pf_path).await.map_err(e)?;
    let pairing_bytes = pairing_file.to_bytes();
    ui.set_pairing(pairing_bytes.clone());

    // 1. Apple ID first, so you can see problems before the tunnel work.
    ui.log(format!("== Apple ID =="));
    let email = ui.ask("Apple ID email", false).await;
    let password = ui.ask("Apple ID password (not saved)", true).await;

    let ui2 = ui.clone();
    let two_factor = move |p: TwoFactorCallbackParams| {
        let ui = ui2.clone();
        async move {
        if let Some(err) = &p.last_error {
            ui.log(format!("2FA problem: {err}"));
        }
        let code = ui.ask("Enter the 6-digit Apple verification code (type r to resend)", false).await;
        if code.eq_ignore_ascii_case("r") {
            Ok(TwoFactorCallbackResponse::ResendCode)
        } else if code.is_empty() {
            Ok(TwoFactorCallbackResponse::Abort)
        } else {
            Ok::<_, Report>(TwoFactorCallbackResponse::SubmitCode(code))
        }
        }
    };
    ui.log(format!("Signing in..."));
    let mut account = AppleAccountBuilder::new(&email).login(&password, two_factor).await.map_err(e)?;
    let mut dev_session = DeveloperSession::from_account(&mut account).await.map_err(e)?;
    drop(password);

    // 2. Tunnel to the phone.
    ui.log(format!("== Connecting to this iPhone (LocalDevVPN must be ON) =="));
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
    ui.log(format!("Connected. Device {udid}"));

    // 3. Register the phone on your account.
    let teams = dev_session.list_teams().await.map_err(e)?;
    let team = teams.into_iter().next().ok_or("No developer team on this Apple ID")?;
    dev_session
        .ensure_device_registered(&team, "iPhone", &udid, None)
        .await
        .map_err(e)?;

    // 4. Download the app.
    let ipa = preset.clone().unwrap_or_else(|| dir.join(format!("{name}.ipa")));
    if choice != "5" {
        ui.log(format!("== Downloading the latest {name} =="));
    }
    if choice == "5" {
        // already downloaded by the app
    } else if choice == "4" && !custom.starts_with("http") {
        tokio::fs::copy(&custom, &ipa).await.map_err(|x| format!("Can't read {custom}: {x}"))?;
    } else {
        let url = match choice.as_str() {
            "1" => github_ipa("SideStore/SideStore", "SideStore.ipa").await,
            "2" => altstore_ipa().await,
            "3" => github_ipa("LiveContainer/LiveContainer", "LiveContainer.ipa").await,
            _ => Some(custom.clone()),
        };
        let url = match url {
            Some(u) => u,
            None => return Err("Couldn't find the latest download link (network problem?). Try again.".into()),
        };
        ui.log(format!("  from {url}"));
        let st = tokio::process::Command::new("wget").arg("-q").arg("-O").arg(&ipa).arg(&url).status().await?;
        if !st.success() {
            return Err(format!("Download failed. Try: wget -O {} {url}", ipa.display()).into());
        }
    }

    // 5. Sign. Reuses the certificate pairkit made before; if your account is full, it asks
    //    which certificate to revoke (an old certificate can't be reused without its private key).
    ui.log(format!("== Certificate =="));
    let certs = dev_session.list_ios_certs(&team).await.map_err(e)?;
    let keys_dir = dir.join("keys");
    if certs.is_empty() {
        ui.log("No certificate on your account yet; a new one will be made.");
    } else {
        ui.log("Certificates on your Apple ID:");
        for (i, c) in certs.iter().enumerate() {
            ui.log(format!("  {}: {} ({})", i + 1, c.name.clone().unwrap_or_default(), c.machine_name.clone().unwrap_or_default()));
        }
        ui.log("Note: a certificate can only be reused if pairkit made it earlier (it needs the saved key).");
        let a = match opts.cert_mode.as_str() {
            "reuse" => "1".to_string(),
            "new" => "2".to_string(),
            _ => ask_choice(&ui).await,
        };
        if a == "2" {
            let which = ui.ask("Which to revoke? Numbers like 1 or 1,2, or type all", false).await;
            let mut serials = Vec::new();
            for part in which.split(',') {
                let part = part.trim();
                if part.eq_ignore_ascii_case("all") {
                    serials = certs.iter().filter_map(|c| c.serial_number.clone()).collect();
                    break;
                }
                if let Ok(n) = part.parse::<usize>() {
                    if let Some(sn) = certs.get(n.wrapping_sub(1)).and_then(|c| c.serial_number.clone()) {
                        serials.push(sn);
                    }
                }
            }
            for sn in &serials {
                dev_session.revoke_development_cert(&team, sn, DeveloperDeviceType::Ios).await.map_err(e)?;
                ui.log(format!("Revoked certificate {sn}"));
            }
            let _ = tokio::fs::remove_dir_all(&keys_dir).await;
            ui.log("A new certificate will be made.");
        } else {
            ui.log("Reusing the saved certificate if it matches.");
        }
    }
    ui.log(format!("== Signing =="));
    let ui3 = ui.clone();
    let max_certs = move |certs: Vec<DevelopmentCertificate>| {
        let ui = ui3.clone();
        async move {
        ui.log("Your Apple ID has the maximum number of development certificates:");
        for (i, c) in certs.iter().enumerate() {
            ui.log(format!("  {}: {} ({})", i + 1, c.name.clone().unwrap_or_default(), c.machine_name.clone().unwrap_or_default()));
        }
        let a = ui.ask("Type the numbers to revoke (e.g. 1 or 1,2), or leave empty to cancel", false).await;
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
        }
    };
    let ext: ExtensionsBehaviorSelection<ExtCb> =
        ExtensionsBehaviorSelection::Fixed(if opts.reuse_main_ext { ExtensionsBehavior::ReuseMain } else { ExtensionsBehavior::RegisterAll });
    let mut sideloader = SideloaderBuilder::new(dev_session, email.clone())
        .storage(Box::new(FsStorage::new(dir.join("keys"))))
        .machine_name("pairkit".to_string())
        .max_certs_behavior(MaxCertsBehavior::Prompt(max_certs))
        .extensions_behavior(ext)
        .delete_app_after_install(false)
        .build();
    let mut attempt = 0;
    let (signed, _special) = loop {
        attempt += 1;
        let ui4 = ui.clone();
        let last4 = Arc::new(AtomicU64::new(0));
        let res = sideloader
            .sign_app(
                ipa.clone(),
                Some(team.clone()),
                false,
                Some(move |p: f32| {
                    let ui = ui4.clone();
                    let last = last4.clone();
                    async move {
                        let pct = (p * 100.0) as u64;
                        if pct >= last.load(Ordering::Relaxed) + 20 || pct == 100 {
                            last.store(pct, Ordering::Relaxed);
                            ui.log(format!("  signing {pct}%"));
                        }
                    }
                }),
                Some(&pairing_bytes),
                Some(&udid),
            )
            .await;
        match res {
            Ok(v) => break v,
            Err(err) => {
                let msg = format!("{err:?}");
                let net = msg.contains("dns error") || msg.contains("Try again") || msg.contains("Connect");
                if net && attempt < 5 {
                    ui.log(format!("Network hiccup talking to Apple (try {attempt}/5). Retrying in 5s..."));
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
                return Err(e(msg));
            }
        }
    };

    // 6. Install.
    ui.log(format!("== Installing =="));
    let ui5 = ui.clone();
    let last5 = AtomicU64::new(0);
    isideload::sideload::install::install_app_rsd(&mut handle, &mut handshake, &signed, |p| {
        let last = last5.load(Ordering::Relaxed);
        if p >= last + 10 || p == 100 {
            last5.store(p, Ordering::Relaxed);
            ui5.log(format!("  installing {p}%"));
        }
    })
    .await
    .map_err(e)?;
    ui.log(format!("Done. Open Settings > General > VPN & Device Management and trust your Apple ID if {name} won't open."));
    Ok(())
}
