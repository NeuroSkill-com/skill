# 设置选项卡
配置设备偏好、信号处理、嵌入参数、校准、快捷键和日志。

## 已配对设备
列出应用程序已发现的所有 BCI 设备。您可以设置首选设备（自动连接目标）、忘记设备或扫描新设备。最近发现的设备会显示 RSSI 信号强度。

## 信号处理
配置实时 EEG 滤波器链：低通截止频率（去除高频噪声）、高通截止频率（去除直流漂移）和工频陷波滤波器（去除 50 或 60 Hz 电源干扰及谐波）。更改立即应用于波形显示和频段功率。

## EEG 嵌入
调整连续 5 秒嵌入时段之间的重叠量。更高的重叠意味着每分钟更多的嵌入（搜索中更精细的时间分辨率），但会增加存储和计算开销。

## 校准
Configure the calibration task: action labels (e.g. "eyes open", "eyes closed"), phase durations, number of repetitions, and whether to auto-start calibration on app launch.

## 校准语音引导（TTS）
During calibration the app announces each phase by name using on-device English text-to-speech. The engine is powered by KittenTTS (tract-onnx, ~30 MB) with espeak-ng phonemisation. The model is downloaded from HuggingFace Hub on first launch and cached locally — no data leaves your device after that. Speech fires for: session start, each action phase, every break ("Break. Next: …"), and session completion. Requires espeak-ng on PATH (brew / apt / apk install espeak-ng). English only.

## 全局快捷键
设置系统级键盘快捷键，可从任何应用程序打开标签、搜索、设置和校准窗口。使用标准加速键格式（如 CmdOrCtrl+Shift+L）。

## 调试日志
切换各子系统的日志记录到 {dataDir}/logs/ 中的每日日志文件。子系统包括嵌入器、设备、WebSocket、CSV、滤波器和频段。

## 更新
检查并安装应用更新。使用 Tauri 内置的更新器和 Ed25519 签名验证。

## 外观
选择颜色模式（跟随系统 / 浅色 / 深色），启用高对比度以获得更醒目的边框和文字，并为 EEG 波形和频段功率可视化选择图表配色方案。提供色盲友好调色板。也可在此通过语言选择器切换语言。

## 目标
设置每日录制目标（分钟）。流式传输期间仪表盘上会显示进度条，达到目标时会触发通知。最近 30 天图表显示哪些天达标（绿色）、达到一半（琥珀色）、有一些进度（暗色）或未记录（无）。

## 文本嵌入
Select the sentence-transformer model used to embed your label text for semantic search. Smaller models (≤384-dim, e.g. all-MiniLM-L6-v2) are fast and sufficient for personal search. Larger models produce richer representations at the cost of download size and inference time. Weights are downloaded once from HuggingFace and cached locally. After switching models, run Re-embed All Labels to reindex.

## 快捷键
配置全局键盘快捷键（系统级热键），用于打开标签、搜索、设置和校准窗口。还显示所有应用内快捷键（⌘K 打开命令面板、? 打开快捷键浮层、⌘↵ 提交标签）。快捷键使用标准加速键格式——如 CmdOrCtrl+Shift+L。

# 活动追踪
NeuroSkill 可以选择性地记录哪个应用处于前台以及键盘和鼠标的最后使用时间。两项功能默认关闭、需手动启用、完全本地化，并可在设置 → 活动追踪中独立配置。

## 活动窗口追踪
后台线程每秒唤醒一次，询问操作系统当前前台应用程序是什么。当应用名称或窗口标题发生变化时，会向 activity.sqlite 插入一行：应用显示名称（如 "Safari"）、应用程序包或可执行文件的完整路径、最前面窗口的标题（如文档名称或当前网页），以及记录该窗口变为活动状态的 Unix 秒时间戳。如果您停留在同一窗口中，则不会写入新行——在单个应用中空闲不会产生数据库活动。在 macOS 上，追踪器调用 osascript；应用名称和路径不需要辅助功能权限，但沙盒应用的窗口标题可能为空。在 Linux 上使用 xdotool 和 xprop（需要 X11 会话）。在 Windows 上使用 PowerShell GetForegroundWindow 调用。

## 键盘和鼠标活动追踪
A global input hook (rdev) listens for every key press and mouse or trackpad event system-wide. It does not record what you typed, which keys you pressed, or where the cursor moved — it only updates two Unix-second timestamps in memory: one for the most recent keyboard event and one for the most recent mouse/trackpad event. These are flushed to activity.sqlite every 60 seconds, but only when at least one value has changed since the last flush, so idle periods leave no trace. The Settings panel receives a live update event (throttled to at most once per second) so the "Last keyboard" and "Last mouse" fields reflect activity in near-real-time.

## 数据存储位置
所有活动数据存储在单个 SQLite 文件中：~/.skill/activity.sqlite。它永远不会被传输、同步或包含在任何分析中。维护两个表：active_windows（每次窗口焦点变化一行，包含应用名称、路径、标题和时间戳）和 input_activity（检测到活动时每 60 秒刷新一行，包含上次键盘和上次鼠标时间戳）。两个表在时间戳列上都有降序索引。启用 WAL 日志模式，因此后台写入永远不会阻塞读取。您可以随时使用任何 SQLite 浏览器打开、检查、导出或删除该文件。

## 所需操作系统权限
macOS——活动窗口追踪（应用名称和路径）不需要特殊权限。键盘和鼠标追踪使用 CGEventTap，需要辅助功能权限：打开系统设置 → 隐私与安全性 → 辅助功能，在列表中找到 NeuroSkill 并开启。没有此权限，输入钩子会静默失败——时间戳保持为零，应用其余部分完全不受影响。您可以在设置 → 活动追踪中禁用该开关以完全阻止权限提示。Linux——两项功能都需要 X11 会话。活动窗口追踪使用 xdotool 和 xprop，大多数桌面发行版已预装。输入追踪使用 libxtst 的 XRecord 扩展。如果缺少任一工具，该功能会记录警告并自行禁用。Windows——不需要特殊权限。活动窗口追踪通过 PowerShell 使用 GetForegroundWindow；输入追踪使用 SetWindowsHookEx。

## 禁用和清除数据
设置 → 活动追踪中的两个开关立即生效——无需重启。禁用活动窗口追踪会停止向 active_windows 插入新行并清除内存中的当前窗口状态。禁用输入追踪会停止 rdev 回调更新时间戳并阻止未来刷新到 input_activity；现有行不会自动删除。要删除所有已收集的历史记录：退出应用，删除 ~/.skill/activity.sqlite，然后重新启动。下次启动时会自动创建空数据库。

# UMAP

## UMAP
控制会话比较中 3D UMAP 投影的参数：邻居数（控制局部与全局结构）、最小距离（点聚类的紧密程度）和度量（余弦或欧几里得）。更高的邻居数保留更多全局拓扑；更低的数值揭示细粒度的局部聚类。投影在后台任务中运行，结果会被缓存。

# EEG 模型选项卡
监控 ZUNA 编码器和 HNSW 向量索引状态。

## 编码器状态
显示 ZUNA wgpu 编码器是否已加载、架构摘要（维度、层数、头数）以及 .safetensors 权重文件的路径。编码器完全在设备端通过 GPU 运行。

## 今日嵌入数
实时计数器，显示今天有多少 5 秒 EEG 时段已嵌入到今天的 HNSW 索引中。每个嵌入是一个紧凑的向量，捕捉该时刻的神经特征。

## HNSW 参数
M（每个节点的连接数）和 ef_construction（构建时的搜索宽度）控制最近邻索引的质量/速度权衡。更高的值提供更好的召回率但使用更多内存。默认值（M=16，ef=200）是良好的平衡。

## 数据归一化
编码前应用于原始 EEG 的 data_norm 缩放因子。默认值（10）针对 Muse 2 / Muse S 头戴设备调优。

# OpenBCI 开发板
连接和配置任何 OpenBCI 开发板——Ganglion、Cyton、Cyton+Daisy、WiFi Shield 变体或 Galea——可单独使用或与其他 BCI 设备同时使用。

## 开发板选择
选择要使用的 OpenBCI 开发板。Ganglion（4 通道，BLE）是最便携的选择。Cyton（8 通道，USB 串口）增加了更多通道。Cyton+Daisy 将通道数翻倍至 16。WiFi Shield 变体将 USB/BLE 链路替换为 1 kHz Wi-Fi 流。Galea（24 通道，UDP）是高密度研究级开发板。所有变体均可单独运行或与其他 BCI 设备同时使用。

## Ganglion BLE
Ganglion 通过蓝牙低功耗连接。按下连接，NeuroSkill™ 会在配置的扫描超时时间内搜索最近的广播 Ganglion。将开发板保持在 3–5 米范围内并保持通电（蓝色 LED 闪烁）。每个蓝牙适配器只能激活一个 Ganglion。如果开发板广播速度较慢，请在设置中延长 BLE 扫描超时时间。

## 串口（Cyton / Cyton+Daisy）
Cyton 开发板通过 USB 无线电加密狗通信。串口字段留空可自动检测第一个可用端口，或手动输入（macOS 上为 /dev/cu.usbserial-…，Linux 上为 /dev/ttyUSB0，Windows 上为 COM3）。点击连接前先插入加密狗，并确保有串口权限——在 Linux 上将用户添加到 dialout 组。

## WiFi Shield
OpenBCI WiFi Shield 创建自己的 2.4 GHz 接入点（SSID：OpenBCI-XXXX）。将计算机连接到该网络，然后将 IP 设置为 192.168.4.1（Shield 的默认网关）。或者，Shield 可以加入您的家庭网络——改为输入其分配的 IP。IP 字段留空将尝试通过 mDNS 自动发现。WiFi Shield 以 1 kHz 流式传输——在信号处理设置中将低通滤波器截止频率设为 ≤ 500 Hz。

## Galea
Galea 是一款 24 通道研究级生物信号头戴设备（EEG + EMG + AUX），通过 UDP 流式传输。输入 Galea 设备的 IP 地址，或留空以接受本地网络上任何发送者的数据包。通道 1–8 为 EEG，驱动实时分析；通道 9–16 为 EMG；17–24 为 AUX。所有 24 个通道均保存至 CSV。

## 通道标签和预设
为每个物理通道分配标准 10-20 电极名称，以便频段功率指标、前额 Alpha 不对称性和电极可视化具有电极感知能力。使用预设（前额、运动、枕部、全 10-20）自动填充标签，或输入自定义名称。超过前 4 个的通道仅记录到 CSV，不驱动实时分析管线。
