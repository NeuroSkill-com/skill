# 窗口
{app} 使用独立窗口执行特定任务。每个窗口都可以从托盘上下文菜单或通过全局键盘快捷键打开。

## 🏷  标签窗口
Opened via the tray menu, global shortcut, or the tag button on the main window. Type a free-text label to annotate the current EEG moment (e.g. "meditation", "focused reading"). The label is saved to {dataDir}/labels.sqlite with the exact timestamp range. Submit with Ctrl/⌘+Enter or click Submit. Press Escape to cancel.

## 🔍  搜索窗口
搜索窗口有三种模式——EEG 相似性、文本和交互式——每种模式以不同方式查询您的录制数据。

## EEG 相似性搜索
Pick a start/end date-time range and run an approximate nearest-neighbour search over all ZUNA embeddings recorded in that window. The HNSW index returns the k most similar 5-second EEG epochs from your entire history, ranked by cosine distance. Lower distance = more similar brain state. Any labels that overlap a result timestamp are shown inline. Useful for finding past moments that `felt` similar to a reference period.

## 文本嵌入搜索
Type any concept, activity, or mental state in plain language (e.g. "deep focus", "anxious", "eyes closed meditation"). Your query is embedded by the same sentence-transformer model used for label indexing and matched against every annotation you have ever written via cosine similarity over the HNSW label index. Results are your own labels ranked by semantic closeness — not keyword matching. You can filter the list and re-sort by date or similarity. A 3D kNN graph visualises the neighbourhood structure: the query node sits at the centre, result labels radiate outward by distance.

## 交互式跨模态搜索
输入自由文本概念，{app} 运行四步跨模态管线：(1) 查询被嵌入为文本向量；(2) 检索 k 个最语义相似的标签（text-k）；(3) 对于每个匹配的标签，计算其平均 EEG 嵌入并用于搜索每日 EEG HNSW 索引以找到 k 个最相似的 EEG 时刻（eeg-k）；(4) 对于每个 EEG 邻居，收集 ±reach 分钟范围内的附近标签（label-k）。结果是一个具有四个节点层的有向图——查询 → 文本匹配 → EEG 邻居 → 发现的标签——渲染为交互式 3D 可视化，可导出为 SVG 或 Graphviz DOT。使用 text-k / eeg-k / label-k 滑块控制图的密度，使用 ±reach 来扩大或缩小时间搜索窗口。

## 🎯  校准窗口
Runs a guided calibration task: alternating action phases (e.g. "eyes open" → break → "eyes closed" → break) for a configurable number of loops. Requires a connected, streaming BCI device. Calibration events are emitted over the Tauri event bus and WebSocket so external tools can synchronise. The timestamp of the last completed calibration is saved in settings.

## ⚙  设置窗口
四个选项卡：设置、快捷键（全局热键、命令面板、应用内按键）、EEG 模型（编码器和 HNSW 状态）。从托盘菜单或主窗口上的齿轮按钮打开。

## ?  帮助窗口
就是这个窗口。{app} 界面每个部分的完整参考——主仪表盘、每个设置选项卡、每个弹出窗口、托盘图标和 WebSocket API。从托盘菜单打开。

## 🧭  设置向导
五步首次运行向导，引导您完成蓝牙配对、头戴设备佩戴和首次校准。首次启动时自动打开；可随时通过命令面板重新打开（⌘K → 设置向导）。

## 🌐  API 状态窗口
A live dashboard showing all currently connected WebSocket clients and a scrollable request log. Displays the server port, protocol, and mDNS discovery info. Includes quick-connect snippets for ws:// and dns-sd. Auto-refreshes every 2 seconds. Open from the tray menu or command palette.

## 🌙 睡眠分期
对于持续 30 分钟或更长的会话，历史记录视图会显示自动生成的睡眠图——根据 Delta、Theta、Alpha 和 Beta 频段功率比分类的睡眠阶段（清醒 / N1 / N2 / N3 / REM）阶梯图。展开历史记录中的任何长会话可查看包含每阶段百分比和持续时间的睡眠图。注意：消费级 BCI 头戴设备（如 Muse）使用 4 个干电极，因此分期是近似的——这不是临床多导睡眠图。

## ⚖  比较窗口
在时间线上选择任意两个时间范围，并排比较它们的平均频段功率分布、放松/参与度评分和前额 Alpha 不对称性。包括睡眠分期、高级指标和 Brain Nebula™——一个 3D UMAP 投影，显示两个时段在高维 EEG 空间中的相似程度。从托盘菜单或命令面板打开（⌘K → 比较）。

# 浮层和命令面板
通过键盘快捷键在每个窗口中可用的快速访问浮层。

## ⌨  命令面板（⌘K / Ctrl+K）
快速访问下拉菜单，列出应用中所有可执行的操作。开始输入以模糊过滤命令，使用 ↑↓ 导航，按 Enter 执行。在每个窗口中都可用。命令包括打开窗口（设置、帮助、搜索、标签、历史、校准）、设备操作（重试连接、打开蓝牙设置）和实用工具（显示快捷键浮层、检查更新）。

## ?  键盘快捷键浮层
在任何窗口中按 ?（文本输入框外）可切换显示所有键盘快捷键的浮动面板——包括在设置 → 快捷键中配置的全局快捷键，以及应用内按键（如 ⌘K 打开命令面板和 ⌘Enter 提交标签）。再次按 ? 或 Esc 关闭。
