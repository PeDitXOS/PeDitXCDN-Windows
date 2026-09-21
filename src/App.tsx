import { useAppStore } from "./store";
import { LoginScreen } from "./components/LoginScreen";
import { Dashboard } from "./components/Dashboard";

export default function App() {
  const screen = useAppStore((s) => s.screen);

  return (
    <div className="min-h-screen">
      {screen === "login" ? <LoginScreen /> : <Dashboard />}
    </div>
  );
}
