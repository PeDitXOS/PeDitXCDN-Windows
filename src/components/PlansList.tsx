import { useAppStore } from "../store";

export function PlansList() {
  const plans = useAppStore((s) => s.plans);
  const userInfo = useAppStore((s) => s.userInfo);

  if (!plans.length) return null;

  return (
    <div className="card space-y-3">
      <h3 className="text-sm font-medium" style={{ color: "var(--muted)" }}>پلن‌های موجود</h3>
      <div className="space-y-2">
        {plans.map((plan) => {
          const isCurrent = userInfo?.plan === String(plan.id);
          return (
            <div
              key={plan.id}
              className="flex items-center justify-between p-3 rounded-xl"
              style={{
                background: isCurrent ? "rgba(0,212,170,0.1)" : "var(--bg)",
                border: isCurrent ? "1px solid var(--p)" : "1px solid var(--border)",
              }}
            >
              <div>
                <div className="text-sm font-medium">{plan.name}</div>
                <div className="text-xs" style={{ color: "var(--muted)" }}>
                  {plan.days} روز · {plan.gb} GB · {plan.mbps} Mb/s
                </div>
              </div>
              <div className="text-sm font-bold" style={{ color: "var(--p)" }}>
                {plan.price.toLocaleString("fa-IR")} تومان
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
