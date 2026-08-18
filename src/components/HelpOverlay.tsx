import { useEffect } from "react";

/**
 * 应用内速查卡。
 *
 * 打包后的软件里没有 README，也没有仓库里那份手册 —— 装完就用的人手上
 * 唯一的说明就是界面本身。这里放的是"第一次用需要知道的全部"，
 * 逐条规则的细节仍然归手册管。
 */
export default function HelpOverlay({ onClose }: { onClose: () => void }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="help-backdrop" onClick={onClose}>
      <div
        className="help-card"
        role="dialog"
        aria-modal="true"
        aria-label="快速上手"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="help-head">
          <span className="help-title">快速上手</span>
          <button className="btn btn-slim" onClick={onClose}>
            关闭
          </button>
        </header>

        <div className="help-body">
          <section className="help-sec">
            <h3>五步走完一次发送</h3>
            <ol className="help-steps">
              <li>
                <b>打开文件</b> —— 打开后会自动按内容推测编码和行首前缀，
                推测结果写在状态栏。
              </li>
              <li>
                <b>核对「数据原文」</b> —— 暗色是被丢掉的前缀，亮色是真正会发出去的
                数据。不对就在左边「解析规则」里调，改一个数字立刻重画。
              </li>
              <li>
                <b>填「发送目标」</b> —— 单播填 IP 和端口；组播还要选出站网卡。
              </li>
              <li>
                <b>填「节奏控制」</b> —— 发送间隔、起止行、是否循环。
              </li>
              <li>
                <b>按「开始发送」</b> —— 按钮是灰的就说明还有问题，
                配置面板顶部会写明是哪一条。
              </li>
            </ol>
          </section>

          <section className="help-sec">
            <h3>前缀是怎么剥的</h3>
            <pre className="help-fig">
{`[TX] 000123 发送 01 A5 3F 2B
└────── 前缀 ──────┘└─ 数据 ─┘`}
            </pre>
            <p>
              两种剥法二选一：<b>按字段丢弃</b>（指定分隔符，丢掉行首 N 个字段）、
              <b>按字符跳过</b>（跳过行首 N 个字符，一个汉字算一个）。
              数据里的空白和 <code>: - ,</code> 会被忽略；遇到其他非十六进制字符
              数据就到此为止，其后当尾注，所以行尾写注释不影响使用。
            </p>
          </section>

          <section className="help-sec">
            <h3>三块屏各看什么</h3>
            <dl className="help-dl">
              <dt>数据原文</dt>
              <dd>文件内容加规则标注。被筛选规则排除的行整行变暗。</dd>
              <dt>发送数据</dt>
              <dd>真正发出去的字节。发得快时是采样显示，标题上写着实际已发多少帧。</dd>
              <dt>日志</dt>
              <dd>解析错误按类型聚合，不会被几百万条同类错误刷屏。</dd>
            </dl>
          </section>

          <section className="help-sec">
            <h3>修改规则只需记住一件事</h3>
            <p>
              <b>阶段一</b>（插入、替换、删除）的偏移一律按<b>原始帧</b>数，
              规则之间互不影响。<b>阶段二</b>（序号、时间戳、长度、校验和）
              在结构改完之后、按你排的先后顺序执行。
              <b>校验和要排在最后</b> —— 长度字段通常也在校验范围内，排错了界面会提醒。
            </p>
          </section>

          <section className="help-sec">
            <h3>不顺的时候先看这三处</h3>
            <dl className="help-dl">
              <dt>原文满屏报错</dt>
              <dd>解析规则没配对。点「解析规则」里的<b>自动推测</b>，或手动调丢弃的字段数。</dd>
              <dt>「开始发送」按不动</dt>
              <dd>看配置面板最顶上的红色条，它写着具体是哪一项拦住了。</dd>
              <dt>对面收不到</dt>
              <dd>
                组播先确认出站网卡选的是数据网那一张。macOS 还要在
                系统设置 › 隐私与安全性 › 本地网络 里允许本软件，否则局域网流量会被系统直接拦掉。
              </dd>
            </dl>
          </section>

          <section className="help-sec">
            <h3>两个省事的习惯</h3>
            <p>
              首次验证用慢间隔，或者<b>暂停 → 单步</b>逐帧核对实际发出的字节，确认无误再提速。
              配置会自动记住，下次打开就在；要带到别的机器上，用「配置档」存成 JSON。
            </p>
          </section>
        </div>
      </div>
    </div>
  );
}
