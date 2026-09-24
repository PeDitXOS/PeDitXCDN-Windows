import { useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";
import { IcPower } from "./icons";

export function ConnectButton() {
  const {
    connectionStatus, relayIp, setConnectionStatus,
    setDnsStatus, setError,
  } = useAppStore();
  const loading = connectionStatus === "connecting";
  // Which press owns the in-flight `connect`. A later press (cancel, or a
  // reconnect) bumps it, so the promise that was already out there writes
  // nothing — otherwise a cancelled connect would flip the UI back to
  // «متصل» when it finally returned.
  const gen = useRef(0);

  const toggle = async () => {
    const my = ++gen.current;

    // Cancel: the button is the cancel control while a connect is running.
    // `disconnect` bumps the backend generation (the running start_proxy
    // unwinds and puts DNS back) and stops whatever it already built; the
    // in-flight connect's own result is discarded by the guard above.
    if (loading) {
      setConnectionStatus("disconnected");
      setError(null);
      try {
        await invoke("disconnect");
        const dns = await invoke<{ configured: boolean; current_dns?: string }>("get_dns_status");
        if (my === gen.current) setDnsStatus(dns as never);
      } catch (e) {
        setError(String(e));
      }
      return;
    }

    if (connectionStatus === "connected") {
      try {
        await invoke("disconnect");
        if (my !== gen.current) return;
        setConnectionStatus("disconnected");
        // Keep relayIp: it is the resolved panel host and is what the next
        // connect needs. Clearing it here made the Connect button a no-op
        // ("آی‌پی رله در دسترسی نیست") for every reconnect.
        const dns = await invoke<{ configured: boolean; current_dns?: string }>("get_dns_status");
        setDnsStatus(dns as never);
      } catch (e) {
        setError(String(e));
      }
      return;
    }

    if (!relayIp) {
      setError("آی‌پی رله در دسترسی نیست. ابتدا وارد شوید.");
      return;
    }

    setConnectionStatus("connecting");
    setError(null);
    try {
      await invoke<string>("connect", { relayIp });
      if (my !== gen.current) return;
      setConnectionStatus("connected");
      const dns = await invoke<{ configured: boolean; current_dns?: string }>("get_dns_status");
      setDnsStatus(dns as never);
    } catch (e) {
      if (my !== gen.current) return;
      setConnectionStatus("error");
      setError(String(e));
    }
  };

  const stateClass = loading
    ? "connecting"
    : connectionStatus === "connected"
    ? "connected"
    : connectionStatus === "error"
    ? "error"
    : "disconnected";

  const label = loading
    ? "لغو اتصال"
    : connectionStatus === "connected"
    ? "قطع اتصال"
    : "اتصال";

  const sublabel = loading
    ? "در حال اتصال… — کلیک برای لغو"
    : connectionStatus === "connected"
    ? "DNS فعال است"
    : relayIp
    ? `هدف: ${relayIp}`
    : "آماده اتصال";

  // Enabled while connecting — that is the cancel control; otherwise it only
  // needs a relay to connect to.
  return (
    <button onClick={toggle} disabled={!loading && !relayIp}
            className={`connect-btn ${stateClass}`}>
      {/* Pulse animation when connected */}
      {connectionStatus === "connected" && (
        <div className="absolute inset-0 rounded glow-success" />
      )}

      <div className="relative z-10 flex flex-col items-center gap-1.5">
        <span className="opacity-90">
          <IcPower size={26} />
        </span>

        {/* Main Label */}
        <div className="text-lg font-bold">{label}</div>

        {/* Sub Label */}
        <div className="text-xs mt-0.5 opacity-70">{sublabel}</div>
      </div>
    </button>
  );
}
