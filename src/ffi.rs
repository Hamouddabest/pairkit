// C interface used by the iOS app (see ios/PairKit/pairkit.h).

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use idevice::remote_pairing::{PairableHost, PairableHostInfo, RpPairingFile, RpPairingSocket};
use tokio::net::TcpListener;

use crate::install::{self, Opts};
use crate::ui::Ui;

type LogCb = extern "C" fn(*const c_char, *mut c_void);
type AskCb = extern "C" fn(*const c_char, i32, *mut c_void) -> *mut c_char;
type PinCb = extern "C" fn(*const c_char, *mut c_void);
type DoneCb = extern "C" fn(i32, *const c_char, *mut c_void);

extern "C" {
    fn free(p: *mut c_void);
}

#[derive(Clone, Copy)]
struct Ctx(*mut c_void);
unsafe impl Send for Ctx {}
unsafe impl Sync for Ctx {}

fn rt() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
        tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_all().build().expect("runtime")
    })
}

fn cstr(s: &str) -> CString {
    CString::new(s.replace('\0', " ")).unwrap()
}

fn home_dir() -> PathBuf {
    std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."))
}

static PAIR_TASK: Mutex<Option<tokio::task::JoinHandle<()>>> = Mutex::new(None);

/// Frees a string returned by this library.
#[no_mangle]
pub extern "C" fn pk_free(p: *mut c_char) {
    if !p.is_null() {
        unsafe { drop(CString::from_raw(p)) };
    }
}

/// Starts the "pair with this iPhone" responder. Returns JSON
/// {"identifier":"..","port":N,"txt":{"k":"v",..}} for the app to publish over Bonjour
/// as _remotepairing-pairable-host._tcp, or null on failure.
/// `pin` is called with the 6-digit code; `done(ok, message)` when finished.
#[no_mangle]
pub extern "C" fn pk_pair_start(log: LogCb, pin: PinCb, done: DoneCb, ctx: *mut c_void) -> *mut c_char {
    let ctx = Ctx(ctx);
    let say = move |s: &str| {
        let c = cstr(s);
        log(c.as_ptr(), ctx.0);
    };
    let res: Result<(String, u16, Vec<(String, String)>, tokio::net::TcpListener, RpPairingFile, PairableHostInfo), String> =
        rt().block_on(async {
            let pairing_file = RpPairingFile::generate("pairkit");
            let mut info = PairableHostInfo::generate("pairkit", "Mac17,7");
            info.identifier = pairing_file.identifier.clone();
            let txt: Vec<(String, String)> = info
                .mdns_txt_records(&pairing_file.identifier)
                .iter()
                .map(|(k, v)| (format!("{k}"), format!("{v}")))
                .collect();
            let l = TcpListener::bind("0.0.0.0:0").await.map_err(|e| e.to_string())?;
            let port = l.local_addr().map_err(|e| e.to_string())?.port();
            Ok((pairing_file.identifier.clone(), port, txt, l, pairing_file, info))
        });
    let (identifier, port, txt, listener, mut pairing_file, info) = match res {
        Ok(v) => v,
        Err(e) => {
            say(&format!("Could not start pairing: {e}"));
            return std::ptr::null_mut();
        }
    };

    let out_dir = home_dir().join(".pairkit");
    let handle = rt().spawn(async move {
        let _ = tokio::fs::create_dir_all(&out_dir).await;
        let mut last_err = "nobody connected, or the code was entered wrong 3 times".to_string();
        for attempt in 1..=3 {
            let accepted = tokio::time::timeout(Duration::from_secs(600), listener.accept()).await;
            let stream = match accepted {
                Ok(Ok((s, _))) => s,
                Ok(Err(e)) => {
                    last_err = e.to_string();
                    break;
                }
                Err(_) => {
                    last_err = "timed out waiting for the iPhone".to_string();
                    break;
                }
            };
            say(&format!("Connected (attempt {attempt})."));
            let socket = RpPairingSocket::new_device(stream);
            let mut host = PairableHost::new(socket, info.clone());
            let pin_ctx = ctx;
            let r = host
                .accept(&mut pairing_file, move |p| async move {
                    let c = cstr(&format!("{p}"));
                    pin(c.as_ptr(), pin_ctx.0);
                })
                .await;
            match r {
                Ok(_) => {
                    let bytes = pairing_file.to_bytes();
                    let path = out_dir.join("rp_pairing_file.plist");
                    match tokio::fs::write(&path, &bytes).await {
                        Ok(()) => {
                            let c = cstr(&path.to_string_lossy());
                            done(1, c.as_ptr(), ctx.0);
                        }
                        Err(e) => {
                            let c = cstr(&format!("could not save the pairing file: {e}"));
                            done(0, c.as_ptr(), ctx.0);
                        }
                    }
                    return;
                }
                Err(e) => say(&format!("Pairing attempt failed: {e:?}. Tap the entry in Settings and try again.")),
            }
        }
        let c = cstr(&last_err);
        done(0, c.as_ptr(), ctx.0);
    });
    *PAIR_TASK.lock().unwrap() = Some(handle);

    let txt_json: serde_json::Map<String, serde_json::Value> =
        txt.into_iter().map(|(k, v)| (k, serde_json::Value::String(v))).collect();
    let json = serde_json::json!({"identifier": identifier, "port": port, "txt": txt_json}).to_string();
    cstr(&json).into_raw()
}

/// Stops a running pairing responder.
#[no_mangle]
pub extern "C" fn pk_pair_stop() {
    if let Some(h) = PAIR_TASK.lock().unwrap().take() {
        h.abort();
    }
}

/// Signs and installs an already-downloaded IPA. Blocks until finished; call from a background thread.
/// `opts_json`: {"cert_mode":"ask|reuse|new","reuse_main_ext":true}
/// Returns null on success or an error message (free with pk_free).
#[no_mangle]
pub extern "C" fn pk_install(ipa_path: *const c_char, opts_json: *const c_char, log: LogCb, ask: AskCb, ctx: *mut c_void) -> *mut c_char {
    let ctx = Ctx(ctx);
    let ipa = unsafe { CStr::from_ptr(ipa_path) }.to_string_lossy().to_string();
    let opts_s = if opts_json.is_null() { "{}".to_string() } else { unsafe { CStr::from_ptr(opts_json) }.to_string_lossy().to_string() };
    let v: serde_json::Value = serde_json::from_str(&opts_s).unwrap_or_default();
    let opts = Opts {
        ipa: Some(PathBuf::from(ipa)),
        cert_mode: v.get("cert_mode").and_then(|x| x.as_str()).unwrap_or("ask").to_string(),
        reuse_main_ext: v.get("reuse_main_ext").and_then(|x| x.as_bool()).unwrap_or(true),
    };

    let ui = Arc::new(Ui::default());
    ui.set_hooks(
        Arc::new(move |s: &str| {
            let c = cstr(s);
            log(c.as_ptr(), ctx.0);
        }),
        Arc::new(move |label: &str, secret: bool| {
            let c = cstr(label);
            let p = ask(c.as_ptr(), secret as i32, ctx.0);
            if p.is_null() {
                return String::new();
            }
            let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().to_string();
            unsafe { free(p as *mut c_void) };
            s
        }),
    );
    match rt().block_on(install::run_with(ui, opts)) {
        Ok(()) => std::ptr::null_mut(),
        Err(e) => cstr(&e.to_string()).into_raw(),
    }
}
