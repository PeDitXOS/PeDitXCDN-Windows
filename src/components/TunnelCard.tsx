import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";
import type { TunnelConn, TunnelStatus } from "../types";
import { IcShield } from "./icons";
import { PillSwitch } from "./TunnelSwitch";

/**
 * The sing-box sidecar: paste a config, tick the apps that should ride it,
 * start it. Every claim on screen comes from the backend — `tunnel_status`
 * for what is, `tunnel_connections` (clash_api) for what is *actually*
 * proxying, because a config that says "route this game" is not evidence
 * that the game is on the tunnel.
 *
 * The configs sit beside the apps because the question is one sentence with
 * two halves: *which program, on which link* — Discord on u28, the rest on
 * whatever is left switched on.
 *
 * Independent of the panel session on purpose: a game tunnel is useful even
 * when the account UI is logged out, so its state lives here, not in the
 * store that `logout()` clears.
 */
export function TunnelCard() {
  const { relayIp, panelUrl } = useAppStore();
  const [st, setSt] = useState<TunnelStatus | null>(null);
  const [conns, setConns] = useState<TunnelConn[]>([]);
  const [body, setBody] = useState("");
  const [path, setPath] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setSt(await invoke<TunnelStatus>("tunnel_status"));
    } catch (e) {
      setErr(String(e));
    }
  }, []);

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 3000);
    return () => clearInterval(id);
  }, [refresh]);

  // clash_api only answers while the child is up, so this poll starts and
  // stops with it instead of failing every 4 s forever.
  useEffect(() => {
    if (!st?.running) {
      setConns([]);
      return;
    }
    let alive = true;
    const tick = async () => {
      try {
        const c = await invoke<TunnelConn[]>("tunnel_connections");
        if (alive) setConns(c);
      } catch {
        if (alive) setConns([]);
      }
    };
    tick();
    const id = setInterval(tick, 4000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [st?.running]);

  const run = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    setErr(null);
    try {
      await fn();
      await refresh();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const saveCfg = () => run(() => invoke("tunnel_set_config", { body }));
  const addApp = () =>
    run(async () => {
      await invoke("tunnel_add_app", { path });
      setPath("");
    });
  const removeApp = (p: string) => run(() => invoke("tunnel_remove_app", { path: p }));

  // Only configs that are on can be ridden — the backend refuses the rest,
  // so offering them in the dropdown would be a promise that fails on click.
  const onCfgs = (st?.outbounds ?? []).filter((c) => c.enabled);
  /** What the generated rule will actually say for this app. */
  const effRoute = (p: string): string => {
    const r = st?.routes.find((x) => x.path === p);
    return r && onCfgs.some((c) => c.tag === r.tag) ? r.tag : (onCfgs[0]?.tag ?? "");
  };
  const setRoute = (p: string, tag: string) =>
    run(() => invoke("tunnel_set_route", { path: p, tag }));
  const setCfg = (tag: string, on: boolean) =>
    run(() => invoke("tunnel_set_cfg", { tag, on }));

  const start = () =>
    run(async () => {
      // Both resolve to the relay in this architecture; the main connect is
      // not a prerequisite, so the tunnel works on its own.
      const panel = await invoke<string>("resolve_relay_ip", { panelUrl });
      const relay = relayIp || panel;
      await invoke("tunnel_start", { relayIp: relay, panelIp: panel });
    });

  const stop = () => run(() => invoke("tunnel_stop"));

  const running = !!st?.running;
  const dot = running ? "var(--success)" : st?.error ? "var(--danger)" : "var(--muted)";
  const label = running ? "فعال" : st?.error ? "خطا" : st?.has_config ? "آماده" : "تنظیم نشده";

  // clash_api repeats a process per connection; one row per binary is the
  // readable answer to "is my game on it?".
  const seen = new Map<string, TunnelConn>();
  for (const c of conns) {
    const key = (c.path || c.process).toLowerCase();
    if (!seen.has(key)) seen.set(key, c);
  }
  const live = [...seen.values()];

  return (
    <div
      className="card p-4 space-y-2"
      style={{
        borderColor: running ? "var(--success)" : "var(--border)",
        boxShadow: running ? "0 0 20px rgba(33,169,255,0.1)" : "none",
      }}
    >
      <div className="row">
        <span className="k flex items-center gap-1.5" style={{ color: "var(--p)" }}>
          <IcShield size={14} />
          <span style={{ color: "var(--muted)" }}>تونل بازی (sing-box)</span>
        </span>
        <div className="flex items-center gap-2">
          <div
            className="status-ring"
            style={{ background: dot, boxShadow: `0 0 8px ${dot}` }}
          />
          <span className="font-medium" style={{ color: dot }}>
            {label}
          </span>
        </div>
      </div>

      {!st?.has_config ? (
        <div className="space-y-1.5 pt-1">
          <div className="text-xs" style={{ color: "var(--muted)" }}>
            لینک‌های sing-box یا کانفیگ JSON را بچسبانید (هر خط یک لینک)
          </div>
          <textarea
            className="input text-xs"
            rows={4}
            dir="ltr"
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder="vless://…   /   { …inbounds… }"
          />
          <button
            className="btn-primary w-full py-2 text-xs"
            disabled={busy || !body.trim()}
            onClick={saveCfg}
          >
            {busy ? "در حال بررسی..." : "ثبت کانفیگ"}
          </button>
        </div>
      ) : (
        <>
          <div className="row">
            <span className="k">مسیر DNS جدا</span>
            <span className="v" style={{ color: "var(--success)" }}>
              سیستم دست‌نخورده ✓
            </span>
          </div>

          {/* Apps and configs, side by side: which program, on which link */}
          <div
            className={
              onCfgs.length
                ? "grid grid-cols-2 gap-x-3 gap-y-1.5 pt-1"
                : "space-y-1 pt-1"
            }
          >
            <div className="space-y-1.5 min-w-0">
              <div className="text-xs" style={{ color: "var(--muted)" }}>
                برنامه‌های تیک‌خورده
                <span className="block text-[10px]">بقیه مستقیم می‌روند</span>
              </div>
              {st.apps.length === 0 && (
                <div className="text-[11px]" style={{ color: "var(--warn)" }}>
                  هنوز برنامه‌ای اضافه نشده — تونل بی‌خاصیت است.
                </div>
              )}
              {st.apps.map((a) => (
                <div key={a} className="space-y-1">
                  <div className="row">
                    <span
                      className="v font-mono text-[10px] truncate"
                      style={{ textAlign: "start" }}
                      title={a}
                    >
                      {a}
                    </span>
                    <button
                      className="btn-ghost px-2 py-0.5 text-[10px]"
                      disabled={busy}
                      onClick={() => removeApp(a)}
                      title="حذف"
                    >
                      ✕
                    </button>
                  </div>
                  {/* The routing method: which config this app connects on */}
                  <select
                    className="input text-[10px] w-full py-1"
                    dir="ltr"
                    disabled={busy || onCfgs.length === 0}
                    value={effRoute(a)}
                    title="روی کدام کانفیگ وصل شود"
                    onChange={(e) => setRoute(a, e.target.value)}
                  >
                    {onCfgs.map((c) => (
                      <option key={c.tag} value={c.tag}>
                        {c.tag}
                      </option>
                    ))}
                  </select>
                </div>
              ))}
            </div>

            {onCfgs.length > 0 && (
              <div className="space-y-1.5 min-w-0">
                <div className="text-xs" style={{ color: "var(--muted)" }}>
                  کانفیگ‌ها
                  <span className="block text-[10px]">خاموش = اصلاً ساخته نمی‌شود</span>
                </div>
                {st.outbounds.map((c) => (
                  <div className="row" key={c.tag}>
                    <span
                      className="v font-mono text-[10px] truncate"
                      dir="ltr"
                      style={{ textAlign: "start" }}
                      title={c.tag}
                    >
                      {c.tag}
                    </span>
                    <PillSwitch
                      on={c.enabled}
                      disabled={busy}
                      label={`روشن/خاموش کردن کانفیگ ${c.tag}`}
                      title={
                        c.enabled && onCfgs.length === 1
                          ? "آخرین کانفیگ روشن را نمی‌توان خاموش کرد"
                          : c.enabled
                            ? `خاموش کردن ${c.tag}`
                            : `روشن کردن ${c.tag}`
                      }
                      onToggle={() => setCfg(c.tag, !c.enabled)}
                    />
                  </div>
                ))}
                <div className="text-[10px] leading-4" style={{ color: "var(--muted)" }}>
                  {st.outbounds.length > 1
                    ? "هر برنامه با کانفیگ کنارش وصل می‌شود."
                    : "برای انتخاب مسیر، چند لینک بچسبانید."}
                </div>
              </div>
            )}
          </div>

          <div className="flex gap-1.5 pt-0.5">
            <input
              className="input text-[11px] px-2 py-1.5"
              dir="ltr"
              value={path}
              onChange={(e) => setPath(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && path.trim()) addApp();
              }}
              placeholder="C:\Games\game\game.exe"
            />
            <button
              className="btn-ghost px-3 text-[11px] shrink-0"
              disabled={busy || !path.trim()}
              onClick={addApp}
            >
              افزودن
            </button>
          </div>

          {/* Live proof, not a promise */}
          {running && (
            <div className="space-y-1 pt-1">
              <div className="row">
                <span className="k">ترافیک لحظه‌ای روی تونل</span>
                <span className="v font-mono">{live.length}</span>
              </div>
              {live.slice(0, 5).map((c) => (
                <div className="row" key={c.path || c.process}>
                  <span className="v font-mono text-[11px]" style={{ textAlign: "start" }}>
                    {c.process}
                  </span>
                  <span className="v font-mono text-[10px]" style={{ color: "var(--muted)" }}>
                    {c.chains.join(" → ")}
                  </span>
                </div>
              ))}
            </div>
          )}
        </>
      )}

      {st?.error && (
        <div
          className="text-xs p-2 rounded"
          style={{
            color: "var(--danger)",
            background: "rgba(255,71,87,0.1)",
            border: "1px solid rgba(255,71,87,0.2)",
          }}
        >
          {st.error}
        </div>
      )}
      {err && (
        <div
          className="text-xs p-2 rounded"
          style={{
            color: "var(--danger)",
            background: "rgba(255,71,87,0.1)",
            border: "1px solid rgba(255,71,87,0.2)",
          }}
        >
          {err}
        </div>
      )}

      {st?.has_config && (
        <button
          className={running ? "btn-danger w-full py-2 text-xs" : "btn-primary w-full py-2 text-xs"}
          disabled={busy}
          onClick={running ? stop : start}
        >
          {busy ? "..." : running ? "توقف تونل" : "شروع تونل"}
        </button>
      )}
    </div>
  );
}
