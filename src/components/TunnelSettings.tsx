import { useCallback, useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { TunnelOpts, TunnelStatus } from "../types";
import { IcGear, IcShield } from "./icons";

/**
 * Every sing-box knob the sidecar actually honours — TUN, routing, log,
 * clash_api — plus a look at the config that was really written.
 *
 * What is *not* here is who rides the tunnel: that is the ticked-app list in
 * TunnelCard and no field on this page can widen it. The scope line at the
 * top says so in as many words, because a settings page that looks like it
 * controls "everything" is how someone ends up tunneling their whole PC.
 */
const DEFAULTS: TunnelOpts = {
  mtu: 1400,
  auto_route: true,
  strict_route: false,
  dns_mode: "disabled",
  exclude: "",
  extra_rules: "",
  log_level: "warn",
  clash_secret: "",
};

const DNS_LABELS: Record<string, string> = {
  disabled: "غیرفعال — سیستم دست‌نخورده (پیشنهادی)",
  native: "native — سیستم‌عامل",
  hijack: "hijack — خود sing-box جواب می‌دهد",
};

const LOG_LEVELS = ["error", "warn", "info", "debug", "trace"];

function Field({
  label, hint, children,
}: { label: string; hint?: string; children: ReactNode }) {
  return (
    <label className="block space-y-1">
      <span className="text-xs" style={{ color: "var(--muted)" }}>{label}</span>
      {children}
      {hint && (
        <span className="block text-[10px] leading-4" style={{ color: "var(--muted)" }}>
          {hint}
        </span>
      )}
    </label>
  );
}

function Check({
  label, hint, on, onChange,
}: { label: string; hint: string; on: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="flex items-start gap-2 cursor-pointer select-none">
      <input
        type="checkbox"
        checked={on}
        onChange={(e) => onChange(e.target.checked)}
        className="mt-0.5 w-3.5 h-3.5 shrink-0"
      />
      <span className="text-xs" style={{ color: "var(--text)" }}>
        {label}
        <span className="block text-[10px] leading-4" style={{ color: "var(--muted)" }}>
          {hint}
        </span>
      </span>
    </label>
  );
}

export function TunnelSettings() {
  const [o, setO] = useState<TunnelOpts>(DEFAULTS);
  const [st, setSt] = useState<TunnelStatus | null>(null);
  const [cfg, setCfg] = useState<string | null>(null);
  const [showCfg, setShowCfg] = useState(false);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  // The page is a fallback path, not the product: its knobs stay behind the
  // gear until somebody actually wants them.
  const [open, setOpen] = useState(false);

  const load = useCallback(async () => {
    try {
      setO(await invoke<TunnelOpts>("tunnel_opts_get"));
    } catch (e) {
      setErr(String(e));
    }
    try {
      setSt(await invoke<TunnelStatus>("tunnel_status"));
    } catch { /* status is decoration here */ }
  }, []);

  useEffect(() => { void load(); }, [load]);

  const set = <K extends keyof TunnelOpts>(k: K, v: TunnelOpts[K]) =>
    setO((p) => ({ ...p, [k]: v }));

  const preview = async () => {
    try {
      setCfg(await invoke<string>("tunnel_preview"));
    } catch (e) {
      setCfg(null);
      setErr(String(e));
    }
  };

  const apply = async () => {
    if (busy) return;
    setBusy(true);
    setErr(null);
    setSaved(false);
    try {
      await invoke("tunnel_opts_set", { opts: o });
      setSaved(true);
      setTimeout(() => setSaved(false), 3500);
      await load();
      await preview();
    } catch (e) {
      // Nothing was written: the backend validates before it saves.
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const dirty = JSON.stringify(o) !== JSON.stringify(DEFAULTS);
  const running = !!st?.running;
  const risky = o.strict_route || o.dns_mode !== "disabled";
  const excludes = o.exclude.split("\n").filter((l) => l.trim()).length;

  return (
    <div
      className="card p-4 space-y-3"
      style={{ borderColor: running ? "var(--success)" : "var(--border)" }}
    >
      {/* Behind a gear, closed by default: knobs belong to someone who came
          here on purpose, not to the eye-line of a player starting a game. */}
      <button
        type="button"
        className="row w-full cursor-pointer"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <span className="k flex items-center gap-1.5" style={{ color: "var(--p)" }}>
          <IcGear size={14} />
          <span style={{ color: "var(--muted)" }}>تنظیمات sing-box</span>
        </span>
        <span className="flex items-center gap-2">
          {running && (
            <span className="text-[10px]" style={{ color: "var(--success)" }}>
              اعمال = ری‌استارت تونل
            </span>
          )}
          <span className="text-[10px]" style={{ color: "var(--muted)" }}>
            {open ? "بستن ▲" : "باز کردن ▼"}
          </span>
        </span>
      </button>

      {!open && (
        <div className="text-[11px] truncate" style={{ color: "var(--muted)" }}>
          MTU {o.mtu} · dns {o.dns_mode} · {excludes} مسیر استثنا · لاگ {o.log_level}
          {dirty && " · تغییرِ ذخیره‌نشده"}
        </div>
      )}

      {err && (
        <div
          className="text-xs p-2 rounded"
          style={{ color: "var(--danger)", background: "rgba(255,71,87,0.1)",
                   border: "1px solid rgba(255,71,87,0.2)" }}
        >
          {err}
        </div>
      )}

      {open && (
        <>
      {/* The one thing this page must not be able to change */}
      <div
        className="text-[11px] leading-5 p-2 rounded flex items-start gap-2"
        style={{
          color: "var(--muted)",
          background: "rgba(33,169,255,0.06)",
          border: "1px dashed rgba(33,169,255,0.35)",
        }}
      >
        <span style={{ color: "var(--success)", marginTop: 1 }}>
          <IcShield size={13} />
        </span>
        <span>
          دامنه:{" "}
          <b style={{ color: "var(--success)" }}>
            فقط {st?.apps.length ?? 0} برنامهٔ انتخاب‌شده
          </b>{" "}
          — بقیهٔ ترافیک مستقیم می‌رود و این صفحه نمی‌تواند آن را عوض کند.
        </span>
      </div>

      {/* TUN */}
      <div className="space-y-2 pt-1">
        <div className="k">تونل (TUN)</div>
        <div className="grid grid-cols-2 gap-2">
          <Field label="MTU" hint="۱۴۰۰ برای بازی؛ کمتر = پایداری، بیشتر = سرعت">
            <input
              className="input text-xs"
              type="number"
              dir="ltr"
              min={576}
              max={9000}
              value={o.mtu}
              onChange={(e) => set("mtu", Number(e.target.value) || 0)}
            />
          </Field>
          <Field label="dns_mode" hint="چه کسی به پرسش DNS جواب می‌دهد">
            <select
              className="input text-xs"
              dir="ltr"
              value={o.dns_mode}
              onChange={(e) => set("dns_mode", e.target.value as TunnelOpts["dns_mode"])}
            >
              {Object.entries(DNS_LABELS).map(([v, l]) => (
                <option key={v} value={v}>{l}</option>
              ))}
            </select>
          </Field>
        </div>
        <Check
          label="auto_route — قرار گرفتن روی مسیر پیش‌فرض"
          hint="خاموش = هیچ ترافیکی وارد تونل نمی‌شود، حتی برنامه‌های انتخاب‌شده"
          on={o.auto_route}
          onChange={(v) => set("auto_route", v)}
        />
        <Check
          label="strict_route — محافظت سخت‌گیرانهٔ ویندوز (WFP)"
          hint="نشت کمتر، اما ممکن است پورت ۵۳ را هم بگیرد و مسیر DNS را بشکند"
          on={o.strict_route}
          onChange={(v) => set("strict_route", v)}
        />
        {risky && (
          <p
            className="text-[10px] leading-4 p-2 rounded"
            style={{ color: "var(--warn)", background: "rgba(255,193,7,0.08)",
                     border: "1px solid rgba(255,193,7,0.25)" }}
          >
            با این تنظیم، «سیستم دست‌نخورده» تضمین نمی‌شود؛ اتصال اصلی ممکن است
            به‌هم بخورد. اگر مشکل دیدید هر دو را به حالت پیش‌فرض برگردانید.
          </p>
        )}
      </div>

      {/* Routing */}
      <div className="space-y-2 pt-1">
        <div className="k">مسیریابی</div>
        <Field label="مسیرهای استثنا (CIDR، هر خط یکی)" hint="مثلاً شبکهٔ محلی که نباید وارد تونل شود">
          <textarea
            className="input text-[11px]"
            rows={2}
            dir="ltr"
            value={o.exclude}
            onChange={(e) => set("exclude", e.target.value)}
            placeholder={"192.168.0.0/16\n10.0.0.0/8"}
          />
        </Field>
        <Field
          label="قوانین اضافهٔ مسیریابی (JSON)"
          hint="بعد از قوانین خودمان اعمال می‌شود؛ برنامه‌های انتخاب‌شده را دور نمی‌زند"
        >
          <textarea
            className="input text-[11px]"
            rows={3}
            dir="ltr"
            value={o.extra_rules}
            onChange={(e) => set("extra_rules", e.target.value)}
            placeholder={'[{"domain_suffix":[".ir"],"action":"route","outbound":"direct"}]'}
          />
        </Field>
      </div>

      {/* Diagnostics */}
      <div className="space-y-2 pt-1">
        <div className="k">تشخیص</div>
        <div className="grid grid-cols-2 gap-2">
          <Field label="سطح لاگ" hint="برای پیدا کردن علت خطا زیاد کنید">
            <select
              className="input text-xs"
              dir="ltr"
              value={o.log_level}
              onChange={(e) => set("log_level", e.target.value)}
            >
              {LOG_LEVELS.map((l) => <option key={l} value={l}>{l}</option>)}
            </select>
          </Field>
          <Field label="رمز clash_api" hint="خالی = بدون رمز، فقط روی localhost">
            <input
              className="input text-xs"
              type="password"
              dir="ltr"
              autoComplete="off"
              value={o.clash_secret}
              onChange={(e) => set("clash_secret", e.target.value)}
              placeholder="—"
            />
          </Field>
        </div>
      </div>

      <div className="flex gap-1.5">
        <button
          className="btn-primary flex-1 py-2 text-xs"
          disabled={busy}
          onClick={apply}
        >
          {busy ? "در حال اعمال..." : saved ? "اعمال شد ✓" : dirty ? "اعمال تنظیمات" : "ذخیره"}
        </button>
        <button
          className="btn-ghost px-3 py-2 text-xs"
          disabled={busy}
          onClick={() => { setO(DEFAULTS); setErr(null); }}
        >
          پیش‌فرض
        </button>
        <button
          className="btn-ghost px-3 py-2 text-xs"
          disabled={busy}
          onClick={() => { setShowCfg((v) => !v); if (!showCfg) void preview(); }}
        >
          خروجی
        </button>
      </div>

      {showCfg && (
        <div className="space-y-1">
          <div className="row">
            <span className="k">tunnel.json — همان‌چه sing-box می‌گیرد</span>
            <button
              className="btn-ghost px-2 py-0.5 text-[10px]"
              onClick={() => { if (cfg) void navigator.clipboard?.writeText(cfg); }}
              disabled={!cfg}
            >
              کپی
            </button>
          </div>
          <pre
            className="text-[10px] leading-4 p-2 rounded overflow-auto max-h-56 whitespace-pre-wrap break-all"
            dir="ltr"
            style={{
              background: "rgba(0,0,0,0.35)",
              border: "1px solid var(--border)",
              color: "var(--muted)",
              fontFamily: "var(--mono)",
            }}
          >
            {cfg ?? "— اول کانفیگ را در «تونل بازی» ثبت کنید —"}
          </pre>
        </div>
      )}
        </>
      )}
    </div>
  );
}
