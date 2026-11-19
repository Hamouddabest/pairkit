// Tiny local web page shown in Minis (via `minis-open`). Shows the steps live and asks
// for the Apple ID, password (visible, with a toggle), 2FA code, etc.

use std::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;

struct Prompt {
    label: String,
    secret: bool,
}

#[derive(Default)]
struct St {
    log: Vec<String>,
    prompt: Option<Prompt>,
    answer: Option<String>,
    done: bool,
}

#[derive(Default)]
pub struct Ui {
    st: Mutex<St>,
    notify: Notify,
}

fn esc(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => {}
            c if (c as u32) < 0x20 => {}
            c => o.push(c),
        }
    }
    o
}

impl Ui {
    pub fn log(&self, s: impl Into<String>) {
        let s = s.into();
        println!("{s}");
        self.st.lock().unwrap().log.push(s);
    }

    pub fn finish(&self) {
        self.st.lock().unwrap().done = true;
    }

    pub async fn ask(&self, label: &str, secret: bool) -> String {
        {
            let mut st = self.st.lock().unwrap();
            st.answer = None;
            st.prompt = Some(Prompt { label: label.to_string(), secret });
        }
        loop {
            {
                let mut st = self.st.lock().unwrap();
                if let Some(a) = st.answer.take() {
                    st.prompt = None;
                    return a.trim().to_string();
                }
            }
            self.notify.notified().await;
        }
    }

    fn status_json(&self) -> String {
        let st = self.st.lock().unwrap();
        let log: Vec<String> = st.log.iter().map(|l| format!("\"{}\"", esc(l))).collect();
        let prompt = match &st.prompt {
            Some(p) => format!("{{\"label\":\"{}\",\"secret\":{}}}", esc(&p.label), p.secret),
            None => "null".to_string(),
        };
        format!("{{\"log\":[{}],\"prompt\":{},\"done\":{}}}", log.join(","), prompt, st.done)
    }

    fn submit(&self, v: String) {
        self.st.lock().unwrap().answer = Some(v);
        self.notify.notify_one();
    }

    async fn handle(&self, mut s: TcpStream) {
        let mut buf = Vec::new();
        let mut tmp = [0u8; 4096];
        let head_end;
        loop {
            match s.read(&mut tmp).await {
                Ok(0) | Err(_) => return,
                Ok(n) => buf.extend_from_slice(&tmp[..n]),
            }
            if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                head_end = p + 4;
                break;
            }
            if buf.len() > 65536 {
                return;
            }
        }
        let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
        let first = head.lines().next().unwrap_or("").to_string();
        let clen: usize = head
            .lines()
            .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0)))
            .unwrap_or(0);
        while buf.len() < head_end + clen {
            match s.read(&mut tmp).await {
                Ok(0) | Err(_) => break,
                Ok(n) => buf.extend_from_slice(&tmp[..n]),
            }
        }
        let body = String::from_utf8_lossy(&buf[head_end..]).to_string();
        let (ctype, out) = if first.starts_with("GET /status") {
            ("application/json", self.status_json())
        } else if first.starts_with("POST /answer") {
            self.submit(body);
            ("text/plain", "ok".to_string())
        } else {
            ("text/html; charset=utf-8", PAGE.to_string())
        };
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}",
            out.len()
        );
        let _ = s.write_all(resp.as_bytes()).await;
        let _ = s.shutdown().await;
    }
}

pub async fn serve(ui: std::sync::Arc<Ui>) -> std::io::Result<u16> {
    let l = TcpListener::bind("127.0.0.1:0").await?;
    let port = l.local_addr()?.port();
    tokio::spawn(async move {
        loop {
            if let Ok((s, _)) = l.accept().await {
                let ui = ui.clone();
                tokio::spawn(async move { ui.handle(s).await });
            }
        }
    });
    Ok(port)
}

const PAGE: &str = r##"<!doctype html><html><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>pairkit</title><style>
body{font:16px -apple-system,sans-serif;margin:0;padding:16px;background:#0b0b0f;color:#eee}
h1{font-size:20px;margin:0 0 12px}
#log{background:#15151c;border-radius:12px;padding:12px;white-space:pre-wrap;font:13px ui-monospace,monospace;min-height:120px;max-height:45vh;overflow:auto}
#card{margin-top:14px;background:#1d2a4a;border-radius:12px;padding:14px;display:none}
input[type=text],input[type=password]{width:100%;box-sizing:border-box;padding:12px;font-size:17px;border-radius:10px;border:1px solid #445;background:#0b0b0f;color:#fff;margin:8px 0}
button{width:100%;padding:13px;font-size:17px;border:0;border-radius:10px;background:#3b82f6;color:#fff}
label.s{display:block;margin:4px 0 10px;font-size:14px}
</style></head><body>
<h1>pairkit</h1><div id="log">Starting...</div>
<div id="card"><div id="lbl"></div><input id="v" autocapitalize="off" autocorrect="off" spellcheck="false">
<label class="s" id="sw"><input type="checkbox" id="show"> Show what I type</label>
<button id="go">Send</button></div>
<script>
const $=id=>document.getElementById(id);let cur=null;
$('show').onchange=()=>{$('v').type=$('show').checked?'text':'password'};
async function send(){const v=$('v').value;$('v').value='';$('card').style.display='none';cur=null;
 await fetch('/answer',{method:'POST',body:v});}
$('go').onclick=send;$('v').onkeydown=e=>{if(e.key==='Enter')send()};
async function poll(){try{const r=await fetch('/status');const s=await r.json();
 const l=$('log');const stick=l.scrollTop+l.clientHeight>=l.scrollHeight-30;
 l.textContent=s.log.join('\n')||'Starting...';if(stick)l.scrollTop=l.scrollHeight;
 if(s.prompt&&cur!==s.prompt.label){cur=s.prompt.label;$('card').style.display='block';$('lbl').textContent=s.prompt.label;
  $('v').type=s.prompt.secret?'password':'text';$('show').checked=false;$('sw').style.display=s.prompt.secret?'block':'none';$('v').focus();}
 if(!s.prompt&&cur){cur=null;$('card').style.display='none';}
}catch(e){}setTimeout(poll,1000)}
poll();
</script></body></html>"##;
