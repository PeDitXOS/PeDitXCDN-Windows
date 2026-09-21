import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../store";

export function LoginScreen() {
  const { panelUrl, setPanelUrl, setSession, setScreen, setError } = useAppStore();
  const [mode, setMode] = useState<"login" | "signup">("login");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [name, setName] = useState("");
  const [loading, setLoading] = useState(false);
  const [localError, setLocalError] = useState("");

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);
    setLocalError("");

    try {
      if (mode === "login") {
        const resp = await invoke<{
          ok: boolean; session?: string; message?: string;
        }>("login", { panelUrl, username, password });

        if (resp.ok && resp.session) {
          setSession(resp.session);
          setScreen("dashboard");
        } else {
          setLocalError(resp.message || "ورود ناموفق بود");
        }
      } else {
        const resp = await invoke<{
          ok: boolean; session?: string; message?: string;
        }>("signup", { panelUrl, username, password, name });

        if (resp.ok && resp.session) {
          setSession(resp.session);
          setScreen("dashboard");
        } else {
          setLocalError(resp.message || "ثبت‌نام ناموفق بود");
        }
      }
    } catch (e) {
      setLocalError(String(e));
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="h-screen flex flex-col items-center justify-center p-6"
         style={{ background: "var(--bg)" }}>

      {/* Background glow */}
      <div className="fixed inset-0 pointer-events-none"
           style={{
             background: "radial-gradient(ellipse at 50% 30%, rgba(0,212,170,0.08) 0%, transparent 70%)",
           }} />

      <div className="w-full max-w-sm space-y-6 relative z-10">
        {/* Logo */}
        <div className="text-center space-y-3">
          <div className="inline-flex items-center justify-center w-20 h-20 rounded-2xl overflow-hidden"
               style={{
                 boxShadow: "0 8px 32px rgba(0,212,170,0.3)",
               }}>
            <img src="/logo.png" alt="PeDitXCDN" className="w-full h-full object-cover" />
          </div>
          <div>
            <h1 className="text-2xl font-bold" style={{ color: "var(--text)" }}>PeDitXCDN</h1>
            <p className="text-sm mt-1" style={{ color: "var(--muted)" }}>سرویس DNS اشتراکی</p>
          </div>
        </div>

        {/* Panel URL (collapsible) */}
        <details className="card p-3 group">
          <summary className="text-xs cursor-pointer select-none"
                   style={{ color: "var(--muted)" }}>
            تنظیمات پنل ▾
          </summary>
          <input
            type="text"
            value={panelUrl}
            onChange={(e) => setPanelUrl(e.target.value)}
            className="input text-xs mt-2"
            placeholder="https://panel.example.com:8443"
          />
        </details>

        {/* Form */}
        <form onSubmit={handleSubmit} className="card p-5 space-y-4">
          <h2 className="text-base font-bold" style={{ color: "var(--text)" }}>
            {mode === "login" ? "ورود" : "ثبت‌نام"}
          </h2>

          {mode === "signup" && (
            <div>
              <label className="text-[11px] mb-1 block" style={{ color: "var(--muted)" }}>نام</label>
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                className="input"
                placeholder="نام نمایشی"
                required
              />
            </div>
          )}

          <div>
            <label className="text-[11px] mb-1 block" style={{ color: "var(--muted)" }}>نام کاربری</label>
            <input
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              className="input"
              placeholder="نام کاربری"
              required
              autoComplete="username"
            />
          </div>

          <div>
            <label className="text-[11px] mb-1 block" style={{ color: "var(--muted)" }}>رمز عبور</label>
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="input"
              placeholder="رمز عبور"
              required
              autoComplete={mode === "login" ? "current-password" : "new-password"}
            />
          </div>

          {localError && (
            <div className="text-xs p-3 rounded-xl" style={{
              color: "var(--danger)",
              background: "rgba(255,71,87,0.1)",
              border: "1px solid rgba(255,71,87,0.2)",
            }}>
              {localError}
            </div>
          )}

          <button type="submit" disabled={loading}
                  className="btn-primary w-full py-3 text-sm">
            {loading ? "در حال پردازش..." : mode === "login" ? "ورود" : "ثبت‌نام"}
          </button>

          <button type="button"
                  onClick={() => { setMode(mode === "login" ? "signup" : "login"); setLocalError(""); }}
                  className="btn-ghost w-full text-xs py-2">
            {mode === "login" ? "حساب ندارید؟ ثبت‌نام کنید" : "حساب دارید؟ وارد شوید"}
          </button>
        </form>
      </div>
    </div>
  );
}
