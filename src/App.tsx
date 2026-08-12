import ChromeBar from "./components/ChromeBar";
import ConfigPanel from "./components/ConfigPanel";
import SourceView from "./components/SourceView";
import StatusBar from "./components/StatusBar";

export default function App() {
  return (
    <div className="app">
      <ChromeBar />
      <ConfigPanel />
      <div className="screens">
        <SourceView />
      </div>
      <StatusBar />
    </div>
  );
}
