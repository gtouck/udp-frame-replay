//! 从文件内容反推解析规则。
//!
//! 默认配置是「整行都当数据」，而真实的数据文件几乎总有行首前缀
//! （`[TX] 000123 发送 …`、时间戳、通道号）。结果是新人打开文件的第一眼满屏
//! 都是解析错误 —— 看上去像软件读不了这个文件，其实只是还没配规则。
//!
//! 与其写一段说明让人自己去调，不如直接试出来：候选组合就那么二十来种，
//! 每种取几十行样本跑一遍**真正的解析器**，谁解出的字节最多就是谁。
//! 用的是 `parse::parse_line`，和界面上看到的完全是同一条代码路径，
//! 不存在「推测时按一套规则、显示时按另一套」的偏差。

use serde::Serialize;

use crate::config::{Delimiter, ParseConfig, PrefixRule, TextEncoding};
use crate::parse;
use crate::source::DataSource;

/// 取多少行做样本。足够跨过文件开头可能存在的表头，又不至于拖慢打开。
const SAMPLE: usize = 60;

/// 前缀字段数的搜索上限。超过这个数的前缀在实际数据里没见过。
const MAX_SKIP: usize = 6;

const DELIMITERS: [Delimiter; 3] = [Delimiter::Whitespace, Delimiter::Comma, Delimiter::Tab];

/// 只在 UTF-8 和 GBK 之间选。
///
/// Latin-1 不参与：它把任意字节都映射成某个字符，永远不会产生替换字符，
/// 于是在「谁的乱码少」这个判据下必然赢 —— 但它对中文标识是错的。
/// Latin-1 留作手动逃生口。
const ENCODINGS: [TextEncoding; 2] = [TextEncoding::Utf8, TextEncoding::Gbk];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Guess {
    pub config: ParseConfig,
    /// 说给使用者听的一句话，讲清楚软件替他做了什么决定
    pub summary: String,
}

/// 一个候选跑出来的成绩
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Score {
    /// 这些行一共解出多少字节
    bytes: usize,
    /// 解出了至少一个字节、且没有报错的行数
    ok: usize,
    /// 丢掉了几个前缀字段 —— 只在前两项打平时用来分胜负
    skip: usize,
}

impl Score {
    /// 依次比：解出的字节总量、成功行数、丢掉的前缀字段数。
    ///
    /// 顺序反过来（先比行数）会掉进一个很具体的坑：形如
    /// `[TX] 000021 发送` 的行只有三个字段，正确的「丢 3 个」在它上面必然报错，
    /// 而「丢 1 个」会把序号 `000021` 当成三个字节欣然接受 —— 于是错误答案
    /// 反倒行行成功，赢下比较。真正的信号是**留住了多少数据**：
    /// 切少了会撞上非十六进制字符提前收尾，切多了会把数据本身丢掉，
    /// 两个方向都体现为字节数下降，只有切在正地方字节数才最大。
    /// 第三项是给平局用的。典型场景是带时间戳的 CSV：
    /// `2024-05-01T00:00:01,CH1,01 A5 3F 2B` —— 忽略字符里有 `:` 和 `-`，
    /// 于是「整行都是数据」会把时间戳的数字当成十六进制吃掉一截，
    /// 字节数可能恰好和正确切法打平。这时候倾向剥掉更多前缀：
    /// 两个候选取出的数据一样多，那个「解释掉了更多行首内容」的更可信。
    fn better_than(self, other: Score) -> bool {
        (self.bytes, self.ok, self.skip) > (other.bytes, other.ok, other.skip)
    }
}

fn with_prefix(
    base: &ParseConfig,
    encoding: TextEncoding,
    delimiter: Delimiter,
    skip_fields: usize,
) -> ParseConfig {
    ParseConfig {
        encoding,
        prefix: PrefixRule::Fields {
            delimiter,
            collapse: true,
            skip_fields,
        },
        hex: base.hex.clone(),
    }
}

fn sample_len(src: &DataSource) -> usize {
    src.line_count().min(SAMPLE)
}

fn score(src: &DataSource, cfg: &ParseConfig, skip: usize) -> Score {
    let mut buf = Vec::with_capacity(1024);
    let mut ok = 0;
    let mut bytes = 0;

    for i in 0..sample_len(src) {
        let Some(text) = src.line_text(i, cfg.encoding) else {
            break;
        };
        let (_, err) = parse::parse_line(&text, cfg, &mut buf);
        if err.is_none() && !buf.is_empty() {
            ok += 1;
            bytes += buf.len();
        }
    }

    Score { bytes, ok, skip }
}

/// 样本里出现了多少个替换字符 —— 编码选错的直接信号
fn garble(src: &DataSource, enc: TextEncoding) -> usize {
    (0..sample_len(src))
        .filter_map(|i| src.line_text(i, enc))
        .map(|t| t.chars().filter(|c| *c == char::REPLACEMENT_CHARACTER).count())
        .sum()
}

fn delimiter_label(d: &Delimiter) -> &'static str {
    match d {
        Delimiter::Whitespace => "空白",
        Delimiter::Comma => "逗号",
        Delimiter::Tab => "Tab",
        Delimiter::Custom(_) => "自定义",
    }
}

fn encoding_label(e: TextEncoding) -> &'static str {
    match e {
        TextEncoding::Utf8 => "UTF-8",
        TextEncoding::Gbk => "GBK",
        TextEncoding::Latin1 => "Latin-1",
    }
}

/// 推测解析规则。
///
/// 没有任何组合能解出数据时返回 `None` —— 这种情况下界面保持原配置不动，
/// 让使用者对着原文视图自己调，胡乱改一通反而更难办。
pub fn guess_parse(src: &DataSource, base: &ParseConfig) -> Option<Guess> {
    // 先定编码：乱码最少的那个。一个替换字符都没有就不必再试下一种。
    let mut encoding = TextEncoding::Utf8;
    let mut fewest = usize::MAX;
    for enc in ENCODINGS {
        let n = garble(src, enc);
        if n < fewest {
            fewest = n;
            encoding = enc;
        }
        if fewest == 0 {
            break;
        }
    }

    let mut best: Option<(Score, Delimiter, usize)> = None;
    for delimiter in DELIMITERS {
        for skip in 0..=MAX_SKIP {
            let s = score(src, &with_prefix(base, encoding, delimiter.clone(), skip), skip);
            if s.ok == 0 {
                continue;
            }
            // 不用 is_none_or：那是 1.82 才稳定的 API，crate 声明的 MSRV 是 1.77
            let take = match &best {
                None => true,
                Some((b, _, _)) => s.better_than(*b),
            };
            if take {
                best = Some((s, delimiter.clone(), skip));
            }
        }
    }

    let (_, delimiter, skip) = best?;

    let summary = if skip == 0 {
        format!("已按文件内容推测：{}，整行都是数据", encoding_label(encoding))
    } else {
        format!(
            "已按文件内容推测：{}，按{}丢弃行首 {skip} 个字段",
            encoding_label(encoding),
            delimiter_label(&delimiter),
        )
    };

    Some(Guess {
        config: with_prefix(base, encoding, delimiter, skip),
        summary,
    })
}
