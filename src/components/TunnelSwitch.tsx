import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";
import type { TunnelStatus } from "../types";
import { IcShield } from "./icons";

/**
 * The one on/off link this app uses — the main page's tunnel, and every
 * config in the list beside the apps. Same shape everywhere, so "on" means
 * on everywhere and nobody has to learn a second switch.
 */
export function PillSwitch({
  on, onToggle, disabled, label, title,
}: {
  on: boolean;
  onToggle: () => void;
  disabled?: boolean;
  label: string;
  title?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      title={title ?? label}
      disabled={disabled}
      onClick={onToggle}
      className="relative shrink-0 rounded-full transition-colors duration-150 disabled:opacity-40"
      style={{
        width: 38,
        height: 20,
        cursor: disabled ? "not-allowed" : "pointer",
        background: on ? "var(--success)" : "rgba(255,255,255,0.12)",
        border: `1px solid ${on ? "var(--success)" : "var(--border)"}`,
      }}
    >
      <span
        className="absolute rounded-full transition-all duration-150"
        style={{
          top: 2,
          insetInlineStart: on ? 20 : 2,
          width: 14,
          height: 14,
          background: on ? "#04121c" : "var(--muted)",
        }}
      />
    </button>
  );
}

/**
 * The whole of sing-box that the main page is allowed to show: its switch,
 * and the icon that takes you to the app list. The tunnel is the path taken
 * when DNS alone is not enough, so it must not compete with the connect
 * button — config, apps and settings live on their own page behind that icon.
 */
export function TunnelSwitch({ onOpen }: { onOpen: () => void }) {
  const { relayIp, panelUrl } = useAppStore();
  const [st, setSt] = useState<TunnelStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setSt(await invoke<TunnelStatus>("tunnel_status"));
    } catch {
      /* decoration only — the switch just stays where it was */
    }
  }, []);

  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), 3000);
    return () => clearInterval(id);
  }, [refresh]);

  const running = !!st?.running;
  const ready = !!st?.has_config;
  const nApps = st?.apps.length ?? 0;
  const sub = err
    ? err
    : !ready
      ? "تنظیم نشده — برو صفحهٔ تونل"
      : running
        ? `فعال · ${nApps} برنامه`
        : `خاموش · ${nApps} برنامه`;

  const toggle = async () => {
    if (busy || !ready) return;
    setBusy(true);
    setErr(null);
    try {
      if (running) {
        await invoke("tunnel_stop");
      } else {
        // Same two calls the tunnel page makes; the main connect is not a
        // prerequisite, so the switch works on its own.
        const panel = await invoke<string>("resolve_relay_ip", { panelUrl });
        await invoke("tunnel_start", { relayIp: relayIp || panel, panelIp: panel });
      }
      await refresh();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      className="card px-3.5 py-2.5 flex items-center gap-3"
      style={{ borderColor: running ? "var(--success)" : "var(--border)" }}
    >
      {/* Icon into the app list — the only door on this page */}
      <button
        type="button"
        title="برنامه‌های تونل و تنظیمات sing-box"
        aria-label="برنامه‌های تونل"
        onClick={onOpen}
        className="rail-item shrink-0"
        style={running ? { color: "var(--success)" } : undefined}
      >
        <IcShield size={18} />
      </button>

      <div className="min-w-0 flex-1">
        <div className="text-xs font-medium" style={{ color: "var(--text)" }}>
          تونل پشتیبان
        </div>
        <div
          className="text-[10px] truncate"
          style={{ color: err ? "var(--danger)" : "var(--muted)" }}
        >
          {sub}
        </div>
      </div>

      {/* The switch itself */}
      <PillSwitch
        on={running}
        disabled={busy || !ready}
        label="روشن/خاموش کردن تونل"
        title={ready ? (running ? "توقف تونل" : "شروع تونل") : "اول کانفیگ را ثبت کن"}
        onToggle={() => void toggle()}
      />
    </div>
  );
}
