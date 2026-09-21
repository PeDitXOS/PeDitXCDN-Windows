import { ConnectButton } from "./components/ConnectButton";
import { StatusCard } from "./components/StatusCard";
import { PanelWebview } from "./components/PanelWebview";

export default function App() {
  return (
    <div className="min-h-screen p-6 max-w-lg mx-auto space-y-6">
      <header className="text-center space-y-1">
        <h1 className="text-2xl font-bold text-brand-400">PeDitXCDN</h1>
        <p className="text-sm text-gray-500">سرویس CDN اشتراکی</p>
      </header>

      <ConnectButton />
      <StatusCard />
      <PanelWebview />
    </div>
  );
}
