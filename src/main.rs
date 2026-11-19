// pairkit: pairs an iOS 27 iPhone with itself from Open Minis and saves a pairing file.
// No signing, no installing. Written against idevice 0.1.64, not yet compiled:
// if the build fails, paste the error and it will be fixed.

use std::path::PathBuf;
use std::time::Duration;

use idevice::remote_pairing::{PairableHost, PairableHostInfo, RpPairingFile, RpPairingSocket};
use tokio::net::TcpListener;
use tokio::process::Command;

const SERVICE_TYPE: &str = "_remotepairing-pairable-host._tcp";

fn home_dir() -> PathBuf {
    std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."))
}

// 8 kHz, 8-bit mono WAV of silence. Played so iOS keeps Minis awake while you are in Settings.
fn silence_wav(seconds: u32) -> Vec<u8> {
    let rate: u32 = 8000;
    let n = rate * seconds;
    let mut v = Vec::with_capacity(44 + n as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + n).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes()); // PCM
    v.extend_from_slice(&1u16.to_le_bytes()); // mono
    v.extend_from_slice(&rate.to_le_bytes());
    v.extend_from_slice(&rate.to_le_bytes()); // byte rate
    v.extend_from_slice(&1u16.to_le_bytes()); // block align
    v.extend_from_slice(&8u16.to_le_bytes()); // bits
    v.extend_from_slice(b"data");
    v.extend_from_slice(&n.to_le_bytes());
    v.resize(44 + n as usize, 0x80);
    v
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("\nError: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let dir = home_dir().join(".pairkit");
    tokio::fs::create_dir_all(&dir).await?;
    let out_path = dir.join("rp_pairing_file.plist");

    // Identity for this pairing.
    let mut pairing_file = RpPairingFile::generate("pairkit");
    let mut info = PairableHostInfo::generate("pairkit", "Mac17,7");
    info.identifier = pairing_file.identifier.clone();
    let txt = info.mdns_txt_records(&pairing_file.identifier);

    // Local port that Minis forwards incoming connections to.
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    // Advertise over Bonjour through Minis: minis-bonjour advertise <type> <name> <port> k=v ...
    let mut cmd = Command::new("minis-bonjour");
    cmd.arg("advertise")
        .arg(SERVICE_TYPE)
        .arg(&pairing_file.identifier)
        .arg(port.to_string());
    for (k, v) in &txt {
        cmd.arg(format!("{k}={v}"));
    }
    let adv = cmd.output().await.map_err(|e| format!("could not run minis-bonjour: {e}"))?;
    let adv_out = format!(
        "{}{}",
        String::from_utf8_lossy(&adv.stdout),
        String::from_utf8_lossy(&adv.stderr)
    );
    if !adv.status.success() {
        return Err(format!("minis-bonjour advertise failed:\n{adv_out}").into());
    }
    let fwd_id: Option<String> = adv_out
        .split(|c: char| c.is_whitespace() || c == '"' || c == ',' || c == ':')
        .find(|t| t.starts_with("fwd-"))
        .map(|s| s.to_string());
    println!("Advertising on port {port}. {}", adv_out.trim());

    // Keep Minis awake while you are in Settings (best effort).
    let wav = dir.join("silence.wav");
    if tokio::fs::write(&wav, silence_wav(600)).await.is_ok() {
        let _ = Command::new("apple-player").arg("play").arg(&wav).spawn();
    }

    println!();
    println!("Now open: Settings > Privacy & Security > Developer Mode");
    println!("Tap \"Pair with pairkit\". The 6-digit code is shown below.");
    println!("Waiting up to 5 minutes...\n");

    let mut result: Result<(), Box<dyn std::error::Error>> =
        Err("nobody connected, or the code was entered wrong 3 times".into());

    for attempt in 1..=3 {
        let accepted = tokio::time::timeout(Duration::from_secs(300), listener.accept()).await;
        let (stream, _) = match accepted {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                result = Err(e.into());
                break;
            }
            Err(_) => {
                result = Err("timed out waiting for the iPhone".into());
                break;
            }
        };
        println!("iPhone connected (attempt {attempt}).");

        let socket = RpPairingSocket::new_device(stream);
        let mut host = PairableHost::new(socket, info.clone());
        match host
            .accept(&mut pairing_file, |pin| async move {
                println!("\n>>> PAIRING CODE: {pin} <<<\n");
            })
            .await
        {
            Ok(_peer) => {
                // `to_bytes` is a guess at idevice's serializer name.
                let bytes = pairing_file.to_bytes();
                tokio::fs::write(&out_path, bytes).await?;
                println!("Paired. Pairing file saved to {}", out_path.display());
                result = Ok(());
                break;
            }
            Err(e) => {
                println!("Pairing attempt failed: {e:?}. Tap the entry in Settings and try again.");
            }
        }
    }

    // Clean up: stop the advertisement and the silence.
    if let Some(id) = fwd_id {
        let _ = Command::new("minis-bonjour").arg("stop").arg(id).output().await;
    }
    let _ = Command::new("apple-player").arg("stop").output().await;

    result
}
