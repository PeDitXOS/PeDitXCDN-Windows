import { useAppStore } from "../store";

export function QuotaBar() {
  const userInfo = useAppStore((s) => s.userInfo);

  if (!userInfo?.gb_total) return null;

  const used = userInfo.gb_used ?? 0;
  const total = userInfo.gb_total;
  const pct = Math.min(Math.round((used / total) * 100), 100);

  return (
    <div className="card space-y-3">
      <div className="flex items-center justify-between">
        <span className="text-sm" style={{ color: "var(--muted)" }}>حجم مصرفی</span>
        <span className="text-sm font-medium">
          {used.toFixed(1)} / {total.toFixed(1)} GB
        </span>
      </div>

      {/* Progress bar */}
      <div className="w-full h-2 rounded-full" style={{ background: "var(--border)" }}>
        <div
          className="h-2 rounded-full transition-all duration-500"
          style={{
            width: `${pct}%`,
            background: pct > 90
              ? "var(--danger)"
              : pct > 70
              ? "var(--warn)"
              : "linear-gradient(90deg, var(--p), var(--p2))",
          }}
        />
      </div>

      {/* Stats row */}
      <div className="grid grid-cols-3 gap-2 text-center">
        <div>
          <div className="text-xs" style={{ color: "var(--muted)" }}>روز باقی‌مانده</div>
          <div className="text-sm font-bold" style={{ color: "var(--p)" }}>
            {userInfo.days_left ?? "—"}
          </div>
        </div>
        <div>
          <div className="text-xs" style={{ color: "var(--muted)" }}>سرعت</div>
          <div className="text-sm font-bold" style={{ color: "var(--p)" }}>
            {userInfo.speed_mbps ? `${userInfo.speed_mbps} Mb` : "—"}
          </div>
        </div>
        <div>
          <div className="text-xs" style={{ color: "var(--muted)" }}>پلن</div>
          <div className="text-sm font-bold" style={{ color: "var(--p)" }}>
            {userInfo.plan_name ?? "—"}
          </div>
        </div>
      </div>
    </div>
  );
}
