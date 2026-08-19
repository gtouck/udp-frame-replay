import FilterSection from "./config/FilterSection";
import MutateSection from "./config/MutateSection";
import PreflightPanel from "./config/PreflightPanel";
import ProfileSection from "./config/ProfileSection";
import PacingSection from "./config/PacingSection";
import ParseSection from "./config/ParseSection";
import TargetSection from "./config/TargetSection";
import Group from "./Group";
import OverlayScrollArea from "./OverlayScrollArea";

export default function ConfigPanel() {
  return (
    <aside className="config">
      {/* 发送目标最常改：钉在面板顶上，不折叠也不跟着滚 */}
      <Group name="发送目标" pinned>
        <TargetSection />
      </Group>

      <OverlayScrollArea className="config-scroll">
        <PreflightPanel />

        <Group name="节奏控制">
          <PacingSection />
        </Group>

        <Group name="解析规则">
          <ParseSection />
        </Group>

        <Group name="筛选规则">
          <FilterSection />
        </Group>

        <Group name="修改规则" defaultOpen={false}>
          <MutateSection />
        </Group>

        <Group name="配置档" defaultOpen={false}>
          <ProfileSection />
        </Group>
      </OverlayScrollArea>
    </aside>
  );
}
