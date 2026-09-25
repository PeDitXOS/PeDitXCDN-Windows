//! Run sing-box as a sidecar and keep the promise the plan makes: it exists
//! only while one of the ticked apps is alive, and nothing of ours depends
//! on it staying alive.
//!
//! Three jobs: spawn the child with its generated config, watch it (exit,
//! stall, or no listed app left running), and read clash_api back so the UI
//! can show *which* processes are actually on the tunnel — "it says on" is
//! not evidence.

use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;

use crate::api::log_to_file;
use crate::singconf::{self, Inject, Opts};

/// Loopback Clash API the generated config opens. Never `0.0.0.0` — a
/// controller with no secret is a control plane for anyone on the LAN.
pub const CLASH_API: &str = "127.0.0.1:9090";

/// How often the watcher re-checks. 2 s is fast enough that a crashed
/// game's tunnel goes away before the user notices, slow enough not to cost.
const WATCH_MS: u64 = 2_000;

/// Seconds with no listed app running before the tunnel is torn down. A
/// launcher can take longer than one tick to fork the game, and killing the
/// tunnel in between would be the bug all over again.
const GRACE_SECS: u64 = 10;

static RUNNING: AtomicBool = AtomicBool::new(false);
/// Bumped on every start so a watcher from a previous run knows to exit
/// rather than tear down its successor.
static GENERATION: AtomicU64 = AtomicU64::new(0);
static CHILD: Mutex<Option<Child>> = Mutex::new(None);
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

/// The user's raw pasted body. Kept so `add`/`remove` can rebuild the config
/// without asking them to paste it twice.
static BODY: Mutex<String> = Mutex::new(String::new());
static APP_PATHS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static INSTALL_ROOTS: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// App path → outbound tag. Lives in memory with the app list itself: the
/// two are lost and re-picked together, never half of each.
static APP_ROUTE: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
/// Tags the user switched off in the config list.
static CFG_OFF: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Where the generated config lives. Next to our own debug.log so a support
/// request can be answered from one directory — and it holds no keys the
/// user did not paste themselves.
pub fn config_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("PeDitXCDN")
}

fn generated_path() -> PathBuf {
    config_dir().join("tunnel.json")
}

fn raw_path() -> PathBuf {
    config_dir().join("tunnel.raw")
}

// --- inputs ---------------------------------------------------------------

/// Store the pasted body (raw, for re-reading) and rebuild the generated
/// config from it. Nothing is started here — a stored config is not a
/// running tunnel.
pub fn set_config(body: &str) -> Result<(), String> {
    let body = body.trim().to_string();
    if body.is_empty() {
        return Err("کانفیگ خالی است.".into());
    }
    // Validate before storing: a body that cannot convert must not replace a
    // config that works.
    let inj = inject();
    singconf::build(&body, &inj)?;
    *BODY.lock().unwrap() = body.clone();
    std::fs::create_dir_all(config_dir()).map_err(|e| e.to_string())?;
    std::fs::write(raw_path(), &body).map_err(|e| e.to_string())?;
    // The new body may name different tags than the old one. Dropping the
    // choices it no longer carries keeps the page from offering a config
    // that is not there; those apps fall back to the first one themselves.
    let known = singconf::tags(&body);
    APP_ROUTE.lock().unwrap().retain(|(_, t)| known.iter().any(|k| k == t));
    CFG_OFF.lock().unwrap().retain(|t| known.iter().any(|k| k == t));
    write_generated(&inj)
}

// --- configs and routing --------------------------------------------------

/// Which outbound one ticked app rides. Empty tag = the first one on.
pub fn set_route(path: String, tag: String) -> Result<(), String> {
    let p = path.trim().to_string();
    let t = tag.trim().to_string();
    if !apps().iter().any(|a| a.eq_ignore_ascii_case(&p)) {
        return Err("این برنامه در لیست نیست.".into());
    }
    if !t.is_empty() {
        let known = singconf::tags(&body());
        if known.is_empty() {
            return Err("اول کانفیگ sing-box را ثبت کنید.".into());
        }
        if !known.iter().any(|k| k == &t) {
            return Err(format!("کانفیگ «{t}» در کانفیگ فعلی نیست."));
        }
        if CFG_OFF.lock().unwrap().iter().any(|o| o == &t) {
            return Err(format!("کانفیگ «{t}» خاموش است؛ اول روشنش کن."));
        }
    }
    let mut routes = APP_ROUTE.lock().unwrap();
    routes.retain(|(a, _)| !a.eq_ignore_ascii_case(&p));
    if !t.is_empty() {
        routes.push((p, t));
    }
    drop(routes);
    reapply()
}

/// The switch beside every config in the list.
pub fn set_cfg(tag: String, on: bool) -> Result<(), String> {
    let t = tag.trim().to_string();
    let known = singconf::tags(&body());
    if !known.iter().any(|k| k == &t) {
        return Err("چنین کانفیگی در کانفیگ فعلی نیست.".into());
    }
    let mut off = CFG_OFF.lock().unwrap();
    if on {
        off.retain(|o| o != &t);
    } else {
        let others = known
            .iter()
            .filter(|k| **k != t && !off.iter().any(|o| o == *k))
            .count();
        if others == 0 {
            return Err("آخرین کانفیگ روشن را نمی‌توان خاموش کرد.".into());
        }
        if !off.iter().any(|o| o == &t) {
            off.push(t.clone());
        }
        // An app parked on this config would otherwise show a choice the
        // page no longer offers.
        APP_ROUTE.lock().unwrap().retain(|(_, tg)| tg != &t);
    }
    drop(off);
    reapply()
}

pub fn add_app(path: String) -> Result<(), String> {
    let p = path.trim().trim_matches('"').to_string();
    if p.is_empty() {
        return Err("مسیر خالی است.".into());
    }
    if !p.to_ascii_lowercase().ends_with(".exe") {
        return Err("باید مسیر یک فایل ‎.exe‎ باشد.".into());
    }
    if !std::path::Path::new(&p).exists() {
        return Err("چنین فایلی وجود ندارد.".into());
    }
    let mut apps = APP_PATHS.lock().unwrap();
    if !apps.iter().any(|a| a.eq_ignore_ascii_case(&p)) {
        apps.push(p.clone());
    }
    drop(apps);
    // Children of the install root ride along with the same rule.
    let root = std::path::Path::new(&p)
        .parent()
        .map(|d| d.to_string_lossy().to_string())
        .unwrap_or_default();
    if !root.is_empty() {
        let mut roots = INSTALL_ROOTS.lock().unwrap();
        if !roots.iter().any(|r| r.eq_ignore_ascii_case(&root)) {
            roots.push(root);
        }
    }
    rebuild()
}

pub fn remove_app(path: String) -> Result<(), String> {
    APP_PATHS.lock().unwrap().retain(|a| !a.eq_ignore_ascii_case(path.trim()));
    rebuild()
}

pub fn apps() -> Vec<String> {
    APP_PATHS.lock().unwrap().clone()
}

fn inject() -> Inject {
    let opts = opts();
    // clash_api is read through this same static, so the secret the config
    // gets and the secret the UI authenticates with cannot drift apart.
    *SECRET.lock().unwrap() = opts.clash_secret.clone();
    Inject {
        // Both are supplied by the caller at start time; empty here is fine
        // for validation, which only needs the body to convert.
        relay_ip: RELAY_IP.lock().unwrap().clone(),
        panel_ip: PANEL_IP.lock().unwrap().clone(),
        app_paths: APP_PATHS.lock().unwrap().clone(),
        install_roots: INSTALL_ROOTS.lock().unwrap().clone(),
        bind_interface: BIND_IFACE.lock().unwrap().clone(),
        opts,
        app_route: APP_ROUTE.lock().unwrap().clone(),
        cfg_off: CFG_OFF.lock().unwrap().clone(),
    }
}

static RELAY_IP: Mutex<String> = Mutex::new(String::new());
static PANEL_IP: Mutex<String> = Mutex::new(String::new());
static BIND_IFACE: Mutex<String> = Mutex::new(String::new());

// --- settings -------------------------------------------------------------

fn opts_path() -> PathBuf {
    config_dir().join("tunnel.opts.json")
}

/// Memory first (fast path for the settings page), then disk — a settings
/// file written by an earlier run must survive a restart the same way the
/// pasted body does.
pub fn opts() -> Opts {
    let mut slot = OPTS.lock().unwrap();
    if slot.is_none() {
        let disk = std::fs::read_to_string(opts_path())
            .ok()
            .and_then(|s| serde_json::from_str::<Opts>(&s).ok());
        *slot = Some(disk.unwrap_or_default());
    }
    slot.clone().unwrap_or_default()
}

/// Store the settings, rebuild the generated config, and restart a live
/// tunnel so the change is real rather than only written down.
pub fn set_opts(next: Opts) -> Result<(), String> {
    next.validate()?;
    let json = serde_json::to_string_pretty(&next).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(config_dir()).map_err(|e| e.to_string())?;
    // Disk first: if this fails, memory must not claim something saved.
    std::fs::write(opts_path(), json).map_err(|e| e.to_string())?;
    *OPTS.lock().unwrap() = Some(next);
    reapply()
}

/// Regenerate the config from what is stored and, if a child is up, put that
/// child on the new file. Shared by the settings page, the routing page and
/// the config switches — one place that decides whether a click is real.
fn reapply() -> Result<(), String> {
    if body().is_empty() {
        return Ok(()); // nothing to regenerate yet
    }
    rebuild()?;

    if RUNNING.load(Ordering::SeqCst) {
        let relay = RELAY_IP.lock().unwrap().clone();
        let panel = PANEL_IP.lock().unwrap().clone();
        stop("settings changed")?;
        start(relay, panel)?;
    }
    Ok(())
}

/// What sing-box would be handed right now, pretty-printed. This is the
/// only honest "what did my settings do" — the UI never renders the config
/// itself, it shows what was actually written.
pub fn preview() -> Result<String, String> {
    if body().is_empty() {
        return Err("اول کانفیگ sing-box را ثبت کنید.".into());
    }
    singconf::build(&body(), &inject())
}

static OPTS: Mutex<Option<Opts>> = Mutex::new(None);

/// The pasted body, read back from disk after a restart. Without this a
/// config that exists on disk but not in memory refuses to start, and
/// adding an app silently writes nothing.
fn body() -> String {
    let b = BODY.lock().unwrap().clone();
    if !b.is_empty() {
        return b;
    }
    let disk = std::fs::read_to_string(raw_path())
        .unwrap_or_default()
        .trim()
        .to_string();
    if !disk.is_empty() {
        *BODY.lock().unwrap() = disk.clone();
    }
    disk
}

fn rebuild() -> Result<(), String> {
    if body().is_empty() {
        return Ok(()); // nothing pasted yet — not an error
    }
    write_generated(&inject())
}

fn write_generated(inj: &Inject) -> Result<(), String> {
    let body = body();
    if body.is_empty() {
        return Ok(());
    }
    let cfg = singconf::build(&body, inj)?;
    std::fs::create_dir_all(config_dir()).map_err(|e| e.to_string())?;
    std::fs::write(generated_path(), cfg).map_err(|e| e.to_string())
}

// --- lifecycle ------------------------------------------------------------

/// Where the sidecar binary sits. Bundled next to the app by CI.
pub fn binary_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("sing-box.exe")))
            .unwrap_or_else(|| PathBuf::from("sing-box.exe"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        PathBuf::from("sing-box")
    }
}

/// One row of the config list beside the apps: the tag sing-box will see,
/// and whether the user has left it on.
#[derive(Serialize, Clone)]
pub struct CfgRow {
    pub tag: String,
    pub enabled: bool,
}

/// One app that was given a config. Absent = the first one that is on.
#[derive(Serialize, Clone)]
pub struct RouteRow {
    pub path: String,
    pub tag: String,
}

#[derive(Serialize)]
pub struct TunnelStatus {
    pub running: bool,
    pub has_config: bool,
    pub apps: Vec<String>,
    pub error: Option<String>,
    /// clash_api is not up until the child is, so this doubles as a liveness
    /// check rather than a promise about the config.
    pub controller: String,
    /// The configs this body becomes, in order, each with its switch.
    pub outbounds: Vec<CfgRow>,
    /// Apps with an explicit choice; the rest take the first config on.
    pub routes: Vec<RouteRow>,
}

pub fn status() -> TunnelStatus {
    let b = body();
    let off = CFG_OFF.lock().unwrap().clone();
    TunnelStatus {
        running: RUNNING.load(Ordering::SeqCst),
        // Touch the body so a restart still reports a usable config.
        has_config: generated_path().exists() && !b.is_empty(),
        apps: apps(),
        error: LAST_ERROR.lock().unwrap().clone(),
        controller: CLASH_API.to_string(),
        outbounds: singconf::tags(&b)
            .into_iter()
            .map(|tag| CfgRow {
                enabled: !off.iter().any(|o| *o == tag),
                tag,
            })
            .collect(),
        routes: APP_ROUTE
            .lock()
            .unwrap()
            .iter()
            .map(|(path, tag)| RouteRow {
                path: path.clone(),
                tag: tag.clone(),
            })
            .collect(),
    }
}

/// Start the sidecar with the stored config.
pub fn start(relay_ip: String, panel_ip: String) -> Result<(), String> {
    if RUNNING.load(Ordering::SeqCst) {
        return Ok(());
    }
    *RELAY_IP.lock().unwrap() = relay_ip;
    *PANEL_IP.lock().unwrap() = panel_ip;
    LAST_ERROR.lock().unwrap().take();

    let inj = inject();
    let body = body();
    if body.is_empty() {
        return Err("اول کانفیگ sing-box را وارد کنید.".into());
    }
    let cfg = singconf::build(&body, &inj)?;
    std::fs::create_dir_all(config_dir()).map_err(|e| e.to_string())?;
    std::fs::write(generated_path(), &cfg).map_err(|e| e.to_string())?;

    let bin = binary_path();
    if !bin.exists() {
        return Err(format!(
            "sing-box.exe پیدا نشد ({}). نصب را دوباره انجام دهید.",
            bin.display()
        ));
    }

    // The generated config uses `dns_mode`, which only exists from 1.14.0.
    // An older binary answers with a schema error nobody can act on, so ask
    // the binary what it is first and name the number.
    if let (Some(v), Some(min)) = (version_of(&bin), singconf::ver(singconf::MIN_SING_BOX)) {
        if v < min {
            return Err(format!(
                "این sing-box نسخهٔ {}.{}.{} است و {}.{}.{} لازم است. نصب را دوباره انجام دهید.",
                v.0, v.1, v.2, min.0, min.1, min.2
            ));
        }
    }

    let cfg_path = generated_path();
    let child = crate::dns::cmd(&bin.to_string_lossy())
        .args(["run", "-c", &cfg_path.to_string_lossy()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("اجرای sing-box شکست خورد: {e}"))?;

    RUNNING.store(true, Ordering::SeqCst);
    let gen = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    *CHILD.lock().unwrap() = Some(child);
    log_to_file(&format!(
        "TUNNEL START {} -> {}",
        bin.display(),
        cfg_path.display()
    ));
    spawn_watch(gen);
    Ok(())
}

pub fn stop(reason: &str) -> Result<(), String> {
    RUNNING.store(false, Ordering::SeqCst);
    GENERATION.fetch_add(1, Ordering::SeqCst);
    let mut child = CHILD.lock().unwrap().take();
    let mut err = None;
    if let Some(c) = child.as_mut() {
        match c.kill() {
            Ok(()) => {
                let _ = c.wait();
            }
            Err(e) => err = Some(e.to_string()),
        }
    }
    drop(child);
    cleanup();
    log_to_file(&format!("TUNNEL STOP {reason}"));
    err.map(Err).unwrap_or(Ok(()))
}

/// Whatever sing-box leaves behind is ours to undo: the DNS cache still
/// holds answers the tunnel returned. Nothing of ours repoints DNS (that is
/// `dns_mode: "disabled"`), but a flush costs 50 ms and removes the doubt.
fn cleanup() {
    #[cfg(target_os = "windows")]
    {
        let _ = crate::dns::run("ipconfig", &["/flushdns"]);
    }
    log_to_file("TUNNEL CLEANUP");
}

/// Own the child's lifetime and the "is anyone still using this" question.
fn spawn_watch(gen: u64) {
    std::thread::spawn(move || {
        let mut idle_ticks = 0u32;
        loop {
            std::thread::sleep(Duration::from_millis(WATCH_MS));
            if GENERATION.load(Ordering::SeqCst) != gen {
                return; // a newer run owns the child now
            }
            if !RUNNING.load(Ordering::SeqCst) {
                return;
            }

            // 1. did it die on us?
            let exited = {
                let mut c = CHILD.lock().unwrap();
                match c.as_mut() {
                    Some(child) => match child.try_wait() {
                        Ok(Some(status)) => Some(format!("exit {status}")),
                        Ok(None) => None,
                        Err(e) => Some(format!("wait: {e}")),
                    },
                    None => Some("no child".into()),
                }
            };
            if let Some(why) = exited {
                *LAST_ERROR.lock().unwrap() = Some(why.clone());
                let _ = stop(&format!("EXIT {why}"));
                log_to_file(&format!("TUNNEL EXIT {why}"));
                return;
            }

            // 2. is anyone still using it?
            if apps().is_empty() {
                idle_ticks = 0;
                continue;
            }
            if any_app_running() {
                idle_ticks = 0;
                continue;
            }
            idle_ticks += 1;
            if idle_ticks * (WATCH_MS as u32 / 1000) >= GRACE_SECS as u32 {
                let _ = stop("no listed app running");
                return;
            }
        }
    });
}

/// What the binary reports for `sing-box version`, or `None` when it does
/// not answer — a version we could not read must not block a start; the
/// watchdog will report the real failure instead.
fn version_of(bin: &std::path::Path) -> Option<(u64, u64, u64)> {
    let out = crate::dns::run(&bin.to_string_lossy(), &["version"]).ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let v = singconf::ver(&text)?;
    let _ = crate::api::log_to_file(&format!("TUNNEL VERSION {v:?}"));
    Some(v)
}

// --- process matching -----------------------------------------------------

/// Full paths of every running process, or an empty vector when the snapshot
/// itself fails (rare, and never worth tearing the tunnel down for).
#[cfg(target_os = "windows")]
pub fn running_paths() -> Vec<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    const MAX_PATH_CHARS: usize = 32_768;
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap as isize == -1 {
            return out;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut e) != 0 {
            loop {
                let pid = e.th32ProcessID;
                if pid != 0 {
                    let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                    if !h.is_null() {
                        let mut buf = [0u16; MAX_PATH_CHARS];
                        let mut n = buf.len() as u32;
                        if QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut n) != 0 {
                            let s: String = buf[..n as usize]
                                .iter()
                                .map(|&c| char::from_u32(c as u32).unwrap_or('\u{fffd}'))
                                .collect();
                            out.push(s);
                        }
                        CloseHandle(h);
                    }
                }
                if Process32NextW(snap, &mut e) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
    }
    out
}

#[cfg(not(target_os = "windows"))]
pub fn running_paths() -> Vec<String> {
    Vec::new()
}

/// Case-insensitive: Windows paths are, and a game installed on another
/// drive can differ only in case.
fn any_app_running() -> bool {
    let want = apps();
    if want.is_empty() {
        return false;
    }
    running_paths()
        .iter()
        .any(|p| want.iter().any(|w| p.eq_ignore_ascii_case(w)))
}

// --- clash_api ------------------------------------------------------------

#[derive(Serialize, Clone)]
pub struct TunnelConn {
    pub process: String,
    pub path: String,
    pub chains: Vec<String>,
}

/// Which processes are on the tunnel *right now*. This is the only honest
/// answer to "is it proxying my game?" — the config says what should
/// happen; this says what did.
pub async fn connections() -> Result<Vec<TunnelConn>, String> {
    let url = format!("http://{CLASH_API}/connections");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.get(&url);
    let secret = SECRET.lock().unwrap().clone();
    if !secret.is_empty() {
        req = req.header("Authorization", format!("Bearer {secret}"));
    }
    let resp = req.send().await.map_err(|e| format!("clash_api: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("clash_api: HTTP {}", resp.status()));
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    if let Some(list) = v.get("connections").and_then(serde_json::Value::as_array) {
        for c in list {
            // The sing-box API reports the local side of every loopback
            // socket as 127.0.0.1:0 — the path is the useful half.
            let md = c.get("metadata").cloned().unwrap_or_default();
            let path = md
                .get("processPath")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string();
            let process = md
                .get("process")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    std::path::Path::new(&path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default()
                });
            let chains = c
                .get("chains")
                .and_then(serde_json::Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            out.push(TunnelConn {
                process,
                path,
                chains,
            });
        }
    }
    Ok(out)
}

static SECRET: Mutex<String> = Mutex::new(String::new());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_paths_only_rejects_everything_else() {
        // add_app rejects before it touches the list, so a bad path never
        // reaches the generated rules.
        assert!(add_app(String::new()).is_err());
        assert!(add_app("C:\\Games\\foo.dll".into()).is_err());
        assert!(add_app("C:\\Games\\does-not-exist.exe".into()).is_err());
        assert!(APP_PATHS.lock().unwrap().is_empty());
    }

    #[test]
    fn empty_body_is_not_a_config() {
        assert!(set_config("   ").is_err());
        assert!(BODY.lock().unwrap().is_empty());
    }

    #[test]
    fn body_is_read_back_from_disk() {
        std::fs::create_dir_all(config_dir()).unwrap();
        std::fs::write(raw_path(), "vless://a@b:443#x").unwrap();
        // Simulate a fresh process: nothing in memory yet.
        BODY.lock().unwrap().clear();
        assert_eq!(body(), "vless://a@b:443#x");
        BODY.lock().unwrap().clear();
    }

    #[test]
    fn status_reports_not_running_by_default() {
        let s = status();
        assert!(!s.running);
        assert_eq!(s.controller, CLASH_API);
    }

    #[test]
    fn generated_path_sits_beside_our_logs() {
        assert!(generated_path()
            .to_string_lossy()
            .ends_with("PeDitXCDN\\tunnel.json")
            || generated_path().ends_with("PeDitXCDN/tunnel.json"));
    }
}
