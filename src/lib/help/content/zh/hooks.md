# 概述
主动钩子让应用在您最近的 EEG 模式匹配特定关键词或脑状态时自动触发操作。

## 什么是主动钩子？
主动钩子是一条实时监控您最近 EEG 标签嵌入的规则。当您最近脑状态嵌入与钩子关键词嵌入之间的余弦距离低于配置的阈值时，钩子触发——发送命令、显示通知、触发 TTS 或广播 WebSocket 事件。钩子让您无需编写代码即可构建闭环神经反馈自动化。

## 工作原理
应用每隔几秒从您最近的脑数据中计算 EEG 嵌入。这些嵌入通过 HNSW 索引上的余弦相似度与每个活跃钩子中定义的关键词嵌入进行比较。如果任何钩子的距离阈值被满足，钩子就会触发。冷却时间防止同一钩子在短时间内重复触发。匹配完全在本地进行——没有数据离开您的设备。

## 场景
Each hook can be scoped to a scenario — Cognitive, Emotional, Physical, or Any. Cognitive hooks target mental states like focus, distraction, or mental fatigue. Emotional hooks target affective states like stress, calm, or frustration. Physical hooks target bodily states like drowsiness or physical fatigue. 'Any' matches regardless of the inferred scenario category.

# 配置钩子
每个钩子有几个字段控制其触发时机和方式。

## 钩子名称
A descriptive name for the hook (e.g. 'Deep Work Guard', 'Calm Recovery'). The name is used in the history log and WebSocket events. It must be unique across all hooks.

## 关键词
One or more keywords or short phrases that describe the brain state you want to detect (e.g. 'focus', 'deep work', 'stress', 'tired'). These are embedded using the same sentence-transformer model as your EEG labels. The hook fires when recent EEG embeddings are close to these keyword embeddings in the shared vector space.

## 关键词建议
As you type a keyword, the app suggests related terms from your existing label history using both fuzzy string matching and semantic embedding similarity. Suggestions show a source badge — 'fuzzy' for string-based matches, 'semantic' for embedding-based matches, or 'fuzzy+semantic' for both. Use ↑/↓ arrow keys and Enter to quickly accept a suggestion.

## 距离阈值
最近 EEG 嵌入与钩子关键词嵌入之间的最大余弦距离（0–1），达到此值钩子触发。较低的值要求更接近的匹配（更严格），较高的值更频繁触发（更宽松）。典型值范围从 0.08（非常严格）到 0.25（宽松）。从 0.12–0.16 开始，根据建议工具进行调整。

## 距离建议工具
Click 'Suggest threshold' to analyse your recorded EEG data against the hook's keywords. The tool computes the distance distribution (min, p25, p50, p75, max) and recommends a threshold that balances sensitivity and specificity. A visual percentile bar shows where your current and suggested thresholds fall in the distribution. Click 'Apply' to use the suggested value.

## 最近参考数
与钩子关键词进行比较的最近 EEG 嵌入样本数量（默认：12）。较高的值平滑瞬态尖峰但增加检测延迟。较低的值反应更快但可能因短暂伪影而触发。有效范围：10–20。

## 命令
钩子触发时在 WebSocket 事件中广播的可选命令字符串（如 'focus_reset'、'calm_breath'）。在 WebSocket 上监听的外部自动化工具可以对此命令做出反应，触发特定应用的操作、通知或脚本。

## 负载文本
An optional human-readable message included in the hook's fire event (e.g. 'Take a 2-minute break.'). This text is shown in notifications and can be spoken aloud via TTS if voice guidance is enabled.

# 高级
提示、历史和与外部工具的集成。

## 快速示例
The 'Quick examples' panel provides ready-made hook templates for common use cases: Deep Work Guard (cognitive focus reset), Calm Recovery (emotional stress relief), and Body Break (physical fatigue). Click any example to add it as a new hook with pre-filled keywords, scenario, threshold, and payload. Adjust the values to match your personal EEG patterns.

## 钩子触发历史
钩子面板底部的可折叠历史日志记录每次钩子触发事件，包含时间戳、匹配标签、余弦距离、命令和触发时的关键词。使用它来审计钩子行为、验证阈值和调试误报。展开任何行可查看完整详情。分页控件让您浏览旧事件。

## WebSocket 事件
当钩子触发时，应用通过 WebSocket API 广播一个 JSON 事件，包含钩子名称、命令、文本、匹配标签、距离和时间戳。外部客户端可以监听这些事件以构建自定义自动化——例如调暗灯光、暂停音乐、发送 Slack 消息或记录到个人仪表盘。

## 调优提示
Start with one hook and a few keywords that match labels you have already recorded. Use the distance suggestion tool to set an initial threshold. Monitor the history log for a day and adjust: lower the threshold if you see false positives, raise it if the hook never fires. Adding more specific keywords (e.g. 'deep focus reading' vs. 'focus') generally improves precision. Avoid very short or generic single-word keywords unless you want broad matching.
