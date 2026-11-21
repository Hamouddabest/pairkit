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
use isideload::anisette::remote_v3::RemoteV3AnisetteProvider;
use isideload::auth::builder::AppleAccountBuilder;
use isideload::dev::certificates::DevelopmentCertificate;
use isideload::dev::developer_session::DeveloperSession;
use isideload::dev::app_ids::AppIdsApi;
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


/// Asks for the Apple ID and password, then signs in, trying each anisette server in turn.
async fn apple_login(ui: &Arc<Ui>, dir: &std::path::Path, anisette: &[String]) -> Result<(String, DeveloperSession), Err> {
    let opts_anisette = anisette.to_vec();
    let ui = ui.clone();
    ui.log(format!("== Apple ID =="));
    let email = ui.ask("Apple ID email", false).await;
    let password = ui.ask("Apple ID password (not saved)", true).await;

    // Try each anisette server in turn (the app passes a list); an empty entry means the built-in default.
    let servers: Vec<String> = if opts_anisette.is_empty() { vec![String::new()] } else { opts_anisette.clone() };
    let mut account = None;
    let mut last_err = String::new();
    for url in servers.iter() {
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
        let mut builder = AppleAccountBuilder::new(&email);
        if url.is_empty() {
            ui.log("Signing in...");
        } else {
            ui.log(format!("Signing in (anisette server {url})..."));
            let tag: String = url.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
            let storage = FsStorage::new(dir.join("anisette").join(tag));
            match RemoteV3AnisetteProvider::new(url, Box::new(storage), "0".to_string()) {
                Ok(prov) => builder = builder.anisette_provider(prov),
                Err(err) => {
                    last_err = format!("{err:?}");
                    ui.log(format!("Could not use that server: {last_err}"));
                    continue;
                }
            }
        }
        match builder.login(&password, two_factor).await {
            Ok(a) => {
                account = Some(a);
                break;
            }
            Err(err) => {
                last_err = format!("{err:?}");
                let low = last_err.to_lowercase();
                ui.log(format!("Sign-in failed: {}", last_err.lines().next().unwrap_or("")));
                // Wrong password / cancelled: another server won't help.
                if low.contains("incorrect") || low.contains("-20101") || low.contains("abort") {
                    break;
                }
                ui.log("Trying the next anisette server...");
            }
        }
    }
    let mut account = match account {
        Some(a) => a,
        None => return Err(format!("Could not sign in. {last_err}").into()),
    };
    let dev_session = DeveloperSession::from_account(&mut account).await.map_err(e)?;
    drop(password);
    Ok((email, dev_session))
}

/// Signs in, shows your app IDs (the app answers with the ones to delete) and deletes them.
/// The question label is `APPIDS:<json>`; the answer is a comma-separated list of app ID ids, or empty to finish.
pub async fn manage_app_ids(ui: Arc<Ui>, anisette: Vec<String>) -> Result<(), Err> {
    let dir = home_dir().join(".pairkit");
    tokio::fs::create_dir_all(&dir).await?;
    let (_email, mut dev) = apple_login(&ui, &dir, &anisette).await?;
    let teams = dev.list_teams().await.map_err(e)?;
    let team = teams.into_iter().next().ok_or("No developer team on this Apple ID")?;
    loop {
        let resp = dev.list_app_ids(&team, DeveloperDeviceType::Ios).await.map_err(e)?;
        let ids: Vec<serde_json::Value> = resp
            .app_ids
            .iter()
            .map(|a| {
                serde_json::json!({
                    "id": a.app_id_id,
                    "identifier": a.identifier,
                    "name": a.name,
                    "expires": a.expiration_date.map(|d| d.to_xml_format()),
                })
            })
            .collect();
        let json = serde_json::json!({
            "max": resp.max_quantity,
            "available": resp.available_quantity,
            "ids": ids,
        })
        .to_string();
        let answer = ui.ask(&format!("APPIDS:{json}"), false).await;
        let picked: Vec<String> = answer.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
        if picked.is_empty() {
            break;
        }
        for id in picked {
            match dev.delete_app_id(&team, &id, DeveloperDeviceType::Ios).await {
                Ok(()) => ui.log(format!("Deleted app ID {id}")),
                Err(err) => ui.log(format!("Could not delete {id}: {}", format!("{err:?}").lines().next().unwrap_or(""))),
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Container ("guest app") support: sign an app with your certificate but under PairKit's own
// app ID, so it needs no app ID of its own. The app's executable is turned into a loadable
// library *before* signing, so the signature stays valid.
// ---------------------------------------------------------------------------------------------

fn patch_exec_to_dylib(buf: &mut Vec<u8>) -> Result<(), Err> {
    if buf.len() < 32 {
        return Err("The app's executable is too small to be valid.".into());
    }
    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    match magic {
        0xfeedfacf => {
            // thin 64-bit Mach-O: filetype is at offset 12 (2 = executable, 6 = dylib)
            buf[12..16].copy_from_slice(&6u32.to_le_bytes());
            Ok(())
        }
        0xbebafeca => {
            // fat binary (big-endian header): patch every 64-bit ARM slice
            let n = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
            let mut patched = false;
            for i in 0..n {
                let base = 8 + i * 20;
                if base + 20 > buf.len() {
                    break;
                }
                let cputype = u32::from_be_bytes([buf[base], buf[base + 1], buf[base + 2], buf[base + 3]]);
                let off = u32::from_be_bytes([buf[base + 8], buf[base + 9], buf[base + 10], buf[base + 11]]) as usize;
                if cputype == 0x0100000c && off + 16 <= buf.len() {
                    buf[off + 12..off + 16].copy_from_slice(&6u32.to_le_bytes());
                    patched = true;
                }
            }
            if patched { Ok(()) } else { Err("No arm64 code found in the app.".into()) }
        }
        _ => Err("The app's executable is not a 64-bit iOS binary.".into()),
    }
}

/// Copies the IPA, giving the app `new_bundle_id` and making its executable loadable.
/// Returns (original bundle id, executable name).
fn prepare_guest_ipa(src: &std::path::Path, dst: &std::path::Path, new_bundle_id: &str) -> Result<(String, String), Err> {
    use std::io::{Cursor, Read, Write};
    let mut zin = zip::ZipArchive::new(std::fs::File::open(src)?)?;
    let mut info_name = None;
    for i in 0..zin.len() {
        let n = zin.by_index(i)?.name().to_string();
        if n.starts_with("Payload/") && n.ends_with(".app/Info.plist") && n.matches('/').count() == 2 {
            info_name = Some(n);
            break;
        }
    }
    let info_name = info_name.ok_or("This doesn't look like an IPA (no Info.plist).")?;
    let app_prefix = info_name.trim_end_matches("Info.plist").to_string();

    let mut raw = Vec::new();
    zin.by_name(&info_name)?.read_to_end(&mut raw)?;
    let mut info = plist::Value::from_reader(Cursor::new(&raw)).map_err(|x| format!("Bad Info.plist: {x}"))?;
    let d = info.as_dictionary_mut().ok_or("Bad Info.plist")?;
    let orig = d.get("CFBundleIdentifier").and_then(|v| v.as_string()).unwrap_or("").to_string();
    let exe = d.get("CFBundleExecutable").and_then(|v| v.as_string()).ok_or("The app has no executable name.")?.to_string();
    d.insert("PKGuestOriginalBundleID".into(), plist::Value::String(orig.clone()));
    d.insert("CFBundleIdentifier".into(), plist::Value::String(new_bundle_id.to_string()));
    let mut new_info = Vec::new();
    plist::to_writer_xml(&mut new_info, &info).map_err(|x| format!("Could not write Info.plist: {x}"))?;
    let exe_path = format!("{app_prefix}{exe}");

    let mut zout = zip::ZipWriter::new(std::fs::File::create(dst)?);
    for i in 0..zin.len() {
        let mut f = zin.by_index(i)?;
        let name = f.name().to_string();
        let mode = f.unix_mode().unwrap_or(0o644);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(mode);
        if f.is_dir() {
            zout.add_directory(name, opts)?;
            continue;
        }
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        if name == info_name {
            buf = new_info.clone();
        } else if name == exe_path {
            patch_exec_to_dylib(&mut buf)?;
        }
        zout.start_file(name, opts)?;
        zout.write_all(&buf)?;
    }
    zout.finish()?;
    Ok((orig, exe))
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        let ft = entry.file_type()?;
        if ft.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else if ft.is_symlink() {
            if let Ok(t) = std::fs::read_link(entry.path()) {
                let _ = std::os::unix::fs::symlink(t, &to);
            }
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// Signs `ipa` as a guest of PairKit and puts the result in `dest/App.app`.
/// `host_bundle_id` is PairKit's own bundle id as installed on the phone.
pub async fn sign_guest(ui: Arc<Ui>, ipa: PathBuf, dest: PathBuf, host_bundle_id: String, opts: Opts) -> Result<(), Err> {
    let dir = home_dir().join(".pairkit");
    tokio::fs::create_dir_all(&dir).await?;
    let (email, mut dev_session) = apple_login(&ui, &dir, &opts.anisette).await?;
    let teams = dev_session.list_teams().await.map_err(e)?;
    let team = teams.into_iter().next().ok_or("No developer team on this Apple ID")?;

    // Same app ID as PairKit itself: strip the team suffix the signer adds again.
    let suffix = format!(".{}", team.team_id);
    let base_id = host_bundle_id.strip_suffix(&suffix).unwrap_or(&host_bundle_id).to_string();
    ui.log(format!("== Preparing the app (using PairKit's app ID {base_id}) =="));
    let prepared = dir.join("guest_prepared.ipa");
    let (orig, exe) = prepare_guest_ipa(&ipa, &prepared, &base_id)?;
    ui.log(format!("App {orig}, executable {exe}"));

    ui.log("== Signing ==");
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
                    if let Some(id) = certs.get(n.wrapping_sub(1)).and_then(|c| c.certificate_id.clone()) {
                        ids.push(id);
                    }
                }
            }
            if ids.is_empty() { Ok::<_, Report>(None) } else { Ok(Some(ids)) }
        }
    };
    let ext: ExtensionsBehaviorSelection<ExtCb> = ExtensionsBehaviorSelection::Fixed(ExtensionsBehavior::ReuseMain);
    let mut sideloader = SideloaderBuilder::new(dev_session, email)
        .storage(Box::new(FsStorage::new(dir.join("keys"))))
        .machine_name("pairkit".to_string())
        .max_certs_behavior(MaxCertsBehavior::Prompt(max_certs))
        .extensions_behavior(ext)
        .delete_app_after_install(false)
        .build();
    let ui4 = ui.clone();
    let (signed, _special) = sideloader
        .sign_app(
            prepared.clone(),
            Some(team),
            false,
            Some(move |p: f32| {
                let ui = ui4.clone();
                async move { ui.log(format!("  signing {}%", (p * 100.0) as u64)) }
            }),
            None,
            None,
        )
        .await
        .map_err(e)?;

    let app_dir = dest.join("App.app");
    let _ = std::fs::remove_dir_all(&app_dir);
    std::fs::create_dir_all(&dest)?;
    copy_dir(&signed, &app_dir)?;
    let _ = std::fs::remove_file(&prepared);
    ui.log(format!("Saved to {}", app_dir.display()));
    Ok(())
}

/// If the account has no app IDs left, shows them and offers to delete some.
async fn offer_free_app_ids(ui: &Arc<Ui>, dev: &mut DeveloperSession, team: &isideload::dev::teams::DeveloperTeam) -> Result<(), Err> {
    let resp = dev.list_app_ids(team, DeveloperDeviceType::Ios).await.map_err(e)?;
    if resp.available_quantity.unwrap_or(1) >= 1 {
        return Ok(());
    }
    ui.log("== App IDs ==");
    ui.log("Your Apple ID has no app IDs left (free accounts get about 10 per week). Your app IDs:");
    for (i, a) in resp.app_ids.iter().enumerate() {
        let exp = a.expiration_date.map(|d| d.to_xml_format()).unwrap_or_default();
        ui.log(format!("  {}: {} ({}) {}", i + 1, a.name, a.identifier, exp.chars().take(10).collect::<String>()));
    }
    ui.log("Deleting an app ID makes the app that uses it stop opening until you reinstall it.");
    let a = ui.ask("Type the numbers to DELETE (e.g. 1,2 or all), or leave empty to skip", false).await;
    let a = a.trim().to_string();
    if a.is_empty() {
        return Ok(());
    }
    let mut picked: Vec<String> = Vec::new();
    if a.eq_ignore_ascii_case("all") {
        picked = resp.app_ids.iter().map(|x| x.app_id_id.clone()).collect();
    } else {
        for part in a.split(',') {
            if let Ok(n) = part.trim().parse::<usize>() {
                if let Some(x) = resp.app_ids.get(n.wrapping_sub(1)) {
                    picked.push(x.app_id_id.clone());
                }
            }
        }
    }
    for id in picked {
        match dev.delete_app_id(team, &id, DeveloperDeviceType::Ios).await {
            Ok(()) => ui.log(format!("Deleted app ID {id}")),
            Err(err) => ui.log(format!("Could not delete {id}: {}", format!("{err:?}").lines().next().unwrap_or(""))),
        }
    }
    let after = dev.list_app_ids(team, DeveloperDeviceType::Ios).await.map_err(e)?;
    ui.log(format!("App IDs left now: {}", after.available_quantity.map(|n| n.to_string()).unwrap_or_else(|| "unknown".into())));
    Ok(())
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
    /// Anisette servers to try in order (empty = built-in default)
    pub anisette: Vec<String>,
}

impl Default for Opts {
    fn default() -> Self {
        Opts { ipa: None, cert_mode: "ask".into(), reuse_main_ext: true, anisette: Vec::new() }
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
        _ if preset.is_some() => "P".to_string(),
        "sidestore" => "1".to_string(),
        "altstore" => "2".to_string(),
        "livecontainer" => "3".to_string(),
        "pairkit" => "5".to_string(),
        _ => loop {
            let a = ui
                .ask("Which app? Type 1 = SideStore, 2 = AltStore, 3 = LiveContainer, 4 = custom IPA, 5 = PairKit app", false)
                .await;
            if ["1", "2", "3", "4", "5"].contains(&a.as_str()) {
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
        "5" => "PairKit",
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
    let (email, mut dev_session) = apple_login(&ui, &dir, &opts.anisette).await?;

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
    if choice != "P" {
        ui.log(format!("== Downloading the latest {name} =="));
    }
    if choice == "P" {
        // already downloaded by the app
    } else if choice == "4" && !custom.starts_with("http") {
        tokio::fs::copy(&custom, &ipa).await.map_err(|x| format!("Can't read {custom}: {x}"))?;
    } else {
        let url = match choice.as_str() {
            "1" => github_ipa("SideStore/SideStore", "SideStore.ipa").await,
            "2" => altstore_ipa().await,
            "3" => github_ipa("LiveContainer/LiveContainer", "LiveContainer.ipa").await,
            "5" => Some("https://github.com/Hamouddabest/pairkit/releases/download/app/PairKit.ipa".to_string()),
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
    offer_free_app_ids(&ui, &mut dev_session, &team).await?;
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
