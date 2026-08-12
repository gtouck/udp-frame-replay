import ChromeBar from "./components/ChromeBar";
import ConfigPanel from "./components/ConfigPanel";
import LogView from "./components/LogView";
import SendView from "./components/SendView";
import SourceView from "./components/SourceView";
import StatusBar from "./components/StatusBar";
import { useEnginePolling } from "./useEnginePolling";
import { usePreflight } from "./usePreflight";

export default function App() {
  useEnginePolling();
  usePreflight();

  return (
    <div className="app">
      <ChromeBar />
      <ConfigPanel />
      <div className="screens">
        <SourceView />
        <SendView />
        <LogView />
      </div>
      <StatusBar />
    </div>
  );
}
