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
    <div className="min-h-screen flex items-center justify-center p-6">
      <div className="w-full max-w-sm space-y-6">
        {/* Header */}
        <div className="text-center space-y-2">
          <div
            className="inline-flex items-center justify-center w-16 h-16 rounded-2xl mb-2"
            style={{ background: "linear-gradient(135deg, var(--p), var(--p2))" }}
          >
            <span className="text-2xl font-black" style={{ color: "var(--bg)" }}>
              PX
            </span>
          </div>
          <h1 className="text-2xl font-bold" style={{ color: "var(--p)" }}>
            PeDitXCDN
          </h1>
          <p className="text-sm" style={{ color: "var(--muted)" }}>
            سرویس CDN اشتراکی
          </p>
        </div>

        {/* Panel URL */}
        <div className="card">
          <label className="text-xs mb-1 block" style={{ color: "var(--muted)" }}>
            آدرس پنل
          </label>
          <input
            type="text"
            value={panelUrl}
            onChange={(e) => setPanelUrl(e.target.value)}
            className="input text-sm"
            placeholder="https://panel.example.com:8443"
          />
        </div>

        {/* Login / Signup form */}
        <form onSubmit={handleSubmit} className="card space-y-4">
          <h2 className="text-lg font-bold" style={{ color: "var(--text)" }}>
            {mode === "login" ? "ورود" : "ثبت‌نام"}
          </h2>

          {mode === "signup" && (
            <div>
              <label className="text-xs mb-1 block" style={{ color: "var(--muted)" }}>
                نام
              </label>
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                className="input"
                placeholder="نام شما"
                required
              />
            </div>
          )}

          <div>
            <label className="text-xs mb-1 block" style={{ color: "var(--muted)" }}>
              نام کاربری
            </label>
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
            <label className="text-xs mb-1 block" style={{ color: "var(--muted)" }}>
              رمز عبور
            </label>
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
            <div
              className="text-sm p-3 rounded-xl"
              style={{
                color: "var(--danger)",
                background: "rgba(255,80,80,0.1)",
                border: "1px solid rgba(255,80,80,0.2)",
              }}
            >
              {localError}
            </div>
          )}

          <button type="submit" disabled={loading} className="btn-primary w-full">
            {loading
              ? "در حال پردازش..."
              : mode === "login"
              ? "ورود"
              : "ثبت‌نام"}
          </button>

          <button
            type="button"
            onClick={() => {
              setMode(mode === "login" ? "signup" : "login");
              setLocalError("");
            }}
            className="btn-ghost w-full text-sm"
          >
            {mode === "login"
              ? "حساب ندارید؟ ثبت‌نام کنید"
              : "حساب دارید؟ وارد شوید"}
          </button>
        </form>
      </div>
    </div>
  );
}
