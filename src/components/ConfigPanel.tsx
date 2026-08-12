import FilterSection from "./config/FilterSection";
import PacingSection from "./config/PacingSection";
import ParseSection from "./config/ParseSection";
import TargetSection from "./config/TargetSection";
import Group from "./Group";

export default function ConfigPanel() {
  return (
    <aside className="config">
      <Group name="解析规则">
        <ParseSection />
      </Group>

      <Group name="筛选规则">
        <FilterSection />
      </Group>

      <Group name="发送目标">
        <TargetSection />
      </Group>

      <Group name="节奏控制">
        <PacingSection />
      </Group>
    </aside>
  );
}
