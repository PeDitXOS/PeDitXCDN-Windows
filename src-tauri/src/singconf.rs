//! Ingest whatever the user pasted for the tunnel layer, inject the
//! guarantees that must hold whatever it contains, emit a sing-box config.
//!
//! The config half is JSON all the way down, so this uses `serde_json` and
//! `urlencoding` rather than a hand-rolled parser — both are already
//! dependencies. Tests run through the scratch crate (regenerated from
//! source each session), not bare `rustc`.
//!
//! Field names below were checked against the sing-box docs for 1.15:
//! `route_exclude_address` and `dns_mode` live on the **TUN inbound** (not
//! under `route`), `sniff` does not exist on the inbound at all, and a route
//! rule names its target with `action: "route"` + `outbound` (`outbound` on
//! the rule itself has been deprecated since 1.11).

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::net::IpAddr;

/// Past 2 MB a body is a fetch bug, not a subscription.
pub const MAX_BODY: usize = 2 * 1024 * 1024;

/// `dns_mode` only exists from sing-box 1.14.0; `action` on a route rule
/// only from 1.11.0. The sidecar is pinned at or above this in CI.
pub const MIN_SING_BOX: &str = "1.14.0";

/// First `x.y.z` anywhere in the text — `sing-box version` output, a
/// `MIN_SING_BOX` literal, or garbage. Unparseable is `None`: a version we
/// could not read must not turn into a decision.
pub fn ver(text: &str) -> Option<(u64, u64, u64)> {
    let start = text.find(|c: char| c.is_ascii_digit())?;
    let rest = &text[start..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(rest.len());
    let mut p = rest[..end].split('.');
    let a = p.next()?.parse().ok()?;
    let b = p.next()?.parse().ok()?;
    let c = p.next().map_or(Some(0), |x| x.parse().ok())?;
    Some((a, b, c))
}

/// Everything the injected rules need to know about *our* side.
#[derive(Default, Clone)]
pub struct Inject {
    /// Relay address. Excluded at TUN level and matched to `direct` by rule,
    /// so our own DNS path survives sing-box dying.
    pub relay_ip: String,
    /// Panel address, same treatment as the relay.
    pub panel_ip: String,
    /// Full paths of the apps the user ticked. Exact `process_path` match —
    /// never a name, or any other `game.exe` would be swept up.
    pub app_paths: Vec<String>,
    /// Install roots of ticked apps; their children (launcher → game →
    /// anti-cheat) are matched by regex scoped to that root only.
    pub install_roots: Vec<String>,
    /// Interface to pin the `direct` outbound to. Empty = let sing-box
    /// detect it (`auto_detect_interface`), because we cannot know the
    /// machine's NIC name ahead of time.
    pub bind_interface: String,
    /// Everything the settings page lets the user turn. Deliberately *not*
    /// a way to widen who rides the tunnel: the ticked apps above are the
    /// only thing that decides scope, and nothing here can override them.
    pub opts: Opts,
    /// Which outbound each ticked app rides, keyed by its exact path.
    /// Nothing here = the first config, which is what every app got before
    /// this field existed. A tag the current body does not have is read the
    /// same way — the config was replaced under the user, not their intent.
    pub app_route: Vec<(String, String)>,
    /// Configs switched off in the list. Never a default, never a target;
    /// the last one on cannot be switched off, so this can never empty the
    /// choices out from under an app that is already routed.
    pub cfg_off: Vec<String>,
}

/// `dns_mode` values sing-box 1.14+ accepts on the TUN inbound.
const DNS_MODES: [&str; 3] = ["disabled", "native", "hijack"];

/// `log.level` values sing-box accepts. Anything else is a schema error
/// at start time, where it would read like a broken config.
const LOG_LEVELS: [&str; 7] = ["trace", "debug", "info", "warn", "error", "fatal", "panic"];

/// The knobs the settings tab exposes. One struct, one file on disk, one
/// `validate()` — the UI never builds JSON itself, so a field cannot reach
/// the generated config without passing the same check here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Opts {
    /// TUN MTU. 1400 is deliberate: sing-box's larger default fragments
    /// inside the tunnel and shows up as jitter in games.
    pub mtu: u32,
    /// Put the TUN on the default route. Off is legal but means no traffic
    /// reaches the tunnel at all, ticked apps included.
    pub auto_route: bool,
    /// WFP-level strict routing on Windows: stronger leak protection, and
    /// it can also catch port 53 — the one thing this design needs left
    /// alone. Off by default for that reason.
    pub strict_route: bool,
    /// `disabled` keeps the system resolver ours (127.0.0.1 → proxy →
    /// relay). `hijack` hands DNS to sing-box.
    pub dns_mode: String,
    /// Extra CIDRs kept out of the TUN routes, one per line — a home LAN a
    /// TUN would otherwise swallow.
    pub exclude: String,
    /// Extra route rules as raw JSON `[ { … }, { … } ]`, for anything the
    /// page has no control for. Appended *after* our guarantees, never in
    /// place of them.
    pub extra_rules: String,
    pub log_level: String,
    /// clash_api bearer secret. Empty = unauthenticated, loopback only.
    pub clash_secret: String,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            mtu: 1400,
            auto_route: true,
            strict_route: false,
            dns_mode: "disabled".into(),
            exclude: String::new(),
            extra_rules: String::new(),
            log_level: "warn".into(),
            clash_secret: String::new(),
        }
    }
}

impl Opts {
    /// Everything that would otherwise fail inside sing-box, in Persian and
    /// before the file is written. A settings file that is silently wrong
    /// only surfaces as "the tunnel does not start".
    pub fn validate(&self) -> Result<(), String> {
        if !(576..=9000).contains(&self.mtu) {
            return Err(format!("MTU باید بین ۵۷۶ تا ۹۰۰۰ باشد ( {}).", self.mtu));
        }
        if !DNS_MODES.contains(&self.dns_mode.as_str()) {
            return Err(format!(
                "dns_mode نامعتبر است: {}. مجاز: {}.",
                self.dns_mode,
                DNS_MODES.join("، ")
            ));
        }
        if !LOG_LEVELS.contains(&self.log_level.as_str()) {
            return Err(format!(
                "سطح لاگ نامعتبر است: {}. مجاز: {}.",
                self.log_level,
                LOG_LEVELS.join("، ")
            ));
        }
        if self.clash_secret.len() > 256 {
            return Err("رمز clash_api بیش از ۲۵۶ نویسه است.".into());
        }
        for cidr in self.exclude_lines() {
            check_cidr(&cidr)?;
        }
        self.rules()?;
        Ok(())
    }

    /// Non-empty, trimmed lines of `exclude`. Blank lines are formatting,
    /// not a CIDR, so they never reach `check_cidr`.
    pub fn exclude_lines(&self) -> Vec<String> {
        self.exclude
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// The extra rules, parsed. `validate()` already ran this, so an error
    /// here means the file was edited under us — return it, never panic.
    pub fn rules(&self) -> Result<Vec<Value>, String> {
        let t = self.extra_rules.trim();
        if t.is_empty() {
            return Ok(Vec::new());
        }
        let v: Value =
            serde_json::from_str(t).map_err(|e| format!("قوانین اضافه JSON نیست: {e}"))?;
        let arr = v
            .as_array()
            .ok_or("قوانین اضافه باید یک آرایه [ … ] باشد.")?;
        for r in arr {
            if !r.is_object() {
                return Err("هر قانون باید یک شیء { … } باشد.".into());
            }
        }
        Ok(arr.clone())
    }
}

/// `host/prefix`, host a real IP, prefix a number that fits the family.
/// Anything else would be handed to sing-box as a CIDR and rejected there.
fn check_cidr(cidr: &str) -> Result<(), String> {
    let (host, pfx) = cidr
        .split_once('/')
        .ok_or_else(|| format!("CIDR نیست (باید مانند 192.168.0.0/16 باشد): {cidr}"))?;
    let ip: IpAddr = host
        .trim()
        .parse()
        .map_err(|_| format!("آی‌پی نامعتبر در CIDR: {cidr}"))?;
    let n: u8 = pfx
        .trim()
        .parse()
        .map_err(|_| format!("پیشوند نامعتبر در CIDR: {cidr}"))?;
    let max = if ip.is_ipv4() { 32 } else { 128 };
    if n > max {
        return Err(format!("پیشوند باید تا {max} باشد: {cidr}"));
    }
    Ok(())
}

/// Turn a pasted body into a complete sing-box config.
pub fn build(raw: &str, inj: &Inject) -> Result<String, String> {
    let body = raw.trim();
    if body.is_empty() {
        return Err("کانفیگ خالی است.".into());
    }
    if body.len() > MAX_BODY {
        return Err(format!("کانفیگ بزرگ‌تر از {MAX_BODY} بایت است."));
    }
    // Settings are validated on every build, not only when they are saved:
    // a hand-edited file on disk must fail here, in Persian, rather than as
    // a schema error from a child we never see the stderr of.
    inj.opts.validate()?;

    let (outs, user_rules, rule_set) = parse_body(body)?;
    if outs.is_empty() {
        return Err(no_usable(body));
    }
    Ok(render(&outs, &user_rules, rule_set.as_ref(), inj))
}

/// What to say when nothing converted — with the count and the first reason,
/// so «۱۲ لینک، ۰ قابل استفاده» is a diagnosis, not a shrug.
fn no_usable(body: &str) -> String {
    let lines = scan(body);
    let mut reasons: Vec<String> = Vec::new();
    let mut total = 0usize;
    for (n, line) in lines {
        if line.starts_with('#') {
            continue;
        }
        total += 1;
        if reasons.len() < 3 {
            match link(&line) {
                Ok(_) => {}
                Err(e) => reasons.push(format!("خط {n}: {e}")),
            }
        }
    }
    format!("{total} لینک، ۰ قابل استفاده. {}", reasons.join(" · "))
}

/// Split a body into (line number, text), whatever shape it came in.
fn scan(body: &str) -> Vec<(usize, String)> {
    body.lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim().to_string()))
        .filter(|(_, l)| !l.is_empty())
        .collect()
}

/// Detect the body shape and pull the outbounds, the user's route rules and
/// their `route.rule_set` out of it. The last one matters: a rule that names
/// a rule-set tag sing-box never heard of is a startup failure, not a hint.
fn parse_body(body: &str) -> Result<(Vec<Value>, Vec<Value>, Option<Value>), String> {
    let head = body.chars().next().unwrap_or(' ');

    if head == '[' {
        let v: Value = serde_json::from_str(body)
            .map_err(|_| format!("آرایهٔ لینک‌ها JSON معتبر نیست: {}", snippet(body)))?;
        let items = v.as_array().ok_or("لیست لینک‌ها آرایه نیست.")?;
        let mut texts = Vec::new();
        for it in items {
            match it.as_str() {
                Some(s) => texts.push(s.to_string()),
                None => return Err("یکی از عناصر آرایه رشته نیست.".into()),
            }
        }
        return Ok((collect_links(&texts), Vec::new(), None));
    }

    if head == '{' {
        let v: Value = serde_json::from_str(body)
            .map_err(|_| format!("JSON معتبر نیست: {}", snippet(body)))?;
        let has = v.get("inbounds").is_some() || v.get("outbounds").is_some();
        if !has {
            return Err(format!(
                "JSON است ولی کانفیگ sing-box نیست: {}",
                snippet(body)
            ));
        }
        let mut outs = Vec::new();
        if let Some(arr) = v.get("outbounds").and_then(Value::as_array) {
            for o in arr {
                if o.get("type").is_some() {
                    outs.push(o.clone());
                }
            }
        }
        let route = v.get("route");
        let rules = route
            .and_then(|r| r.get("rules"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let rule_set = route.and_then(|r| r.get("rule_set")).cloned();
        return Ok((outs, rules, rule_set));
    }

    // Not JSON: either a raw multi-line link list or one base64 blob.
    let text = undecorate(body);
    let lines: Vec<String> = text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    Ok((collect_links(&lines), Vec::new(), None))
}

/// A single-line body with no `://` is a base64 blob; anything else is
/// already text. Decoding a blob that turns out not to be UTF-8 just falls
/// back to the original, so a weird body costs one attempt.
fn undecorate(body: &str) -> String {
    if body.contains("://") || body.contains('\n') || body.contains('#') {
        return body.to_string();
    }
    match b64(body) {
        Some(bytes) => String::from_utf8(bytes).unwrap_or_else(|_| body.to_string()),
        None => body.to_string(),
    }
}

fn collect_links(lines: &[String]) -> Vec<Value> {
    lines
        .iter()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| link(l).ok())
        .collect()
}

/// First 24 characters of a body, for an error message. Deliberately a
/// prefix of what the user pasted and never a decoded key.
fn snippet(body: &str) -> String {
    let s: String = body.chars().take(24).collect();
    format!("{s}…")
}

// --- outbound construction ------------------------------------------------

/// Dispatch one line to the parser for its scheme.
fn link(line: &str) -> Result<Value, String> {
    let (scheme, rest) = line
        .split_once("://")
        .ok_or("scheme ندارد (مثل vless:// یا trojan://).")?;
    let scheme = scheme.trim().to_ascii_lowercase();
    match scheme.as_str() {
        "vless" => vless(rest),
        "vmess" => vmess(rest),
        "trojan" => trojan(rest),
        "ss" => shadowsocks(rest),
        "hysteria2" | "hy2" => hysteria2(rest),
        "tuic" => tuic(rest),
        "wg" | "wireguard" => wireguard(rest),
        "ssr" => Err("ssr پشتیبانی نمی‌شود (از sing-box حذف شده).".into()),
        "hysteria" => Err("hysteria نسخهٔ قدیمی پشتیبانی نمی‌شود؛ hysteria2 بدهید.".into()),
        "warp" => Err("warp پشتیبانی نمی‌شود.".into()),
        other => Err(format!("scheme ناشناخته: {other}")),
    }
}

/// `scheme://userinfo@host:port?query#name`
struct Uri {
    user: String,
    host: String,
    port: u16,
    query: Vec<(String, String)>,
    name: String,
}

fn uri(rest: &str, default_port: u16) -> Result<Uri, String> {
    let (before_frag, frag) = match rest.split_once('#') {
        Some((b, f)) => (b, percent(f)),
        None => (rest, String::new()),
    };
    let (auth, query) = match before_frag.split_once('?') {
        Some((a, q)) => (a, q),
        None => (before_frag, ""),
    };
    let (userinfo, hp) = match auth.rsplit_once('@') {
        Some((u, h)) => (percent(u), h.to_string()),
        None => (String::new(), auth.to_string()),
    };
    let (host, port) = host_port(&hp, default_port)?;
    Ok(Uri {
        user: userinfo,
        host,
        port,
        query: parse_query(query),
        name: frag,
    })
}

/// Split `host:port`, tolerating a bracketed IPv6 literal and an absent port.
fn host_port(hp: &str, default_port: u16) -> Result<(String, u16), String> {
    if let Some(rest) = hp.strip_prefix('[') {
        let (h, tail) = rest
            .split_once(']')
            .ok_or("IPv6 بدون `]` بسته نشده.")?;
        let port = match tail.strip_prefix(':') {
            Some(p) => p.parse().map_err(|_| "پورت نامعتبر است.")?,
            None => default_port,
        };
        return Ok((h.to_string(), port));
    }
    match hp.rsplit_once(':') {
        Some((h, p)) => {
            let port: u16 = p.parse().map_err(|_| "پورت نامعتبر است.")?;
            Ok((h.to_string(), port))
        }
        None => Ok((hp.to_string(), default_port)),
    }
}

fn parse_query(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|s| !s.is_empty())
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.to_ascii_lowercase(), percent(v)))
        .collect()
}

fn q<'a>(u: &'a Uri, k: &str) -> Option<&'a str> {
    u.query
        .iter()
        .find(|(key, _)| key == k)
        .map(|(_, v)| v.as_str())
        .filter(|v| !v.is_empty())
}

fn tls_of(u: &Uri) -> Value {
    let mut t = Map::new();
    t.insert("enabled".into(), json!(true));
    let sni = q(u, "sni").or_else(|| q(u, "peer")).unwrap_or(&u.host);
    t.insert("server_name".into(), json!(sni));
    if q(u, "insecure").map(|v| v == "1" || v == "true").unwrap_or(false) {
        t.insert("insecure".into(), json!(true));
    }
    if let Some(fp) = q(u, "fp").or_else(|| q(u, "fingerprint")) {
        t.insert("utls".into(), json!({"enabled": true, "fingerprint": fp}));
    }
    if let Some(alpn) = q(u, "alpn") {
        t.insert(
            "alpn".into(),
            json!(alpn.split(',').map(|s| s.trim()).collect::<Vec<_>>()),
        );
    }
    // REALITY travels beside TLS, not instead of it.
    if let Some(pk) = q(u, "pbk").or_else(|| q(u, "publicKey")) {
        let mut r = Map::new();
        r.insert("enabled".into(), json!(true));
        r.insert("public_key".into(), json!(pk));
        if let Some(sid) = q(u, "sid").or_else(|| q(u, "shortId")) {
            r.insert("short_id".into(), json!(sid));
        }
        t.insert("reality".into(), Value::Object(r));
    }
    Value::Object(t)
}

/// `transport` from the `type=` query key, for vless/vmess/trojan.
fn transport_of(u: &Uri) -> Option<Value> {
    let kind = q(u, "type")?;
    let mut t = Map::new();
    t.insert("type".into(), json!(kind));
    match kind {
        "ws" => {
            if let Some(p) = q(u, "path") {
                t.insert("path".into(), json!(p));
            }
            if let Some(h) = q(u, "host") {
                t.insert("headers".into(), json!({"Host": h}));
            }
        }
        "grpc" => {
            if let Some(p) = q(u, "serviceName") {
                t.insert("service_name".into(), json!(p));
            }
        }
        "http" | "httpupgrade" => {
            if let Some(p) = q(u, "path") {
                t.insert("path".into(), json!(p));
            }
        }
        _ => {}
    }
    Some(Value::Object(t))
}

fn named(u: &Uri, kind: &str) -> String {
    if u.name.is_empty() {
        format!("{kind}-{}", u.host)
    } else {
        u.name.clone()
    }
}

fn vless(rest: &str) -> Result<Value, String> {
    let u = uri(rest, 443)?;
    if u.user.is_empty() {
        return Err("uuid در vless خالی است.".into());
    }
    let mut o = Map::new();
    o.insert("type".into(), json!("vless"));
    o.insert("tag".into(), json!(named(&u, "vless")));
    o.insert("server".into(), json!(u.host));
    o.insert("uuid".into(), json!(u.user));
    if let Some(f) = q(&u, "flow") {
        o.insert("flow".into(), json!(f));
    }
    if q(&u, "packetEncoding").is_some() || q(&u, "packet_encoding").is_some() {
        o.insert(
            "packet_encoding".into(),
            json!(q(&u, "packetEncoding").or_else(|| q(&u, "packet_encoding")).unwrap()),
        );
    }
    let security = q(&u, "security").unwrap_or("none");
    if security != "none" {
        o.insert("tls".into(), tls_of(&u));
    }
    if let Some(t) = transport_of(&u) {
        o.insert("transport".into(), t);
    }
    o.insert("server_port".into(), json!(u.port));
    Ok(Value::Object(o))
}

fn vmess(rest: &str) -> Result<Value, String> {
    let raw = b64(rest.trim()).ok_or("vmess باید base64 باشد.")?;
    let txt = String::from_utf8(raw).map_err(|_| "vmess base64 به UTF-8 تبدیل نشد.")?;
    let v: Value =
        serde_json::from_str(&txt).map_err(|_| "JSON داخل vmess معتبر نیست.")?;
    let get = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("");
    let host = get("add");
    if host.is_empty() {
        return Err("آدرس سرور در vmess خالی است.".into());
    }
    let name = if get("ps").is_empty() {
        format!("vmess-{host}")
    } else {
        get("ps").to_string()
    };
    let mut o = Map::new();
    o.insert("type".into(), json!("vmess"));
    o.insert("tag".into(), json!(name));
    o.insert("server".into(), json!(host));
    o.insert(
        "server_port".into(),
        json!(get("port").parse::<u16>().unwrap_or(443)),
    );
    o.insert("uuid".into(), json!(get("id")));
    if !get("aid").is_empty() && get("aid") != "0" {
        o.insert("alter_id".into(), json!(get("aid").parse::<u16>().unwrap_or(0)));
    }
    if !get("scy").is_empty() {
        o.insert("security".into(), json!(get("scy")));
    }
    if get("tls") == "tls" || get("tls") == "reality" {
        let mut t = Map::new();
        t.insert("enabled".into(), json!(true));
        let sni = if get("sni").is_empty() { host } else { get("sni") };
        t.insert("server_name".into(), json!(sni));
        o.insert("tls".into(), Value::Object(t));
    }
    match get("net") {
        "ws" => {
            let mut t = map_from(&[("type", json!("ws")), ("path", json!(get("path")))]);
            if !get("host").is_empty() {
                t.insert("headers".into(), json!({"Host": get("host")}));
            }
            o.insert("transport".into(), Value::Object(t));
        }
        "grpc" => {
            o.insert(
                "transport".into(),
                Value::Object(map_from(&[("type", json!("grpc")), ("service_name", json!(get("path")))])),
            );
        }
        "h2" | "http" => {
            o.insert(
                "transport".into(),
                Value::Object(map_from(&[("type", json!("http")), ("path", json!(get("path")))])),
            );
        }
        _ => {}
    }
    Ok(Value::Object(o))
}

fn trojan(rest: &str) -> Result<Value, String> {
    let u = uri(rest, 443)?;
    if u.user.is_empty() {
        return Err("رمز در trojan خالی است.".into());
    }
    let mut o = Map::new();
    o.insert("type".into(), json!("trojan"));
    o.insert("tag".into(), json!(named(&u, "trojan")));
    o.insert("server".into(), json!(u.host));
    o.insert("server_port".into(), json!(u.port));
    o.insert("password".into(), json!(u.user));
    o.insert("tls".into(), tls_of(&u));
    if let Some(t) = transport_of(&u) {
        o.insert("transport".into(), t);
    }
    Ok(Value::Object(o))
}

fn shadowsocks(rest: &str) -> Result<Value, String> {
    let (cred, tail) = rest
        .split_once('@')
        .ok_or("ss باید method:pass@host:port باشد.")?;
    let cred = if cred.contains(':') && !is_b64ish(cred) {
        cred.to_string()
    } else {
        let d = b64(cred).ok_or("رمز base64 در ss خراب است.")?;
        String::from_utf8(d).map_err(|_| "رمز base64 در ss UTF-8 نیست.")?
    };
    let (method, pass) = cred.split_once(':').ok_or("ss method:pass جدا نشده.")?;
    let u = uri(&format!("x://{tail}"), 8388)?;
    let mut o = Map::new();
    o.insert("type".into(), json!("shadowsocks"));
    o.insert("tag".into(), json!(named(&u, "ss")));
    o.insert("server".into(), json!(u.host));
    o.insert("server_port".into(), json!(u.port));
    o.insert("method".into(), json!(method));
    o.insert("password".into(), json!(pass));
    if let Some(p) = q(&u, "plugin") {
        o.insert("plugin".into(), json!(p));
    }
    Ok(Value::Object(o))
}

fn hysteria2(rest: &str) -> Result<Value, String> {
    let u = uri(rest, 443)?;
    if u.user.is_empty() {
        return Err("رمز در hysteria2 خالی است.".into());
    }
    let mut o = Map::new();
    o.insert("type".into(), json!("hysteria2"));
    o.insert("tag".into(), json!(named(&u, "hy2")));
    o.insert("server".into(), json!(u.host));
    o.insert("server_port".into(), json!(u.port));
    o.insert("password".into(), json!(u.user));
    if q(&u, "insecure").map(|v| v == "1" || v == "true").unwrap_or(false) {
        o.insert("tls".into(), tls_of(&u));
    } else {
        let mut t = tls_of(&u);
        if let Some(obj) = t.as_object_mut() {
            obj.insert("insecure".into(), json!(false));
        }
        o.insert("tls".into(), t);
    }
    if let Some(up) = q(&u, "upmbps") {
        o.insert("up_mbps".into(), json!(up.parse::<u64>().unwrap_or(0)));
    }
    if let Some(down) = q(&u, "downmbps") {
        o.insert("down_mbps".into(), json!(down.parse::<u64>().unwrap_or(0)));
    }
    Ok(Value::Object(o))
}

fn tuic(rest: &str) -> Result<Value, String> {
    let u = uri(rest, 443)?;
    let (uuid, pass) = u
        .user
        .split_once(':')
        .ok_or("tuic باید uuid:password@host باشد.")?;
    let mut o = Map::new();
    o.insert("type".into(), json!("tuic"));
    o.insert("tag".into(), json!(named(&u, "tuic")));
    o.insert("server".into(), json!(u.host));
    o.insert("server_port".into(), json!(u.port));
    o.insert("uuid".into(), json!(uuid));
    o.insert("password".into(), json!(pass));
    o.insert("tls".into(), tls_of(&u));
    Ok(Value::Object(o))
}

/// WireGuard has no agreed URI scheme, so providers spell it differently.
/// ponytail: fields are taken from the common `wg://priv@host:port?publickey=…`
/// shape; if a provider hands over something else, the parser returns a
/// Persian error naming the line rather than guessing — extend here.
fn wireguard(rest: &str) -> Result<Value, String> {
    let u = uri(rest, 51820)?;
    if u.user.is_empty() {
        return Err("کلید خصوصی در wg خالی است.".into());
    }
    let pubk = q(&u, "publickey")
        .or_else(|| q(&u, "peer_public_key"))
        .ok_or("publickey= در wg وجود ندارد.")?;
    let mut o = Map::new();
    o.insert("type".into(), json!("wireguard"));
    o.insert("tag".into(), json!(named(&u, "wg")));
    o.insert("server".into(), json!(u.host));
    o.insert("server_port".into(), json!(u.port));
    o.insert("private_key".into(), json!(u.user));
    o.insert("peer_public_key".into(), json!(pubk));
    let ips: Vec<&str> = q(&u, "address")
        .map(|a| a.split(',').map(str::trim).collect())
        .unwrap_or_default();
    if !ips.is_empty() {
        o.insert("local_address".into(), json!(ips));
    }
    if let Some(n) = q(&u, "reserved") {
        o.insert("reserved".into(), json!(n));
    }
    Ok(Value::Object(o))
}

fn map_from(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

// --- base64 / percent -----------------------------------------------------

fn is_b64ish(s: &str) -> bool {
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '-' | '_'))
}

/// Base64 that accepts standard and URL-safe alphabets, with or without
/// padding, and ignores whitespace — providers are inconsistent about all three.
fn b64(s: &str) -> Option<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let s = s.trim_end_matches('=');
    if s.is_empty() || !is_b64ish(s) {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.chars() {
        let v = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' | '-' => 62,
            '/' | '_' => 63,
            _ => return None,
        };
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1u32 << bits) - 1;
        }
    }
    Some(out)
}

fn percent(s: &str) -> String {
    urlencoding::decode(s)
        .map(|d| d.into_owned())
        .unwrap_or_else(|_| s.to_string())
}

// --- the injection layer --------------------------------------------------

/// The tags a body's outbounds end up with once ours has been dropped and
/// the empty ones numbered. Shared with `tags()` so the list the UI shows is
/// the list the generated config actually contains.
fn tags_of(outs: &[Value]) -> Vec<String> {
    let mut n = 0usize;
    outs.iter()
        .filter(|o| o.get("tag").and_then(Value::as_str) != Some("direct"))
        .map(|o| {
            n += 1;
            match o.get("tag").and_then(Value::as_str) {
                Some(t) if !t.is_empty() => t.to_string(),
                _ => format!("out-{n}"),
            }
        })
        .collect()
}

/// The configs a body becomes — what the page lists next to the apps.
/// An unparseable body lists nothing rather than half of it.
pub fn tags(body: &str) -> Vec<String> {
    match parse_body(body.trim()) {
        Ok((outs, _, _)) => tags_of(&outs),
        Err(_) => Vec::new(),
    }
}

/// Whom one app was given, or the default when the choice is gone: the
/// config changed, or that config was switched off. Failing closed here
/// would take the whole tunnel down over a stale dropdown value.
fn route_for(path: &str, tags: &[String], inj: &Inject, default: &str) -> String {
    inj.app_route
        .iter()
        .find(|(p, _)| p.eq_ignore_ascii_case(path))
        .map(|(_, t)| t.as_str())
        .filter(|t| tags.iter().any(|x| x == t))
        .filter(|t| !inj.cfg_off.iter().any(|o| o == t))
        .unwrap_or(default)
        .to_string()
}

/// The folder an exe sits in. Split by hand rather than through `Path`,
/// because the paths are always Windows' backslashed ones and the tests run
/// on whatever machine has the compiler — where `\` is not a separator.
fn folder_of(p: &str) -> &str {
    match p.trim_end_matches(['/', '\\']).rfind(['/', '\\']) {
        Some(i) => &p[..i],
        None => "",
    }
}

/// An install root is the folder of a ticked app, so the launcher and the
/// game under it follow that app's choice. Two ticked apps in one folder
/// wanting two different configs have no single honest answer — they get the
/// default rather than one of them silently winning.
fn root_route(root: &str, tags: &[String], inj: &Inject, default: &str) -> String {
    let mut chosen: Option<String> = None;
    for p in &inj.app_paths {
        let parent = folder_of(p);
        if !parent.eq_ignore_ascii_case(root) {
            continue;
        }
        let r = route_for(p, tags, inj, default);
        match &chosen {
            None => chosen = Some(r),
            Some(prev) if *prev == r => {}
            Some(_) => return default.to_string(),
        }
    }
    chosen.unwrap_or_else(|| default.to_string())
}

fn render(
    outs: &[Value],
    user_rules: &[Value],
    rule_set: Option<&Value>,
    inj: &Inject,
) -> String {
    // A pasted full config almost always brings its own `direct`. Ours is
    // prepended unconditionally, so keeping theirs would hand sing-box two
    // outbounds with the same tag and it would refuse to start.
    let kept: Vec<Value> = outs
        .iter()
        .filter(|o| o.get("tag").and_then(Value::as_str) != Some("direct"))
        .cloned()
        .collect();
    let tags = tags_of(&kept);
    let outs: Vec<Value> = kept
        .iter()
        .zip(&tags)
        .map(|(o, t)| {
            let mut o = o.clone();
            if let Some(m) = o.as_object_mut() {
                m.insert("tag".into(), json!(t));
            }
            o
        })
        .collect();

    // Whom an unrouted app goes to: the first config still switched on.
    // All off is unreachable from the page (it refuses), but the file has to
    // name a real outbound either way.
    let first = tags
        .iter()
        .find(|t| !inj.cfg_off.iter().any(|o| o == *t))
        .or_else(|| tags.first())
        .cloned()
        .unwrap_or_else(|| "direct".into());

    // Our guarantees go first, so a rule in the user's config can never
    // outrank them.
    let mut rules: Vec<Value> = Vec::new();

    // (2) The relay/panel never traverse the user's tunnel: excluded from the
    // TUN routes outright *and* matched to `direct`. Two mechanisms on purpose
    // — the first keeps the packets out of sing-box's stack entirely, the
    // second still helps if `route_exclude_address` is ever unavailable.
    let mut exclude: Vec<String> = Vec::new();
    for ip in [&inj.relay_ip, &inj.panel_ip] {
        if !ip.is_empty() {
            exclude.push(format!("{ip}/32"));
        }
    }
    // Then the user's own exclusions (LAN and friends). Ours first so a
    // paste can never drop the relay out of the list by overflow.
    exclude.extend(inj.opts.exclude_lines());
    if !exclude.is_empty() {
        rules.push(json!({"ip_cidr": exclude, "action": "route", "outbound": "direct"}));
    }

    // (1) The ticked apps and their descendants. Each app goes to the config
    // it was given, or the first one that is still on. `process_path` is
    // exact; the regex is scoped to the install root so a same-named exe
    // elsewhere is not swept up.
    for p in &inj.app_paths {
        let out = route_for(p, &tags, inj, &first);
        rules.push(json!({"process_path": p, "action": "route", "outbound": out}));
    }
    for root in &inj.install_roots {
        let escaped = regex_escape(root);
        let out = root_route(root, &tags, inj, &first);
        rules.push(json!({
            "process_path_regex": format!("^{}", escaped),
            "action": "route",
            "outbound": out
        }));
    }

    rules.extend(user_rules.iter().cloned());
    // The settings page's raw rules. Last, so they can add exceptions but
    // never outrank the guarantees above them.
    if let Ok(extra) = inj.opts.rules() {
        rules.extend(extra);
    }

    let mut tun = Map::new();
    tun.insert("type".into(), json!("tun"));
    tun.insert("tag".into(), json!("tun-in"));
    tun.insert("interface_name".into(), json!("peditx-tun"));
    tun.insert(
        "address".into(),
        json!(["172.19.0.1/30", "fdfe:dcba:9876::1/126"]),
    );
    // The four the settings page owns. Their defaults are the design
    // (small MTU, DNS untouched, no WFP filter); turning them is the user's.
    tun.insert("mtu".into(), json!(inj.opts.mtu));
    tun.insert("dns_mode".into(), json!(inj.opts.dns_mode.as_str()));
    tun.insert("auto_route".into(), json!(inj.opts.auto_route));
    tun.insert("strict_route".into(), json!(inj.opts.strict_route));
    if !exclude.is_empty() {
        tun.insert("route_exclude_address".into(), json!(exclude));
    }

    let mut direct = Map::new();
    direct.insert("type".into(), json!("direct"));
    direct.insert("tag".into(), json!("direct"));
    if !inj.bind_interface.is_empty() {
        direct.insert("bind_interface".into(), json!(inj.bind_interface));
    }

    let mut route = Map::new();
    route.insert("rules".into(), json!(rules));
    if let Some(rs) = rule_set {
        route.insert("rule_set".into(), rs.clone());
    }
    route.insert("final".into(), json!("direct"));
    route.insert("auto_detect_interface".into(), json!(true));
    route.insert("find_process".into(), json!(true));

    let mut root = Map::new();
    root.insert(
        "log".into(),
        json!({"level": inj.opts.log_level.as_str(), "timestamp": false}),
    );
    root.insert(
        "inbounds".into(),
        json!([Value::Object(tun)]),
    );
    let mut all: Vec<Value> = vec![Value::Object(direct)];
    all.extend(outs);
    root.insert("outbounds".into(), json!(all));
    root.insert("route".into(), Value::Object(route));
    // No `dns` section on purpose: with `dns_mode: "disabled"` the system
    // resolver is still ours (127.0.0.1 → proxy → relay), which is the
    // guarantee the whole design hangs on. A private DoH for sing-box's own
    // server lookup is the upgrade path if the relay ever stops resolving.
    root.insert(
        "experimental".into(),
        json!({"clash_api": {"external_controller": "127.0.0.1:9090",
                             "secret": inj.opts.clash_secret.as_str()}}),
    );

    serde_json::to_string_pretty(&Value::Object(root)).unwrap_or_else(|_| "{}".into())
}

/// Minimal regex escaping for a filesystem path: only the characters that
/// can appear in a Windows path *and* mean something to a regex.
fn regex_escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\\' => "\\\\".to_string(),
            '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' | '#'
            | '&' | '-' | '~' => format!("\\{c}"),
            c => c.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ver_reads_whatever_shape_it_gets() {
        assert_eq!(ver("sing-box version 1.14.2"), Some((1, 14, 2)));
        assert_eq!(ver("v1.11.0\nEnvironment: go1.24.1"), Some((1, 11, 0)));
        assert_eq!(ver(MIN_SING_BOX), Some((1, 14, 0)));
        assert_eq!(ver("no digits at all"), None);
    }

    fn inj() -> Inject {
        Inject {
            relay_ip: "92.42.207.101".into(),
            panel_ip: "85.215.222.228".into(),
            app_paths: vec![r"C:\Games\valorant\valorant.exe".into()],
            install_roots: vec![r"C:\Games\valorant".into()],
            bind_interface: String::new(),
            opts: Opts::default(),
            app_route: Vec::new(),
            cfg_off: Vec::new(),
        }
    }

    /// Same, with the settings the page can change.
    fn inj_with(opts: Opts) -> Inject {
        Inject { opts, ..inj() }
    }

    fn cfg_with(inj: &Inject, raw: &str) -> Value {
        let s = build(raw, inj).expect("config should build");
        serde_json::from_str(&s).expect("config should be valid JSON")
    }

    fn cfg(raw: &str) -> Value {
        let s = build(raw, &inj()).expect("config should build");
        serde_json::from_str(&s).expect("config should be valid JSON")
    }

    const VLESS: &str =
        "vless://11111111-2222-3333-4444-555555555555@cdn.example.com:443?\
         type=tcp&security=reality&pbk=PUBKEY&sid=abcd&fp=chrome&sni=cdn.example.com&\
         flow=xtls-rprx-vision#MyNode";

    const TROJAN: &str = "trojan://secret@example.org:444?security=tls&sni=example.org#T";
    const SS: &str = "ss://YWVzLTI1Ni1nY206cGFzcw@example.net:8388#S";
    const SS_PLAIN: &str = "ss://aes-256-gcm:mypassword@1.2.3.4:443#S2";
    const HY2: &str = "hysteria2://pw@host.example:8443?sni=host.example#H";
    const TUIC: &str = "tuic://uuid-1:pass@host.example:443?sni=host.example#U";

    #[test]
    fn injects_the_four_guarantees() {
        let c = cfg(VLESS);
        let tun = &c["inbounds"][0];
        assert_eq!(tun["dns_mode"], "disabled");
        assert_eq!(tun["strict_route"], false);
        assert_eq!(tun["mtu"], 1400);
        assert_eq!(tun["auto_route"], true);
        // route_exclude_address belongs to the TUN inbound, not to `route`.
        let ex = tun["route_exclude_address"].as_array().unwrap();
        assert!(ex.iter().any(|v| v.as_str() == Some("92.42.207.101/32")));
        assert_eq!(c["route"]["final"], "direct");
        assert_eq!(c["route"]["auto_detect_interface"], true);
        // Never a `dns` section: the system resolver stays on 127.0.0.1.
        assert!(c.get("dns").is_none());
    }

    #[test]
    fn app_rules_come_before_user_rules() {
        let full = format!(
            r#"{{"outbounds":[{{"type":"vless","tag":"mine","server":"1.1.1.1",
               "server_port":443,"uuid":"u"}}],
               "route":{{"rules":[{{"domain_suffix":[".ir"],"action":"route","outbound":"direct"}}]}}}}"#
        );
        let c = cfg(&full);
        let rules = c["route"]["rules"].as_array().unwrap();
        // Order is the guarantee: relay, then our app rules, then theirs.
        assert!(rules[0].get("ip_cidr").is_some());
        let app = rules.iter().position(|r| r.get("process_path").is_some()).unwrap();
        let user = rules.iter().position(|r| r.get("domain_suffix").is_some()).unwrap();
        assert!(app < user, "our rule at {app} must precede the user's at {user}");
        assert_eq!(rules[app]["process_path"], r"C:\Games\valorant\valorant.exe");
        assert_eq!(rules[app]["outbound"], "mine");
    }

    #[test]
    fn relay_never_reaches_the_tunnel() {
        let c = cfg(VLESS);
        let r = c["route"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r.get("ip_cidr").is_some())
            .expect("relay rule");
        assert_eq!(r["outbound"], "direct");
        assert_eq!(r["action"], "route");
    }

    #[test]
    fn direct_outbound_is_always_present_and_first() {
        let c = cfg(VLESS);
        assert_eq!(c["outbounds"][0]["type"], "direct");
        assert_eq!(c["outbounds"][0]["tag"], "direct");
    }

    #[test]
    fn parses_each_supported_scheme() {
        assert_eq!(cfg(VLESS)["outbounds"][1]["type"], "vless");
        assert_eq!(cfg(VLESS)["outbounds"][1]["server"], "cdn.example.com");
        assert_eq!(cfg(VLESS)["outbounds"][1]["tls"]["reality"]["enabled"], true);

        assert_eq!(cfg(TROJAN)["outbounds"][1]["type"], "trojan");
        assert_eq!(cfg(TROJAN)["outbounds"][1]["password"], "secret");

        assert_eq!(cfg(SS)["outbounds"][1]["type"], "shadowsocks");
        assert_eq!(cfg(SS)["outbounds"][1]["password"], "pass");
        assert_eq!(cfg(SS_PLAIN)["outbounds"][1]["method"], "aes-256-gcm");

        assert_eq!(cfg(HY2)["outbounds"][1]["type"], "hysteria2");
        assert_eq!(cfg(TUIC)["outbounds"][1]["type"], "tuic");
        assert_eq!(cfg(TUIC)["outbounds"][1]["uuid"], "uuid-1");
    }

    #[test]
    fn parses_vmess_both_base64_shapes() {
        // Standard base64 of the vmess JSON.
        let inner = r#"{"v":"2","ps":"N","add":"10.0.0.1","port":"443",
            "id":"aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee","aid":"0","net":"ws",
            "path":"/ws","host":"h.example","tls":"tls","sni":"h.example"}"#;
        let b = b64encode(inner.as_bytes());
        let c = cfg(&format!("vmess://{b}"));
        let o = &c["outbounds"][1];
        assert_eq!(o["type"], "vmess");
        assert_eq!(o["server"], "10.0.0.1");
        assert_eq!(o["server_port"], 443);
        assert_eq!(o["transport"]["type"], "ws");
        assert_eq!(o["transport"]["path"], "/ws");
        assert_eq!(o["tls"]["enabled"], true);
    }

    #[test]
    fn base64_body_decodes_to_a_link_list() {
        let body = b64encode(format!("{VLESS}\n{TROJAN}").as_bytes());
        let c = cfg(&body);
        assert_eq!(c["outbounds"].as_array().unwrap().len(), 3); // + direct
    }

    #[test]
    fn raw_multiline_body_works() {
        let body = format!("# header\n\n{VLESS}\n{TROJAN}\n");
        assert_eq!(cfg(&body)["outbounds"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn url_safe_and_unpadded_base64_are_accepted() {
        let inner = b"vless://uuid@h:443?security=tls#X";
        // Pad-free URL-safe encoding of the same bytes.
        let enc = b64encode(inner);
        let urlsafe = enc.replace('+', "-").replace('/', "_");
        let c = cfg(&urlsafe.trim_end_matches('='));
        assert_eq!(c["outbounds"][1]["type"], "vless");
    }

    #[test]
    fn full_config_keeps_its_own_outbounds() {
        let full = r#"{"inbounds":[],"outbounds":[
            {"type":"direct","tag":"direct"},
            {"type":"vless","tag":"mine","server":"9.9.9.9","server_port":443,"uuid":"u"}]}"#;
        let c = cfg(full);
        let tags: Vec<&str> = c["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|o| o["tag"].as_str())
            .collect();
        assert!(tags.contains(&"direct"));
        assert!(tags.contains(&"mine"));
    }

    #[test]
    fn a_pasted_direct_is_replaced_not_duplicated() {
        let full = r#"{"outbounds":[
            {"type":"direct","tag":"direct"},
            {"type":"selector","tag":"sel","outbounds":["direct","mine"]},
            {"type":"vless","tag":"mine","server":"9.9.9.9","server_port":443,"uuid":"u"}]}"#;
        let c = cfg(full);
        let direct = c["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|o| o["tag"] == "direct")
            .count();
        assert_eq!(direct, 1, "two outbounds tagged direct is a startup failure");
        assert!(c["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["tag"] == "sel"));
    }

    #[test]
    fn rule_set_definitions_survive_the_conversion() {
        let full = r#"{"outbounds":[{"type":"vless","tag":"mine","server":"9.9.9.9",
            "server_port":443,"uuid":"u"}],
            "route":{"rule_set":[{"type":"remote","tag":"geo","url":"https://x/geo.yaml"}],
            "rules":[{"rule_set":["geo"],"action":"route","outbound":"mine"}]}}"#;
        let c = cfg(full);
        assert_eq!(c["route"]["rule_set"][0]["tag"], "geo");
        let rules = c["route"]["rules"].as_array().unwrap();
        let user = rules.iter().position(|r| r.get("rule_set").is_some()).unwrap();
        assert_eq!(rules[user]["rule_set"][0], "geo");
    }

    #[test]
    fn rejects_unsupported_schemes_by_name() {
        for (body, needle) in [
            ("ssr://abc", "ssr"),
            ("hysteria://abc", "hysteria"),
            ("warp://abc", "warp"),
        ] {
            let e = build(body, &inj()).unwrap_err();
            assert!(e.contains(needle), "{body} -> {e}");
        }
    }

    #[test]
    fn counts_and_explains_when_nothing_is_usable() {
        let e = build("ssr://a\nhysteria://b\nwarp://c", &inj()).unwrap_err();
        assert!(e.contains('3'), "{e}"); // three lines
        assert!(e.contains("۰ قابل استفاده"), "{e}");
    }

    #[test]
    fn garbage_body_gets_a_snippet_not_a_silence() {
        let e = build("just some prose without any scheme at all", &inj()).unwrap_err();
        assert!(e.contains("لینک"), "{e}");
        assert!(e.contains("خط 1"), "{e}");
    }

    #[test]
    fn json_that_is_not_a_config_is_named_as_such() {
        let e = build(r#"{"hello":"world"}"#, &inj()).unwrap_err();
        assert!(e.contains("کانفیگ sing-box نیست"), "{e}");
    }

    #[test]
    fn size_ceiling_is_enforced() {
        let big = "a".repeat(MAX_BODY + 1);
        assert!(build(&big, &inj()).unwrap_err().contains("بزرگ‌تر"));
        assert!(build("   ", &inj()).unwrap_err().contains("خالی"));
    }

    #[test]
    fn install_root_regex_is_escaped_and_scoped() {
        let c = cfg(VLESS);
        let re = c["route"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r.get("process_path_regex").is_some())
            .unwrap();
        assert_eq!(re["process_path_regex"], r"^C:\\Games\\valorant");
        assert_eq!(re["outbound"], "MyNode"); // first user outbound's tag
    }

    #[test]
    fn missing_port_falls_back_to_the_scheme_default() {
        assert_eq!(cfg(VLESS)["outbounds"][1]["server_port"], 443);
        let noport = "trojan://p@h.example#X";
        assert_eq!(cfg(noport)["outbounds"][1]["server_port"], 443);
    }

    #[test]
    fn opts_reach_the_generated_config() {
        let o = Opts {
            mtu: 1280,
            auto_route: false,
            strict_route: true,
            dns_mode: "hijack".into(),
            exclude: "192.168.0.0/16\n10.0.0.0/8".into(),
            extra_rules: r#"[{"domain_suffix":[".ir"],"action":"route","outbound":"direct"}]"#.into(),
            log_level: "debug".into(),
            clash_secret: "s3cret".into(),
        };
        let c = cfg_with(&inj_with(o), VLESS);
        let tun = &c["inbounds"][0];
        assert_eq!(tun["mtu"], 1280);
        assert_eq!(tun["auto_route"], false);
        assert_eq!(tun["strict_route"], true);
        assert_eq!(tun["dns_mode"], "hijack");
        // Our guarantees stay first; the user's exclusions come after.
        let ex: Vec<&str> = tun["route_exclude_address"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(ex[0], "92.42.207.101/32");
        assert!(ex.contains(&"192.168.0.0/16"));
        assert!(ex.contains(&"10.0.0.0/8"));
        assert_eq!(c["log"]["level"], "debug");
        assert_eq!(c["experimental"]["clash_api"]["secret"], "s3cret");
        // Extra rules are appended after the process rules, never instead.
        let rules = c["route"]["rules"].as_array().unwrap();
        assert!(rules.iter().any(|r| r["process_path"].is_string()));
        assert_eq!(rules.last().unwrap()["domain_suffix"][0], ".ir");
        assert!(c.get("dns").is_none());
    }

    #[test]
    fn bad_opts_never_reach_a_config() {
        let mut o = Opts::default();
        o.mtu = 100;
        let e = build(VLESS, &inj_with(o)).unwrap_err();
        assert!(e.contains("MTU"), "{e}");

        let mut o = Opts::default();
        o.dns_mode = "always".into();
        let e = build(VLESS, &inj_with(o)).unwrap_err();
        assert!(e.contains("dns_mode"), "{e}");

        let mut o = Opts::default();
        o.exclude = "nonsense".into();
        let e = build(VLESS, &inj_with(o)).unwrap_err();
        assert!(e.contains("CIDR"), "{e}");

        let mut o = Opts::default();
        o.extra_rules = "{not json}".into();
        let e = build(VLESS, &inj_with(o)).unwrap_err();
        assert!(e.contains("JSON"), "{e}");

        // An array is fine; a bare object is not a list of rules.
        let mut o = Opts::default();
        o.extra_rules = r#"{"domain":["a"]}"#.into();
        assert!(build(VLESS, &inj_with(o)).is_err());
    }

    #[test]
    fn default_opts_are_the_shipped_guarantees() {
        let d = Opts::default();
        assert_eq!(d, Opts::default());
        d.validate().unwrap();
        assert_eq!(d.mtu, 1400);
        assert_eq!(d.dns_mode, "disabled");
        assert!(!d.strict_route);
        assert!(d.exclude_lines().is_empty());
        assert!(d.rules().unwrap().is_empty());
    }

    /// A settings file written by an older build loads with the new defaults
    /// instead of failing deserialization outright.
    #[test]
    fn partial_settings_file_loads() {
        let o: Opts = serde_json::from_str(r#"{"mtu": 1300}"#).unwrap();
        assert_eq!(o.mtu, 1300);
        assert_eq!(o.dns_mode, "disabled");
        o.validate().unwrap();
    }

    /// Local encoder so the tests do not depend on a base64 crate.
    fn outbound_of(c: &Value, path: &str) -> String {
        c["route"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["process_path"].as_str() == Some(path))
            .map(|r| r["outbound"].as_str().unwrap_or_default().to_string())
            .unwrap_or_else(|| panic!("no rule for {path}"))
    }

    fn root_outbound(c: &Value, root: &str) -> String {
        let want = format!("^{}", regex_escape(root));
        c["route"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["process_path_regex"].as_str() == Some(want.as_str()))
            .map(|r| r["outbound"].as_str().unwrap_or_default().to_string())
            .unwrap_or_else(|| panic!("no root rule for {root}"))
    }

    /// What the page lists as configs is what the generated file contains.
    #[test]
    fn tags_are_the_configs_the_file_gets() {
        assert_eq!(tags(VLESS), ["MyNode"]);
        let two = format!("{VLESS}\n{TROJAN}");
        assert_eq!(tags(&two), ["MyNode", "T"]);
        // `direct` is ours: never listed, never numbered.
        let full = r#"{"outbounds":[{"type":"direct","tag":"direct"},{"type":"vless","tag":"x"}]}"#;
        assert_eq!(tags(full), ["x"]);
        assert!(tags("{not json").is_empty());
    }

    /// One app, one config: the ticked game rides the config it was given
    /// while everything else still takes the first one.
    #[test]
    fn an_app_can_ride_a_named_config() {
        let mut i = inj();
        i.app_route.push((r"C:\Games\valorant\valorant.exe".into(), "T".into()));
        let c = cfg_with(&i, &format!("{VLESS}\n{TROJAN}"));
        assert_eq!(outbound_of(&c, r"C:\Games\valorant\valorant.exe"), "T");
        // The install root follows the app, so its launcher rides along.
        assert_eq!(root_outbound(&c, r"C:\Games\valorant"), "T");
        assert_eq!(outbound_of(&cfg(VLESS), r"C:\Games\valorant\valorant.exe"), "MyNode");
    }

    /// The choice is only a choice while the config still carries that tag
    /// and still has its switch on — otherwise the default, silently.
    #[test]
    fn a_missing_or_switched_off_choice_falls_back() {
        let mut i = inj();
        i.app_route.push((r"C:\Games\valorant\valorant.exe".into(), "gone".into()));
        let c = cfg_with(&i, VLESS);
        assert_eq!(outbound_of(&c, r"C:\Games\valorant\valorant.exe"), "MyNode");

        let mut i = inj();
        i.app_route.push((r"C:\Games\valorant\valorant.exe".into(), "MyNode".into()));
        i.cfg_off.push("MyNode".into());
        let c = cfg_with(&i, &format!("{VLESS}\n{TROJAN}"));
        assert_eq!(outbound_of(&c, r"C:\Games\valorant\valorant.exe"), "T");
        assert_eq!(root_outbound(&c, r"C:\Games\valorant"), "T");

        // Every config off still has to name something sing-box knows.
        let mut i = inj();
        i.cfg_off.push("MyNode".into());
        assert_eq!(outbound_of(&cfg_with(&i, VLESS), r"C:\Games\valorant\valorant.exe"), "MyNode");
    }

    /// Two ticked apps in one folder wanting two configs: neither wins the
    /// folder rule, because one of them would be silently overruled.
    #[test]
    fn a_folder_with_two_choices_gets_the_default() {
        let mut i = inj();
        i.app_paths.push(r"C:\Games\valorant\launcher.exe".into());
        i.app_route.push((r"C:\Games\valorant\valorant.exe".into(), "T".into()));
        let c = cfg_with(&i, &format!("{VLESS}\n{TROJAN}"));
        assert_eq!(outbound_of(&c, r"C:\Games\valorant\valorant.exe"), "T");
        assert_eq!(outbound_of(&c, r"C:\Games\valorant\launcher.exe"), "MyNode");
        assert_eq!(root_outbound(&c, r"C:\Games\valorant"), "MyNode");
    }

    fn b64encode(data: &[u8]) -> String {
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
            for i in 0..4 {
                if i < chunk.len() + 1 {
                    out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
                }
            }
        }
        match data.len() % 3 {
            1 => {
                out.pop();
                out.push('=');
            }
            2 => {
                out.pop();
                out.push('=');
            }
            _ => {}
        }
        out
    }
}
