// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** ZH "help" namespace translations. */
const help: Record<string, string> = {
  "helpTabs.dashboard": "仪表盘",
  "helpTabs.electrodes": "电极",
  "helpTabs.settings": "设置",
  "helpTabs.windows": "窗口",
  "helpTabs.api": "API",
  "helpTabs.privacy": "隐私",
  "helpTabs.references": "参考资料",
  "helpTabs.faq": "常见问题",

  "helpDash.mainWindow": "主窗口",
  "helpDash.mainWindowDesc": "主窗口是主要的仪表盘。它显示实时 EEG 数据、设备状态和信号质量。它始终显示在菜单栏中。",
  "helpDash.statusHero": "状态卡片",
  "helpDash.statusHeroBody":
    "顶部卡片显示 BCI 设备的实时连接状态。彩色圆环和徽章指示设备是否已断开、正在扫描、已连接或蓝牙是否关闭。连接后，会显示设备名称、序列号和 MAC 地址（点击可显示/隐藏）。",
  "helpDash.battery": "电池",
  "helpDash.batteryBody":
    "进度条显示已连接 BCI 头戴设备的当前电池电量。随着电量下降，颜色从绿色（高）经过琥珀色变为红色（低）。",
  "helpDash.signalQuality": "信号质量",
  "helpDash.signalQualityBody":
    "四个颜色编码的点——每个 EEG 电极一个（TP9、AF7、AF8、TP10）。绿色 = 良好的皮肤接触和低噪声。黄色 = 一般（有一些伪影）。红色 = 差（高噪声/电极松动）。灰色 = 无信号。质量通过原始 EEG 数据的滚动 RMS 窗口计算。",
  "helpDash.eegChannelGrid": "EEG 通道网格",
  "helpDash.eegChannelGridBody": "四张卡片显示每个通道的最新采样值（单位：µV），颜色与下方波形图一致。",
  "helpDash.uptimeSamples": "运行时间和采样数",
  "helpDash.uptimeSamplesBody":
    "运行时间计算当前会话开始后的实际秒数。采样数是本次会话中从头戴设备接收的原始 EEG 样本总数。",
  "helpDash.csvRecording": "CSV 录制",
  "helpDash.csvRecordingBody":
    "连接时，REC 指示器显示正在写入 {dataDir}/ 的 CSV 文件名。原始（未过滤）EEG 样本会持续保存——每个会话一个文件。",
  "helpDash.bandPowers": "频段功率",
  "helpDash.bandPowersBody":
    "实时柱状图显示每个标准 EEG 频段的相对功率：Delta（1–4 Hz）、Theta（4–8 Hz）、Alpha（8–13 Hz）、Beta（13–30 Hz）和 Gamma（30–50 Hz）。通过 512 样本 Hann 窗口 FFT 以约 4 Hz 更新。每个通道单独显示。",
  "helpDash.faa": "前额 Alpha 不对称性（FAA）",
  "helpDash.faaBody":
    "一个居中锚定的仪表，显示实时前额 Alpha 不对称性指数：ln(AF8 α) − ln(AF7 α)。正值表示右前额 Alpha 功率更大，与左半球趋近动机相关。负值表示回避倾向。该值通过指数移动平均进行平滑，通常范围为 −1 到 +1。FAA 与每 5 秒嵌入时段一起存储在 eeg.sqlite 中。",
  "helpDash.eegWaveforms": "EEG 波形",
  "helpDash.eegWaveformsBody":
    "所有通道滤波后 EEG 信号的滚动时域图。每条波形下方是频谱图色带，显示随时间变化的频率内容。图表显示最近约 4 秒的数据。",
  "helpDash.gpuUtilisation": "GPU 利用率",
  "helpDash.gpuUtilisationBody":
    "主窗口顶部的小型图表，显示 GPU 编码器和解码器利用率。仅在 ZUNA 嵌入编码器活跃时可见。帮助验证 wgpu 管线是否在运行。",
  "helpDash.trayIconStates": "托盘图标状态",
  "helpDash.trayGrey": "灰色——已断开",
  "helpDash.trayGreyDesc": "蓝牙已开启；未连接 BCI 设备。",
  "helpDash.trayAmber": "琥珀色——扫描中",
  "helpDash.trayAmberDesc": "正在搜索 BCI 设备或尝试连接。",
  "helpDash.trayGreen": "绿色——已连接",
  "helpDash.trayGreenDesc": "正在从 BCI 设备流式传输实时 EEG 数据。",
  "helpDash.trayRed": "红色——蓝牙关闭",
  "helpDash.trayRedDesc": "蓝牙无线电已关闭。无法扫描或连接。",
  "helpDash.community": "社区",
  "helpDash.communityDesc": "加入 NeuroSkill Discord 社区，提问、分享反馈，并与其他用户和开发者交流。",
  "helpDash.discordLink": "加入我们的 Discord",

  "helpSettings.settingsTab": "设置选项卡",
  "helpSettings.settingsTabDesc": "配置设备偏好、信号处理、嵌入参数、校准、快捷键和日志。",
  "helpSettings.pairedDevices": "已配对设备",
  "helpSettings.pairedDevicesBody":
    "列出应用程序已发现的所有 BCI 设备。您可以设置首选设备（自动连接目标）、忘记设备或扫描新设备。最近发现的设备会显示 RSSI 信号强度。",
  "helpSettings.signalProcessing": "信号处理",
  "helpSettings.signalProcessingBody":
    "配置实时 EEG 滤波器链：低通截止频率（去除高频噪声）、高通截止频率（去除直流漂移）和工频陷波滤波器（去除 50 或 60 Hz 电源干扰及谐波）。更改立即应用于波形显示和频段功率。",
  "helpSettings.eegEmbedding": "EEG 嵌入",
  "helpSettings.eegEmbeddingBody":
    "调整连续 5 秒嵌入时段之间的重叠量。更高的重叠意味着每分钟更多的嵌入（搜索中更精细的时间分辨率），但会增加存储和计算开销。",
  "helpSettings.calibration": "校准",
  "helpSettings.calibrationBody": `配置校准任务：动作标签（如"睁眼"、"闭眼"）、阶段持续时间、重复次数，以及是否在应用启动时自动开始校准。`,
  "helpSettings.calibrationTts": "校准语音引导（TTS）",
  "helpSettings.calibrationTtsBody": `校准期间，应用使用设备端英语文本转语音来朗读每个阶段名称。引擎基于 KittenTTS（tract-onnx，约 30 MB），使用 espeak-ng 音素化。模型在首次启动时从 HuggingFace Hub 下载并在本地缓存——之后不会有数据离开您的设备。语音触发场景包括：会话开始、每个动作阶段、每次休息（"休息。下一个：…"）和会话完成。需要 espeak-ng 在 PATH 中（brew / apt / apk install espeak-ng）。仅支持英语。`,
  "helpSettings.globalShortcuts": "全局快捷键",
  "helpSettings.globalShortcutsBody":
    "设置系统级键盘快捷键，可从任何应用程序打开标签、搜索、设置和校准窗口。使用标准加速键格式（如 CmdOrCtrl+Shift+L）。",
  "helpSettings.debugLogging": "调试日志",
  "helpSettings.debugLoggingBody":
    "切换各子系统的日志记录到 {dataDir}/logs/ 中的每日日志文件。子系统包括嵌入器、设备、WebSocket、CSV、滤波器和频段。",
  "helpSettings.updates": "更新",
  "helpSettings.updatesBody": "检查并安装应用更新。使用 Tauri 内置的更新器和 Ed25519 签名验证。",
  "helpSettings.appearanceTab": "外观",
  "helpSettings.appearanceTabBody":
    "选择颜色模式（跟随系统 / 浅色 / 深色），启用高对比度以获得更醒目的边框和文字，并为 EEG 波形和频段功率可视化选择图表配色方案。提供色盲友好调色板。也可在此通过语言选择器切换语言。",
  "helpSettings.goalsTab": "目标",
  "helpSettings.goalsTabBody":
    "设置每日录制目标（分钟）。流式传输期间仪表盘上会显示进度条，达到目标时会触发通知。最近 30 天图表显示哪些天达标（绿色）、达到一半（琥珀色）、有一些进度（暗色）或未记录（无）。",
  "helpSettings.embeddingsTab": "文本嵌入",
  "helpSettings.embeddingsTabBody": `选择用于嵌入标签文本进行语义搜索的 sentence-transformer 模型。较小的模型（≤384 维，如 all-MiniLM-L6-v2）速度快，足以满足个人搜索需求。较大的模型产生更丰富的表示，但下载大小和推理时间更多。权重从 HuggingFace 下载一次并在本地缓存。切换模型后，运行"重新嵌入所有标签"以重新索引。`,
  "helpSettings.shortcutsTab": "快捷键",
  "helpSettings.shortcutsTabBody":
    "配置全局键盘快捷键（系统级热键），用于打开标签、搜索、设置和校准窗口。还显示所有应用内快捷键（⌘K 打开命令面板、? 打开快捷键浮层、⌘↵ 提交标签）。快捷键使用标准加速键格式——如 CmdOrCtrl+Shift+L。",
  "helpSettings.activitySection": "活动追踪",
  "helpSettings.activitySectionDesc":
    "NeuroSkill 可以选择性地记录哪个应用处于前台以及键盘和鼠标的最后使用时间。两项功能默认关闭、需手动启用、完全本地化，并可在设置 → 活动追踪中独立配置。",
  "helpSettings.activeWindowHelp": "活动窗口追踪",
  "helpSettings.activeWindowHelpBody":
    '后台线程每秒唤醒一次，询问操作系统当前前台应用程序是什么。当应用名称或窗口标题发生变化时，会向 activity.sqlite 插入一行：应用显示名称（如 "Safari"）、应用程序包或可执行文件的完整路径、最前面窗口的标题（如文档名称或当前网页），以及记录该窗口变为活动状态的 Unix 秒时间戳。如果您停留在同一窗口中，则不会写入新行——在单个应用中空闲不会产生数据库活动。在 macOS 上，追踪器调用 osascript；应用名称和路径不需要辅助功能权限，但沙盒应用的窗口标题可能为空。在 Linux 上使用 xdotool 和 xprop（需要 X11 会话）。在 Windows 上使用 PowerShell GetForegroundWindow 调用。',
  "helpSettings.inputActivityHelp": "键盘和鼠标活动追踪",
  "helpSettings.inputActivityHelpBody": `全局输入钩子（rdev）在系统范围内监听每次按键和鼠标或触控板事件。它不记录您输入了什么、按了哪些键或光标移动到哪里——它只更新内存中的两个 Unix 秒时间戳：一个用于最近的键盘事件，一个用于最近的鼠标/触控板事件。这些数据每 60 秒刷新到 activity.sqlite，但仅在自上次刷新以来至少有一个值发生变化时才会写入，因此空闲期间不会留下任何痕迹。设置面板接收实时更新事件（最多每秒一次），因此"上次键盘"和"上次鼠标"字段近乎实时地反映活动。`,
  "helpSettings.activityStorageHelp": "数据存储位置",
  "helpSettings.activityStorageHelpBody":
    "所有活动数据存储在单个 SQLite 文件中：~/.skill/activity.sqlite。它永远不会被传输、同步或包含在任何分析中。维护两个表：active_windows（每次窗口焦点变化一行，包含应用名称、路径、标题和时间戳）和 input_activity（检测到活动时每 60 秒刷新一行，包含上次键盘和上次鼠标时间戳）。两个表在时间戳列上都有降序索引。启用 WAL 日志模式，因此后台写入永远不会阻塞读取。您可以随时使用任何 SQLite 浏览器打开、检查、导出或删除该文件。",
  "helpSettings.activityPermissionsHelp": "所需操作系统权限",
  "helpSettings.activityPermissionsHelpBody":
    "macOS——活动窗口追踪（应用名称和路径）不需要特殊权限。键盘和鼠标追踪使用 CGEventTap，需要辅助功能权限：打开系统设置 → 隐私与安全性 → 辅助功能，在列表中找到 NeuroSkill 并开启。没有此权限，输入钩子会静默失败——时间戳保持为零，应用其余部分完全不受影响。您可以在设置 → 活动追踪中禁用该开关以完全阻止权限提示。Linux——两项功能都需要 X11 会话。活动窗口追踪使用 xdotool 和 xprop，大多数桌面发行版已预装。输入追踪使用 libxtst 的 XRecord 扩展。如果缺少任一工具，该功能会记录警告并自行禁用。Windows——不需要特殊权限。活动窗口追踪通过 PowerShell 使用 GetForegroundWindow；输入追踪使用 SetWindowsHookEx。",
  "helpSettings.activityDisablingHelp": "禁用和清除数据",
  "helpSettings.activityDisablingHelpBody":
    "设置 → 活动追踪中的两个开关立即生效——无需重启。禁用活动窗口追踪会停止向 active_windows 插入新行并清除内存中的当前窗口状态。禁用输入追踪会停止 rdev 回调更新时间戳并阻止未来刷新到 input_activity；现有行不会自动删除。要删除所有已收集的历史记录：退出应用，删除 ~/.skill/activity.sqlite，然后重新启动。下次启动时会自动创建空数据库。",
  "helpSettings.umapTab": "UMAP",
  "helpSettings.umapTabBody":
    "控制会话比较中 3D UMAP 投影的参数：邻居数（控制局部与全局结构）、最小距离（点聚类的紧密程度）和度量（余弦或欧几里得）。更高的邻居数保留更多全局拓扑；更低的数值揭示细粒度的局部聚类。投影在后台任务中运行，结果会被缓存。",
  "helpSettings.eegModelTab": "EEG 模型选项卡",
  "helpSettings.eegModelTabDesc": "监控 ZUNA 编码器和 HNSW 向量索引状态。",
  "helpSettings.encoderStatus": "编码器状态",
  "helpSettings.encoderStatusBody":
    "显示 ZUNA wgpu 编码器是否已加载、架构摘要（维度、层数、头数）以及 .safetensors 权重文件的路径。编码器完全在设备端通过 GPU 运行。",
  "helpSettings.embeddingsToday": "今日嵌入数",
  "helpSettings.embeddingsTodayBody":
    "实时计数器，显示今天有多少 5 秒 EEG 时段已嵌入到今天的 HNSW 索引中。每个嵌入是一个紧凑的向量，捕捉该时刻的神经特征。",
  "helpSettings.hnswParams": "HNSW 参数",
  "helpSettings.hnswParamsBody":
    "M（每个节点的连接数）和 ef_construction（构建时的搜索宽度）控制最近邻索引的质量/速度权衡。更高的值提供更好的召回率但使用更多内存。默认值（M=16，ef=200）是良好的平衡。",
  "helpSettings.dataNorm": "数据归一化",
  "helpSettings.dataNormBody":
    "编码前应用于原始 EEG 的 data_norm 缩放因子。默认值（10）针对 Muse 2 / Muse S 头戴设备调优。",
  "helpSettings.openbciSection": "OpenBCI 开发板",
  "helpSettings.openbciSectionDesc":
    "连接和配置任何 OpenBCI 开发板——Ganglion、Cyton、Cyton+Daisy、WiFi Shield 变体或 Galea——可单独使用或与其他 BCI 设备同时使用。",
  "helpSettings.openbciBoard": "开发板选择",
  "helpSettings.openbciBoardBody":
    "选择要使用的 OpenBCI 开发板。Ganglion（4 通道，BLE）是最便携的选择。Cyton（8 通道，USB 串口）增加了更多通道。Cyton+Daisy 将通道数翻倍至 16。WiFi Shield 变体将 USB/BLE 链路替换为 1 kHz Wi-Fi 流。Galea（24 通道，UDP）是高密度研究级开发板。所有变体均可单独运行或与其他 BCI 设备同时使用。",
  "helpSettings.openbciGanglion": "Ganglion BLE",
  "helpSettings.openbciGanglionBody":
    "Ganglion 通过蓝牙低功耗连接。按下连接，NeuroSkill™ 会在配置的扫描超时时间内搜索最近的广播 Ganglion。将开发板保持在 3–5 米范围内并保持通电（蓝色 LED 闪烁）。每个蓝牙适配器只能激活一个 Ganglion。如果开发板广播速度较慢，请在设置中延长 BLE 扫描超时时间。",
  "helpSettings.openbciSerial": "串口（Cyton / Cyton+Daisy）",
  "helpSettings.openbciSerialBody":
    "Cyton 开发板通过 USB 无线电加密狗通信。串口字段留空可自动检测第一个可用端口，或手动输入（macOS 上为 /dev/cu.usbserial-…，Linux 上为 /dev/ttyUSB0，Windows 上为 COM3）。点击连接前先插入加密狗，并确保有串口权限——在 Linux 上将用户添加到 dialout 组。",
  "helpSettings.openbciWifi": "WiFi Shield",
  "helpSettings.openbciWifiBody":
    "OpenBCI WiFi Shield 创建自己的 2.4 GHz 接入点（SSID：OpenBCI-XXXX）。将计算机连接到该网络，然后将 IP 设置为 192.168.4.1（Shield 的默认网关）。或者，Shield 可以加入您的家庭网络——改为输入其分配的 IP。IP 字段留空将尝试通过 mDNS 自动发现。WiFi Shield 以 1 kHz 流式传输——在信号处理设置中将低通滤波器截止频率设为 ≤ 500 Hz。",
  "helpSettings.openbciGalea": "Galea",
  "helpSettings.openbciGaleaBody":
    "Galea 是一款 24 通道研究级生物信号头戴设备（EEG + EMG + AUX），通过 UDP 流式传输。输入 Galea 设备的 IP 地址，或留空以接受本地网络上任何发送者的数据包。通道 1–8 为 EEG，驱动实时分析；通道 9–16 为 EMG；17–24 为 AUX。所有 24 个通道均保存至 CSV。",
  "helpSettings.openbciChannels": "通道标签和预设",
  "helpSettings.openbciChannelsBody":
    "为每个物理通道分配标准 10-20 电极名称，以便频段功率指标、前额 Alpha 不对称性和电极可视化具有电极感知能力。使用预设（前额、运动、枕部、全 10-20）自动填充标签，或输入自定义名称。超过前 4 个的通道仅记录到 CSV，不驱动实时分析管线。",

  "helpWindows.title": "窗口",
  "helpWindows.desc": "{app} 使用独立窗口执行特定任务。每个窗口都可以从托盘上下文菜单或通过全局键盘快捷键打开。",
  "helpWindows.labelTitle": "🏷  标签窗口",
  "helpWindows.labelBody": `通过托盘菜单、全局快捷键或主窗口上的标签按钮打开。输入自由文本标签来标注当前 EEG 时刻（如"冥想"、"专注阅读"）。标签会连同精确时间范围保存到 {dataDir}/labels.sqlite。使用 Ctrl/⌘+Enter 提交或点击提交。按 Escape 取消。`,
  "helpWindows.searchTitle": "🔍  搜索窗口",
  "helpWindows.searchBody": "搜索窗口有三种模式——EEG 相似性、文本和交互式——每种模式以不同方式查询您的录制数据。",
  "helpWindows.searchEegTitle": "EEG 相似性搜索",
  "helpWindows.searchEegBody": `选择起止日期时间范围，并对该窗口内记录的所有 ZUNA 嵌入运行近似最近邻搜索。HNSW 索引返回您整个历史中最相似的 k 个 5 秒 EEG 时段，按余弦距离排序。距离越低 = 脑状态越相似。与结果时间戳重叠的任何标签都会内联显示。适用于查找过去与参考时段"感觉"相似的时刻。`,
  "helpWindows.searchTextTitle": "文本嵌入搜索",
  "helpWindows.searchTextBody": `用自然语言输入任何概念、活动或心理状态（如"深度专注"、"焦虑"、"闭眼冥想"）。您的查询由与标签索引相同的 sentence-transformer 模型嵌入，并通过 HNSW 标签索引上的余弦相似度与您写过的每条标注进行匹配。结果是按语义接近程度排序的您自己的标签——而非关键词匹配。您可以过滤列表并按日期或相似度重新排序。3D kNN 图可视化邻域结构：查询节点位于中心，结果标签按距离向外辐射。`,
  "helpWindows.searchInteractiveTitle": "交互式跨模态搜索",
  "helpWindows.searchInteractiveBody":
    "输入自由文本概念，{app} 运行四步跨模态管线：(1) 查询被嵌入为文本向量；(2) 检索 k 个最语义相似的标签（text-k）；(3) 对于每个匹配的标签，计算其平均 EEG 嵌入并用于搜索每日 EEG HNSW 索引以找到 k 个最相似的 EEG 时刻（eeg-k）；(4) 对于每个 EEG 邻居，收集 ±reach 分钟范围内的附近标签（label-k）。结果是一个具有四个节点层的有向图——查询 → 文本匹配 → EEG 邻居 → 发现的标签——渲染为交互式 3D 可视化，可导出为 SVG 或 Graphviz DOT。使用 text-k / eeg-k / label-k 滑块控制图的密度，使用 ±reach 来扩大或缩小时间搜索窗口。",
  "helpWindows.calTitle": "🎯  校准窗口",
  "helpWindows.calBody": `运行引导式校准任务：交替动作阶段（如"睁眼" → 休息 → "闭眼" → 休息），循环可配置的次数。需要已连接且正在流式传输的 BCI 设备。校准事件通过 Tauri 事件总线和 WebSocket 发送，以便外部工具可以同步。最后一次完成校准的时间戳保存在设置中。`,
  "helpWindows.settingsTitle": "⚙  设置窗口",
  "helpWindows.settingsBody":
    "四个选项卡：设置、快捷键（全局热键、命令面板、应用内按键）、EEG 模型（编码器和 HNSW 状态）。从托盘菜单或主窗口上的齿轮按钮打开。",
  "helpWindows.helpTitle": "?  帮助窗口",
  "helpWindows.helpBody":
    "就是这个窗口。{app} 界面每个部分的完整参考——主仪表盘、每个设置选项卡、每个弹出窗口、托盘图标和 WebSocket API。从托盘菜单打开。",
  "helpWindows.onboardingTitle": "🧭  设置向导",
  "helpWindows.onboardingBody":
    "五步首次运行向导，引导您完成蓝牙配对、头戴设备佩戴和首次校准。首次启动时自动打开；可随时通过命令面板重新打开（⌘K → 设置向导）。",
  "helpWindows.apiTitle": "🌐  API 状态窗口",
  "helpWindows.apiBody": `实时仪表盘，显示所有当前连接的 WebSocket 客户端和可滚动的请求日志。显示服务器端口、协议和 mDNS 发现信息。包含 ws:// 和 dns-sd 的快速连接代码片段。每 2 秒自动刷新。从托盘菜单或命令面板打开。`,
  "helpWindows.sleepTitle": "🌙 睡眠分期",
  "helpWindows.sleepBody":
    "对于持续 30 分钟或更长的会话，历史记录视图会显示自动生成的睡眠图——根据 Delta、Theta、Alpha 和 Beta 频段功率比分类的睡眠阶段（清醒 / N1 / N2 / N3 / REM）阶梯图。展开历史记录中的任何长会话可查看包含每阶段百分比和持续时间的睡眠图。注意：消费级 BCI 头戴设备（如 Muse）使用 4 个干电极，因此分期是近似的——这不是临床多导睡眠图。",
  "helpWindows.compareTitle": "⚖  比较窗口",
  "helpWindows.compareBody":
    "在时间线上选择任意两个时间范围，并排比较它们的平均频段功率分布、放松/参与度评分和前额 Alpha 不对称性。包括睡眠分期、高级指标和 Brain Nebula™——一个 3D UMAP 投影，显示两个时段在高维 EEG 空间中的相似程度。从托盘菜单或命令面板打开（⌘K → 比较）。",
  "helpWindows.overlaysTitle": "浮层和命令面板",
  "helpWindows.overlaysDesc": "通过键盘快捷键在每个窗口中可用的快速访问浮层。",
  "helpWindows.cmdPaletteTitle": "⌨  命令面板（⌘K / Ctrl+K）",
  "helpWindows.cmdPaletteBody":
    "快速访问下拉菜单，列出应用中所有可执行的操作。开始输入以模糊过滤命令，使用 ↑↓ 导航，按 Enter 执行。在每个窗口中都可用。命令包括打开窗口（设置、帮助、搜索、标签、历史、校准）、设备操作（重试连接、打开蓝牙设置）和实用工具（显示快捷键浮层、检查更新）。",
  "helpWindows.shortcutsOverlayTitle": "?  键盘快捷键浮层",
  "helpWindows.shortcutsOverlayBody":
    "在任何窗口中按 ?（文本输入框外）可切换显示所有键盘快捷键的浮动面板——包括在设置 → 快捷键中配置的全局快捷键，以及应用内按键（如 ⌘K 打开命令面板和 ⌘Enter 提交标签）。再次按 ? 或 Esc 关闭。",

  "help.searchPlaceholder": "搜索帮助…",
  "help.searchNoResults": `未找到"{query}"的结果`,

  "helpApi.overview": "概述",
  "helpApi.liveStreaming": "实时流式传输",
  "helpApi.liveStreamingBody":
    "{app} 通过本地 WebSocket 服务器流式传输派生的 EEG 指标和设备状态。广播事件包括：eeg-bands（约 4 Hz — 60 多项评分）、device-status（约 1 Hz — 电池、连接状态）和 label-created。原始 EEG/PPG/IMU 样本不通过 WebSocket API 提供。该服务通过 Bonjour/mDNS 以 _skill._tcp 广播，以便客户端自动发现。",
  "helpApi.commands": "命令",
  "helpApi.commandsBody":
    '客户端可以通过 WebSocket 发送 JSON 命令：status（完整系统快照）、calibrate（打开校准）、label（提交标注）、search（最近邻查询）、sessions（列出录制）、compare（A/B 指标 + 睡眠 + UMAP）、sleep（睡眠分期）、umap/umap_poll（3D 嵌入投影）。响应以 JSON 形式在同一连接上到达，包含 "ok" 布尔值。',
  "helpApi.commandReference": "命令参考",
  "helpApi.discoveryWireFormat": "发现与传输格式",
  "helpApi.discoverService": "发现服务",
  "helpApi.outboundEvents": "出站事件（服务器 → 客户端）",
  "helpApi.inboundCommands": "入站命令（客户端 → 服务器）",
  "helpApi.response": "响应",
  "helpApi.cmdStatus": "status",
  "helpApi.cmdStatusParams": "_(无)_",
  "helpApi.cmdStatusDesc":
    "返回设备状态、会话信息、嵌入计数（今日和全部）、标签数量、上次校准时间戳以及每通道信号质量。",
  "helpApi.cmdCalibrate": "calibrate",
  "helpApi.cmdCalibrateParams": "_(无)_",
  "helpApi.cmdCalibrateDesc": "打开校准窗口。需要已连接且正在流式传输的设备。",
  "helpApi.cmdLabel": "label",
  "helpApi.cmdLabelParams": "text（字符串，必填）；label_start_utc（u64，可选——默认为当前时间）",
  "helpApi.cmdLabelDesc": "向标签数据库插入一个带时间戳的标签。返回新的 label_id。",
  "helpApi.cmdSearch": "search",
  "helpApi.cmdSearchParams": "start_utc、end_utc（u64，必填）；k、ef（u64，可选）",
  "helpApi.cmdSearchDesc": "在给定时间范围内搜索 HNSW 嵌入索引中的 k 个最近邻。",
  "helpApi.cmdCompare": "compare",
  "helpApi.cmdCompareParams": "a_start_utc、a_end_utc、b_start_utc、b_end_utc（u64，必填）",
  "helpApi.cmdCompareDesc":
    "通过返回每个时间范围的聚合频段功率指标（相对功率、放松/参与度评分和 FAA）来比较两个时间范围。返回 { a: SessionMetrics, b: SessionMetrics }。",
  "helpApi.cmdSessions": "sessions",
  "helpApi.cmdSessionsParams": "_(无)_",
  "helpApi.cmdSessionsDesc":
    "列出从每日 eeg.sqlite 数据库中发现的所有嵌入会话。会话是连续录制范围（间隔 > 2 分钟 = 新会话）。按最新优先返回。",
  "helpApi.cmdSleep": "sleep",
  "helpApi.cmdSleepParams": "start_utc、end_utc（u64，必填）",
  "helpApi.cmdSleepDesc":
    "使用频段功率比将时间范围内的每个嵌入时段分类为睡眠阶段（Wake/N1/N2/N3/REM），并返回包含每阶段摘要的睡眠图。",
  "helpApi.cmdUmap": "umap",
  "helpApi.cmdUmapParams": "a_start_utc、a_end_utc、b_start_utc、b_end_utc（u64，必填）",
  "helpApi.cmdUmapDesc": "将两个会话嵌入的 3D UMAP 投影加入队列。返回用于轮询的 job_id。非阻塞。",
  "helpApi.cmdUmapPoll": "umap_poll",
  "helpApi.cmdUmapPollParams": "job_id（字符串，必填）",
  "helpApi.cmdUmapPollDesc": "轮询先前入队的 UMAP 任务的结果。返回 { status: 'pending' | 'done', points?: [...] }。",

  "helpPrivacy.overview": "隐私概述",
  "helpPrivacy.overviewDesc":
    "{app} 设计为完全本地优先。您的 EEG 数据、嵌入、标签和设置永远不会离开您的设备，除非您明确选择共享。",
  "helpPrivacy.dataStorage": "数据存储",
  "helpPrivacy.allLocal": "所有数据都保留在您的设备上",
  "helpPrivacy.allLocalBody":
    "{app} 记录的每一项数据——原始 EEG 样本（CSV）、ZUNA 嵌入（SQLite + HNSW 索引）、文本标签、校准时间戳、日志和设置——都本地存储在 {dataDir}/ 中。没有数据上传到任何云服务、服务器或第三方。",
  "helpPrivacy.noAccounts": "无需用户账户",
  "helpPrivacy.noAccountsBody":
    "{app} 不需要注册、登录或任何形式的账户创建。不存储或传输任何用户标识符、令牌或身份验证凭据。",
  "helpPrivacy.dataLocation": "数据位置",
  "helpPrivacy.dataLocationBody":
    "所有文件存储在 macOS 和 Linux 上的 {dataDir}/ 下。每个录制日期都有自己的 YYYYMMDD 子目录，包含 EEG SQLite 数据库和 HNSW 向量索引。标签在 {dataDir}/labels.sqlite 中。日志在 {dataDir}/logs/ 中。您可以随时删除这些文件中的任何一个。",
  "helpPrivacy.network": "网络活动",
  "helpPrivacy.noTelemetry": "无遥测或分析",
  "helpPrivacy.noTelemetryBody":
    "{app} 不收集使用分析、崩溃报告、遥测或任何形式的行为追踪。应用中没有嵌入分析 SDK、追踪像素或回传信标。",
  "helpPrivacy.localWs": "仅限本地的 WebSocket 服务器",
  "helpPrivacy.localWsBody":
    "{app} 运行一个绑定到本地网络接口的 WebSocket 服务器，用于向局域网配套工具流式传输。此服务器不暴露到互联网。它向同一本地网络上的客户端广播派生的 EEG 指标（频段功率、评分、心率）和状态更新。不广播原始 EEG/PPG/IMU 样本流。",
  "helpPrivacy.mdns": "mDNS / Bonjour 服务",
  "helpPrivacy.mdnsBody":
    "{app} 注册一个 _skill._tcp.local. mDNS 服务，以便局域网客户端自动发现 WebSocket 端口。此广播仅限本地（组播 DNS），在您的网络外部不可见。",
  "helpPrivacy.updateChecks": "更新检查",
  "helpPrivacy.updateChecksBody": `当您在设置中点击"检查更新"时，{app} 会联系配置的更新端点以检查是否有更新版本。这是应用发出的唯一出站互联网请求，且仅在您明确触发时才会发生。更新包在安装前使用 Ed25519 签名进行验证。`,
  "helpPrivacy.bluetooth": "蓝牙与设备安全",
  "helpPrivacy.ble": "蓝牙低功耗（BLE）",
  "helpPrivacy.bleBody":
    "{app} 通过蓝牙低功耗或 USB 串口与您的 BCI 设备通信。连接使用标准的 CoreBluetooth（macOS）或 BlueZ（Linux）系统栈。不会安装自定义蓝牙驱动程序或内核模块。",
  "helpPrivacy.osPermissions": "操作系统级权限",
  "helpPrivacy.osPermissionsBody":
    "蓝牙访问需要明确的系统权限。在 macOS 上，您必须在系统设置 → 隐私与安全性 → 蓝牙中授予蓝牙访问权限。{app} 未经您的同意无法访问蓝牙。",
  "helpPrivacy.deviceIds": "设备标识符",
  "helpPrivacy.deviceIdsBody":
    "设备序列号和 MAC 地址从 BCI 头戴设备接收并显示在界面中。这些标识符仅存储在本地设置文件中，永远不会通过网络传输。",
  "helpPrivacy.onDevice": "设备端处理",
  "helpPrivacy.gpuLocal": "GPU 推理保持本地",
  "helpPrivacy.gpuLocalBody":
    "ZUNA 嵌入编码器完全通过 wgpu 在本地 GPU 上运行。模型权重从本地 Hugging Face 缓存（~/.cache/huggingface/）加载。没有 EEG 数据发送到任何外部推理 API 或云 GPU。",
  "helpPrivacy.filtering": "滤波和分析",
  "helpPrivacy.filteringBody":
    "所有信号处理——重叠保存滤波、FFT 频段功率计算、频谱图生成和信号质量监控——都在您的 CPU/GPU 上本地运行。没有原始或处理过的 EEG 数据离开您的设备。",
  "helpPrivacy.nnSearch": "最近邻搜索",
  "helpPrivacy.nnSearchBody":
    "用于相似性搜索的 HNSW 向量索引完全在您的设备上构建和查询。搜索查询永远不会离开您的设备。",
  "helpPrivacy.yourData": "您的数据，您做主",
  "helpPrivacy.access": "访问",
  "helpPrivacy.accessBody":
    "您的所有数据都在 {dataDir}/ 中，采用标准格式（CSV、SQLite、二进制 HNSW）。您可以使用任何工具读取、复制或处理它。",
  "helpPrivacy.delete": "删除",
  "helpPrivacy.deleteBody":
    "随时删除 {dataDir}/ 下的任何文件或目录。无需担心云备份。卸载应用仅删除应用程序二进制文件——{dataDir}/ 中的数据不受影响，除非您自己删除。",
  "helpPrivacy.export": "导出",
  "helpPrivacy.exportBody":
    "CSV 录制和 SQLite 数据库是可移植的标准格式。将它们复制到任何设备或导入 Python、R、MATLAB 或任何分析工具。",
  "helpPrivacy.encrypt": "加密",
  "helpPrivacy.encryptBody":
    "{app} 不对静态数据进行加密。如果您需要磁盘级加密，请使用操作系统的全盘加密功能（macOS 上的 FileVault，Linux 上的 LUKS）。",
  "helpPrivacy.activityTracking": "活动追踪",
  "helpPrivacy.activityTrackingBody":
    "启用后，NeuroSkill 记录哪个应用处于前台以及键盘和鼠标的最后使用时间。此数据完全保留在您的设备上的 ~/.skill/activity.sqlite 中——永远不会发送到任何服务器、远程记录或包含在任何形式的分析中。活动窗口追踪捕获：应用名称、可执行文件路径、窗口标题以及该窗口变为活动状态的 Unix 时间戳。键盘和鼠标追踪仅捕获两个时间戳（上次键盘事件、上次鼠标事件）——绝不包括按键、输入的文本、光标坐标或点击目标。两项功能可在设置 → 活动追踪中独立禁用；禁用后立即停止收集。现有行不会自动删除，但您可以随时通过删除 activity.sqlite 来移除。",
  "helpPrivacy.activityPermission": "辅助功能权限（macOS）",
  "helpPrivacy.activityPermissionBody":
    "在 macOS 上，键盘和鼠标追踪需要辅助功能权限，因为它安装了 CGEventTap——一个拦截输入事件的系统级钩子。Apple 要求任何读取全局输入的应用都具有此权限。仅在功能启用时请求该权限。如果您拒绝或撤销它，钩子会静默失败：应用其余部分继续正常运行，仅输入活动时间戳保持为零。活动窗口追踪（应用名称/路径）不需要辅助功能权限——它使用 AppleScript/osascript，在正常应用权限范围内运行。",
  "helpPrivacy.summaryTitle": "摘要",
  "helpPrivacy.summaryNoCloud": "无云端。所有 EEG 数据、嵌入、标签和设置本地存储在 {dataDir}/ 中。",
  "helpPrivacy.summaryNoTelemetry": "无遥测。没有分析、崩溃报告或任何形式的使用追踪。",
  "helpPrivacy.summaryNoAccounts": "无账户。无需注册、登录或用户标识符。",
  "helpPrivacy.summaryOneReq": "一个可选的网络请求。仅在您明确触发时检查更新。",
  "helpPrivacy.summaryOnDevice": "完全设备端。GPU 推理、信号处理和搜索全部在本地运行。",
  "helpPrivacy.summaryActivityLocal":
    "活动追踪仅限本地。窗口焦点和输入时间戳写入设备上的 activity.sqlite，永远不会离开设备。",

  "helpFaq.title": "常见问题",
  "helpFaq.q1": "我的数据存储在哪里？",
  "helpFaq.a1":
    "所有内容本地存储在 {dataDir}/ 中——原始 CSV 录制、HNSW 向量索引、嵌入 SQLite 数据库、标签、日志和设置。没有任何内容发送到云端。",
  "helpFaq.q2": "ZUNA 编码器做什么？",
  "helpFaq.a2":
    "ZUNA 是一个 GPU 加速的 Transformer 编码器，将 5 秒 EEG 时段转换为紧凑的嵌入向量。这些向量捕捉每个时刻的神经特征，并驱动相似性搜索功能。",
  "helpFaq.q3": "为什么校准需要已连接的设备？",
  "helpFaq.a3":
    "校准运行一个计时任务（如睁眼/闭眼）并记录带标签的 EEG 数据。没有实时流数据，校准就没有神经信号可以与每个阶段关联。",
  "helpFaq.q4": "如何从 Python / Node.js 连接？",
  "helpFaq.a4":
    "通过 mDNS 发现 WebSocket 端口（macOS 上运行 dns-sd -B _skill._tcp），然后打开标准 WebSocket 连接。发送 JSON 命令并接收实时事件流。有关传输格式详情，请参阅 API 选项卡。",
  "helpFaq.q5": "信号质量指示器是什么意思？",
  "helpFaq.a5":
    "每个点代表一个 EEG 电极。绿色 = 良好的皮肤接触，低噪声。黄色 = 有一些运动伪影或佩戴松动。红色 = 高噪声，接触不良。灰色 = 未检测到信号。",
  "helpFaq.q6": "我可以更改陷波滤波器频率吗？",
  "helpFaq.a6":
    "可以——进入设置 → 信号处理，选择 50 Hz（欧洲、大部分亚洲地区）或 60 Hz（美洲、日本）。这会从显示和频段功率计算中去除工频干扰。",
  "helpFaq.q7": "如何重置已配对的设备？",
  "helpFaq.a7": "打开设置 → 已配对设备，然后点击要忘记的设备旁边的 × 按钮。之后可以重新扫描该设备。",
  "helpFaq.q8": "为什么托盘图标变红了？",
  "helpFaq.a8": "您系统上的蓝牙已关闭。打开系统设置 → 蓝牙并启用它。{app} 将在约 1 秒内自动重新连接。",
  "helpFaq.q9": "应用一直在旋转但从不连接——我该怎么办？",
  "helpFaq.a9":
    "1. 确保设备已开机（Muse：长按直到感觉到振动；Ganglion/Cyton：检查蓝色 LED）。2. 保持在 5 米范围内。3. 如果仍然失败，请重启设备电源。",
  "helpFaq.q10": "如何授予蓝牙权限？",
  "helpFaq.a10":
    "macOS 会在 {app} 首次尝试连接时显示权限对话框。如果您之前关闭了它，请进入系统设置 → 隐私与安全性 → 蓝牙并启用 {app}。",
  "helpFaq.q11": "数据库中存储了哪些指标？",
  "helpFaq.a11":
    "每 2.5 秒时段存储：ZUNA 嵌入向量（32 维）、各通道平均的相对频段功率（delta、theta、alpha、beta、gamma、high-gamma）、以 JSON 形式存储的每通道频段功率、派生评分（放松度、参与度）、FAA、交叉频段比（TAR、BAR、DTR）、频谱形状（PSE、APF、BPS、SNR）、相干性、Mu 抑制、情绪指数，以及可用时的 PPG 平均值。",
  "helpFaq.q12": "什么是会话比较？",
  "helpFaq.a12":
    "比较（⌘⇧M）让您选择两个时间范围并排比较：带有差值的相对频段功率条、所有派生评分和比率、前额 Alpha 不对称性、睡眠分期图和 Brain Nebula™——一个 3D UMAP 嵌入投影。",
  "helpFaq.q13": "什么是 Brain Nebula™？",
  "helpFaq.a13":
    "Brain Nebula™（技术上称为：UMAP 嵌入分布）将高维 EEG 嵌入投影到 3D 空间中，使相似的脑状态显示为邻近的点。当会话不同时，范围 A（蓝色）和范围 B（琥珀色）形成不同的聚类。您可以旋转、缩放，并点击标注点以追踪时间连接。可以同时以不同颜色高亮多个标签。",
  "helpFaq.q14": "为什么 Brain Nebula™ 一开始显示随机云团？",
  "helpFaq.a14":
    "UMAP 投影计算量大，在后台任务队列中运行以保持界面响应。计算期间显示随机占位符云团。投影准备就绪后，点会平滑动画到最终位置。",
  "helpFaq.q15": "什么是标签，如何使用？",
  "helpFaq.a15": `标签是用户定义的标记（如"冥想"、"阅读"），附加到录制过程中的某个时刻。它们与 EEG 嵌入一起存储。在 UMAP 查看器中，标注点显示为更大的点，带有彩色圆环——点击其中一个可以追踪该标签在两个会话中的时间轨迹。`,
  "helpFaq.q16": "什么是前额 Alpha 不对称性（FAA）？",
  "helpFaq.a16": "FAA 是 ln(AF8 α) − ln(AF7 α)。正值表示趋近动机（参与、好奇）。负值表示回避（逃避、焦虑）。",
  "helpFaq.q17": "睡眠分期如何工作？",
  "helpFaq.a17":
    "每个 EEG 时段根据相对 delta、theta、alpha 和 beta 功率被分类为清醒、N1、N2、N3 或 REM。比较视图显示每个会话的睡眠图，包含阶段分解和时间百分比。",
  "helpFaq.q18": "键盘快捷键有哪些？",
  "helpFaq.a18": "⌘⇧O——打开 {app} 窗口。⌘⇧M——打开会话比较。在设置 → 快捷键中自定义快捷键。",
  "helpFaq.q19": "什么是 WebSocket API？",
  "helpFaq.a19": `{app} 在本地网络上公开 JSON WebSocket API（mDNS：_skill._tcp）。命令：status、label、search、compare（指标 + 睡眠 + UMAP 票据）、sessions、sleep、umap（入队 3D 投影）、umap_poll（获取结果）。运行 'node test.js' 进行冒烟测试。`,
  "helpFaq.q20": "什么是放松度和参与度评分？",
  "helpFaq.a20":
    "放松度 = α/(β+θ)，衡量平静的清醒状态。参与度 = β/(α+θ)，衡量持续的心理投入。两者通过 sigmoid 映射到 0–100。",
  "helpFaq.q21": "什么是 TAR、BAR 和 DTR？",
  "helpFaq.a21":
    "TAR（Theta/Alpha）——越高 = 越困倦或更冥想。BAR（Beta/Alpha）——越高 = 越紧张或专注。DTR（Delta/Theta）——越高 = 更深睡眠或放松。所有值为各通道平均。",
  "helpFaq.q22": "什么是 PSE、APF、BPS 和 SNR？",
  "helpFaq.a22":
    "PSE（功率谱熵，0–1）——频谱复杂度。APF（Alpha 峰值频率，Hz）——最大 Alpha 功率频率。BPS（频段功率斜率）——1/f 非周期性指数。SNR（信噪比，dB）——宽带功率与线路噪声的比较。",
  "helpFaq.q23": "什么是 Theta/Beta 比率（TBR）？",
  "helpFaq.a23":
    "TBR 是绝对 theta 与绝对 beta 功率的比率。较高的值表示皮层唤醒降低——升高的 TBR 与嗜睡和注意力失调有关。参考：Angelidis 等（2016）。",
  "helpFaq.q24": "什么是 Hjorth 参数？",
  "helpFaq.a24":
    "来自 Hjorth（1970）的三个时域特征：Activity（信号方差/总功率）、Mobility（平均频率估计）和 Complexity（带宽/与纯正弦波的偏差）。它们计算开销低，广泛用于 EEG 机器学习管线。",
  "helpFaq.q25": "计算了哪些非线性复杂度指标？",
  "helpFaq.a25":
    "四种指标：排列熵（序列模式复杂度，Bandt & Pompe 2002）、Higuchi 分形维数（信号分形结构，Higuchi 1988）、DFA 指数（长程时间相关性，Peng 等 1994）和样本熵（信号规律性，Richman & Moorman 2000）。所有指标为 4 个 EEG 通道的平均值。",
  "helpFaq.q26": "什么是 SEF95、频谱重心、PAC 和偏侧化指数？",
  "helpFaq.a26":
    "SEF95（频谱边缘频率）是总功率 95% 以下的频率——用于麻醉监测。频谱重心是功率加权平均频率（唤醒指标）。PAC（相位-幅度耦合）衡量与记忆编码相关的 theta-gamma 交叉频率交互。偏侧化指数是所有频段的广义左/右功率不对称性。",
  "helpFaq.q27": "计算了哪些 PPG 指标？",
  "helpFaq.a27":
    "在 Muse 2/S（带 PPG 传感器）上：心率（bpm，来自红外峰值检测）、RMSSD/SDNN/pNN50（心率变异性——副交感神经张力）、LF/HF 比率（交感迷走平衡）、呼吸频率（次/分钟，来自 PPG 包络）、SpO₂ 估计（未校准的血氧，来自红色/红外比）、灌注指数（外周血流）和 Baevsky 压力指数（自主神经压力）。当连接配备 PPG 的头带时，这些出现在 PPG 生命体征部分。",
  "helpFaq.q28": "如何使用专注计时器？",
  "helpFaq.a28":
    '通过托盘菜单、命令面板（⌘K → "专注计时器"）或全局快捷键（默认 ⌘⇧P）打开专注计时器。选择预设——番茄钟（25/5）、深度工作（50/10）或短暂专注（15/5）——或设置自定义时长。启用"自动标注 EEG"可让 NeuroSkill™ 在每个专注阶段开始和结束时自动标记 EEG 录制。会话点追踪您完成的轮次。您的预设和自定义设置会自动保存，下次打开计时器时恢复。',
  "helpFaq.q29": "如何管理或编辑我的标注？",
  "helpFaq.a29": `通过命令面板打开标签窗口（⌘K → "所有标签"）。它显示所有标注，支持内联文本编辑（点击标签，按 ⌘↵ 保存或 Esc 取消）、删除（带确认）以及显示 EEG 时间范围的元数据。使用搜索框按文本过滤。大型存档中标签每页分页 50 条。`,
  "helpFaq.q30": "如何并排比较两个特定会话？",
  "helpFaq.a30": `在历史记录页面，点击"快速比较"进入比较模式。每个会话行上会出现复选框——选择恰好两个，然后点击"比较所选"以打开预加载了两个会话的比较窗口。或者从托盘或命令面板打开比较窗口，手动使用会话下拉菜单。`,
  "helpFaq.q31": "文本嵌入搜索如何工作？",
  "helpFaq.a31": `您的查询由与索引标签相同的 sentence-transformer 模型转换为向量。然后使用近似最近邻查找在 HNSW 标签索引中搜索该向量。结果是按语义相似度排序的您自己的标注——因此搜索"平静和专注"会找到"深度阅读"或"冥想"等标签，即使这些确切的词从未出现在您的查询中。需要已下载嵌入模型并构建标签索引（设置 → 嵌入）。`,
  "helpFaq.q32": "交互式跨模态搜索如何工作？",
  "helpFaq.a32": `交互式搜索在单次查询中桥接文本、EEG 和时间。步骤 1：您的文本查询被嵌入。步骤 2：找到语义最相似的前 text-k 个标签。步骤 3：对于每个标签，{app} 计算其录制窗口内的平均 EEG 嵌入，并从所有每日索引中检索最相似的前 eeg-k 个 EEG 时段——从语言空间跨越到脑状态空间。步骤 4：对于找到的每个 EEG 时刻，收集 ±reach 分钟范围内的标注作为"发现的标签"。四个节点层（查询 → 文本匹配 → EEG 邻居 → 发现的标签）渲染为 4 层有向图。导出为 SVG 获取静态图像，或导出为 DOT 源代码在 Graphviz 中进一步处理。`,

  "helpOld.hooksTitle": "主动钩子",
  "helpOld.hooksDesc":
    "钩子在后台监听：模糊关键词匹配 → 文本邻居扩展 → EEG 距离检查。如果匹配，应用广播钩子事件并显示通知。",
  "helpOld.hooksFlow": "简明流程",
  "helpOld.hooksFaqQ": "钩子如何触发？",
  "helpOld.hooksFaqA":
    "工作线程将每个新的 EEG 嵌入与通过关键词 + 文本相似度选择的最近标签样本进行比较。如果最佳余弦距离低于您的阈值，钩子就会触发。",
  "helpOld.trayIconStates": "托盘图标状态",
  "helpOld.trayIconDesc": "菜单栏图标会改变颜色和形状，一目了然地反映当前连接状态。",
  "helpOld.greyDisconnected": "灰色——已断开",
  "helpOld.greyDesc": "蓝牙已开启；未连接 BCI 设备。",
  "helpOld.spinningScanning": "旋转——扫描中",
  "helpOld.spinningDesc": "正在搜索 BCI 设备或尝试连接。",
  "helpOld.greenConnected": "绿色——已连接",
  "helpOld.greenDesc": "正在从 BCI 设备流式传输实时 EEG 数据。",
  "helpOld.redBtOff": "红色——蓝牙关闭",
  "helpOld.redDesc": "蓝牙无线电已关闭。无法扫描或连接。",
  "helpOld.btLifecycle": "蓝牙生命周期与自动重连",
  "helpOld.btLifecycleDesc":
    "{app} 使用 CoreBluetooth（macOS）或 BlueZ（Linux）实时监控蓝牙状态。无轮询延迟——状态变化在一秒内反映。",
  "helpOld.btStep1": "蓝牙关闭",
  "helpOld.btStep1Desc": "托盘图标立即变红。蓝牙关闭卡片替换主视图。不进行扫描或连接尝试。",
  "helpOld.btStep2": "蓝牙重新开启",
  "helpOld.btStep2Desc": "约 1 秒内，{app} 自动恢复扫描。如果之前配对了首选设备，将开始连接尝试。",
  "helpOld.btStep3": "BCI 设备已开机",
  "helpOld.btStep3Desc": "后台扫描器在 3–6 秒内发现它并自动连接；图标变为绿色。",
  "helpOld.btStep4": "设备未立即找到",
  "helpOld.btStep4Desc": "{app} 每 3 秒静默重试。旋转图标持续显示直到找到设备或您手动取消。",
  "helpOld.btStep5": "您点击重试",
  "helpOld.btStep5Desc": "与自动重连相同的重试循环。每 3 秒重试直到找到设备。",
  "helpOld.examples": "示例",
  "helpOld.example1Title": "示例 1——正常启动",
  "helpOld.example2Title": "示例 2——蓝牙关闭后重新开启",
  "helpOld.example3Title": "示例 3——蓝牙恢复后 BCI 设备开机",
  "helpOld.ex1Step1": "{app} 打开 → 扫描 BCI 设备",
  "helpOld.ex1Step2": "5 秒内找到设备",
  "helpOld.ex1Step3": "已连接——EEG 正在流式传输",
  "helpOld.ex2Step1": "已连接 → 用户禁用蓝牙",
  "helpOld.ex2Step2": `图标变红；显示"蓝牙已关闭"卡片`,
  "helpOld.ex2Step3": "…用户重新启用蓝牙…",
  "helpOld.ex2Step4": "自动扫描恢复（约 1 秒）",
  "helpOld.ex2Step5": "已重新连接——流式传输恢复",
  "helpOld.ex3Step1": "蓝牙开启，设备仍关闭 → 每 3 秒重试",
  "helpOld.ex3Step2": "…用户开启 BCI 设备…",
  "helpOld.ex3Step3": "在下一个扫描周期发现设备",
  "helpOld.ex3Step4": "自动连接——无需按按钮",
  "helpOld.broadcastEvents": "广播事件（服务器 → 客户端）",
  "helpOld.commands": "命令（客户端 → 服务器）",
  "helpOld.wsTitle": "本地网络流式传输（WebSocket）",
  "helpOld.wsDesc":
    "{app} 通过本地 WebSocket 服务器流式传输派生的 EEG 指标（约 4 Hz 频段功率、评分、心率）和设备状态（约 1 Hz）。不广播原始 EEG/PPG/IMU 样本。该服务通过 Bonjour / mDNS 广播，以便客户端无需知道 IP 地址即可发现。",
  "helpOld.discoverService": "发现服务",
  "helpOld.wireFormat": "传输格式（JSON）",
  "helpOld.faq": "常见问题",
  "helpOld.faqQ1": "为什么托盘图标变红了？",
  "helpOld.faqA1": "您 Mac 上的蓝牙已关闭。打开系统设置 → 蓝牙并启用它。{app} 将在约 1 秒内自动重新连接。",
  "helpOld.faqQ2": "应用一直在旋转但从不连接——我该怎么办？",
  "helpOld.faqA2":
    "1. 确保 BCI 设备已开机（Muse：长按直到感觉到振动；Ganglion/Cyton：检查蓝色 LED）。2. 保持在 5 米范围内。3. 如果仍然失败，请重启设备电源。",
  "helpOld.faqQ3": "如何授予蓝牙权限？",
  "helpOld.faqA3":
    "macOS 会在 {app} 首次尝试连接时显示权限对话框。如果您之前关闭了它，请进入系统设置 → 隐私与安全性 → 蓝牙并启用 {app}。",
  "helpOld.faqQ4": "我可以在同一网络上的其他应用中接收 EEG 数据吗？",
  "helpOld.faqA4":
    "可以。将 WebSocket 客户端连接到 Bonjour 发现输出中显示的地址（参见上方本地网络流式传输部分）。您将收到派生指标（约 4 Hz eeg-bands 事件，包含 60 多项评分）和设备状态（约 1 Hz）。注意：原始 EEG/PPG/IMU 样本流不通过 WebSocket API 提供——仅提供处理后的评分和频段功率。",
  "helpOld.faqQ5": "我的 EEG 录制保存在哪里？",
  "helpOld.faqA5":
    "原始（未过滤）样本写入应用数据文件夹中的 CSV 文件（macOS/Linux 上为 {dataDir}/）。每个会话创建一个文件。",
  "helpOld.faqQ6": "信号质量点是什么意思？",
  "helpOld.faqA6":
    "每个点代表一个 EEG 通道（TP9、AF7、AF8、TP10）。绿色 = 良好（低噪声，良好皮肤接触）。黄色 = 一般（有一些运动伪影或电极松动）。红色 = 差（高噪声，接触非常松或电极脱离皮肤）。灰色 = 无信号。",
  "helpOld.faqQ7": "工频陷波滤波器有什么用？",
  "helpOld.faqA7":
    "市电在 EEG 录制中引入 50 或 60 Hz 噪声。陷波滤波器从波形显示中去除该频率（及其谐波）。选择 60 Hz（美国/日本）或 50 Hz（欧盟/英国）以匹配当地电网。",
  "helpOld.faqQ8": "数据库中存储了哪些指标？",
  "helpOld.faqA8":
    "每 2.5 秒时段存储：ZUNA 嵌入向量（32 维）、各通道平均的相对频段功率（delta、theta、alpha、beta、gamma、high-gamma）、以 JSON 形式存储的每通道频段功率、派生评分（放松度、参与度）、前额 Alpha 不对称性（FAA）、交叉频段比（TAR、BAR、DTR、TBR）、频谱形状（PSE、APF、SEF95、频谱重心、BPS、SNR）、相干性、Mu 抑制、情绪综合指数、Hjorth 参数（activity、mobility、complexity）、非线性复杂度（排列熵、Higuchi FD、DFA、样本熵）、PAC（θ–γ）、偏侧化指数、PPG 平均值，以及连接 Muse 2/S 时的 PPG 派生指标（HR、RMSSD、SDNN、pNN50、LF/HF、呼吸频率、SpO₂、灌注指数、压力指数）。",
  "helpOld.faqQ9": "什么是会话比较功能？",
  "helpOld.faqA9":
    "会话比较（⌘⇧M）让您选择任意两个录制会话并排比较。显示内容包括：带差值的相对频段功率条、所有派生评分和比率、前额 Alpha 不对称性、睡眠分期图和 3D UMAP 嵌入投影，可视化两个会话在高维特征空间中的相似程度。",
  "helpOld.faqQ10": "什么是 3D UMAP 查看器？",
  "helpOld.faqA10":
    "UMAP 查看器将高维 EEG 嵌入投影到 3D 空间中，使相似的脑状态显示为邻近的点。如果会话不同，会话 A（蓝色）和会话 B（琥珀色）形成不同的聚类。您可以旋转、缩放，并点击标注点查看其时间连接。",
  "helpOld.faqQ11": "为什么 UMAP 查看器一开始显示随机云团？",
  "helpOld.faqA11":
    "UMAP 计算量大——它在后台任务队列中运行以保持界面响应。计算期间显示随机高斯占位符云团。真正的投影准备就绪后，点会平滑动画到最终位置。",
  "helpOld.faqQ12": "什么是标签，如何使用？",
  "helpOld.faqA12": `标签是用户定义的标记（如"冥想"、"阅读"、"焦虑"），附加到录制过程中的某个时刻。它们与 EEG 嵌入一起存储在数据库中。在 UMAP 查看器中，标注点显示为更大的点，带有彩色圆环。`,
  "helpOld.faqQ13": "什么是前额 Alpha 不对称性（FAA）？",
  "helpOld.faqA13":
    "FAA 是 ln(AF8 α) − ln(AF7 α)。正值表示左半球 Alpha 抑制更大，与趋近动机（参与、好奇）相关。负值表示回避（逃避、焦虑）。",
  "helpOld.faqQ14": "睡眠分期如何工作？",
  "helpOld.faqA14":
    "{app} 根据相对 delta、theta、alpha 和 beta 功率比将每个 EEG 时段分类为清醒、N1（浅睡）、N2、N3（深睡）或 REM 睡眠。比较视图显示每个会话的睡眠图，包含颜色编码的阶段分解和时间百分比。",
  "helpOld.faqQ15": "键盘快捷键有哪些？",
  "helpOld.faqA15": "⌘⇧O——打开 {app} 窗口。⌘⇧M——打开会话比较。您可以在设置 → 快捷键中自定义快捷键。",
  "helpOld.faqQ16": "什么是 WebSocket API？",
  "helpOld.faqA16":
    "{app} 在本地网络上公开基于 JSON 的 WebSocket API（mDNS：_skill._tcp）。命令包括：status、label、search、compare、sessions、sleep、umap 和 umap_poll。从项目目录运行 'node test.js' 以冒烟测试所有命令。",
  "helpOld.faqQ17": "什么是派生评分（放松度、参与度）？",
  "helpOld.faqA17":
    "放松度 = α / (β + θ)，衡量平静的清醒状态。参与度 = β / (α + θ)，衡量持续的心理投入。两者映射到 0–100 的范围。",
  "helpOld.faqQ18": "什么是交叉频段比？",
  "helpOld.faqA18":
    "TAR（Theta/Alpha）——较高值表示困倦或冥想状态。BAR（Beta/Alpha）——较高值表示紧张或专注。DTR（Delta/Theta）——较高值表示深睡或深度放松。所有值为各通道平均。",
  "helpOld.faqQ19": "什么是 PSE、APF、BPS 和 SNR？",
  "helpOld.faqA19":
    "PSE（功率谱熵，0–1）衡量频谱复杂度。APF（Alpha 峰值频率，Hz）是最大 Alpha 功率的频率。BPS（频段功率斜率）是 1/f 非周期性指数。SNR（信噪比，dB）比较宽带功率与 50–60 Hz 线路噪声。",

  "helpTabs.tts": "语音",

  "helpApi.cmdSay": "say",
  "helpApi.cmdSayParams": "text: string（必填）",
  "helpApi.cmdSayDesc": "通过设备端 TTS 朗读文本。即发即忘——立即返回，音频在后台播放。首次调用时初始化 TTS 引擎。",

  "helpFaq.q33": "如何从脚本或自动化工具触发 TTS 语音？",
  "helpFaq.a33":
    '使用 WebSocket 或 HTTP API。WebSocket：发送 {"command":"say","text":"your message"}。HTTP（curl）：curl -X POST http://localhost:<port>/say -H \'Content-Type: application/json\' -d \'{"text":"your message"}\'。say 命令是即发即忘——立即响应，音频在后台播放。',
  "helpFaq.q34": "为什么 TTS 没有声音？",
  "helpFaq.a34":
    "检查 espeak-ng 是否已安装在 PATH 中（macOS 上 brew install espeak-ng，Ubuntu 上 apt install espeak-ng）。检查系统音频输出是否静音或路由到其他设备。首次运行时，模型（约 30 MB）必须完成下载才能发出声音。在设置 → 语音中启用 TTS 调试日志以查看日志文件中的合成事件。",
  "helpFaq.q35": "我可以更改 TTS 声音或语言吗？",
  "helpFaq.a35":
    "当前版本使用 KittenML/kitten-tts-mini-0.8 模型中的 Jasper 英语（en-us）声音。仅英语文本能正确进行音素化。未来版本计划支持更多声音和语言。",
  "helpFaq.q36": "TTS 需要互联网连接吗？",
  "helpFaq.a36":
    "仅需一次，用于从 HuggingFace Hub 初始下载约 30 MB 模型。之后，所有合成完全离线运行。模型缓存在 ~/.cache/huggingface/hub/ 中，每次后续启动时重复使用。",
  "helpFaq.q37": "NeuroSkill™ 支持哪些 OpenBCI 开发板？",
  "helpFaq.a37":
    "NeuroSkill™ 通过已发布的 openbci crate（crates.io/crates/openbci）支持 OpenBCI 生态系统中的所有开发板：Ganglion（4 通道，BLE）、Ganglion + WiFi Shield（4 通道，1 kHz）、Cyton（8 通道，USB 加密狗）、Cyton + WiFi Shield（8 通道，1 kHz）、Cyton+Daisy（16 通道，USB 加密狗）、Cyton+Daisy + WiFi Shield（16 通道，1 kHz）和 Galea（24 通道，UDP）。任何开发板均可与其他 BCI 设备同时使用。在设置 → OpenBCI 中选择开发板，然后点击连接。",
  "helpFaq.q38": "如何通过蓝牙连接 Ganglion？",
  "helpFaq.a38":
    '1. 开启 Ganglion 电源——蓝色 LED 应缓慢闪烁。2. 在设置 → OpenBCI 中选择 "Ganglion — 4ch · BLE"。3. 保存设置，然后点击连接。NeuroSkill™ 会在配置的超时时间内扫描（默认 10 秒）。将开发板保持在 3–5 米范围内。在 macOS 上，提示时授予蓝牙权限（或前往系统设置 → 隐私与安全性 → 蓝牙）。',
  "helpFaq.q39": "我的 Ganglion 已开机但 NeuroSkill™ 找不到它——我该怎么办？",
  "helpFaq.a39":
    "1. 确认蓝色 LED 正在闪烁（常亮或熄灭表示它未在广播——按按钮唤醒）。2. 在设置 → OpenBCI 中增加 BLE 扫描超时时间。3. 将开发板移至 2 米范围内。4. 退出 NeuroSkill™ 并重新打开以重置 BLE 适配器。5. 在系统设置中关闭蓝牙再重新打开。6. 确保没有其他应用（OpenBCI GUI、另一个 NeuroSkill™ 实例）已经连接——BLE 每次只允许一个主机。7. 在 macOS 14+ 上，检查 NeuroSkill™ 在系统设置 → 隐私与安全性 → 蓝牙中是否有蓝牙权限。",
  "helpFaq.q40": "如何通过 USB 连接 Cyton？",
  "helpFaq.a40":
    '1. 将 USB 无线电加密狗插入计算机（加密狗是无线电——Cyton 开发板本身没有 USB 端口）。2. 开启 Cyton 电源——将电源开关滑到 PC。3. 在设置 → OpenBCI 中选择 "Cyton — 8ch · USB serial"。4. 点击刷新列出串口，然后选择端口（macOS 上为 /dev/cu.usbserial-…，Linux 上为 /dev/ttyUSB0，Windows 上为 COM3）或留空自动检测。5. 保存设置并点击连接。',
  "helpFaq.q41": "串口未列出或出现权限被拒绝错误——如何解决？",
  "helpFaq.a41":
    "macOS：加密狗显示为 /dev/cu.usbserial-*。如果不存在，请从芯片制造商网站安装 CP210x 或 FTDI VCP 驱动程序。Linux：运行 sudo usermod -aG dialout $USER，然后注销再登录。插入后验证设备出现在 /dev/ttyUSB0 或 /dev/ttyACM0。Windows：安装 CP2104 USB 转 UART 驱动程序；COM 端口会出现在设备管理器 → 端口（COM 和 LPT）中。",
  "helpFaq.q42": "如何通过 OpenBCI WiFi Shield 连接？",
  "helpFaq.a42":
    "1. 将 WiFi Shield 堆叠在 Cyton 或 Ganglion 顶部并为开发板通电。2. 在计算机上连接到 Shield 广播的 WiFi 网络（SSID：OpenBCI-XXXX，通常无密码）。3. 在设置 → OpenBCI 中选择匹配的 WiFi 开发板变体。4. 输入 IP 192.168.4.1（Shield 默认值）或留空自动发现。5. 点击连接。WiFi Shield 以 1000 Hz 流式传输——在信号处理中将低通滤波器设为 ≤ 500 Hz 以避免混叠。",
  "helpFaq.q43": "什么是 Galea 开发板，如何设置？",
  "helpFaq.a43":
    'OpenBCI 的 Galea 是一款 24 通道研究级生物信号头戴设备，结合 EEG、EMG 和 AUX 传感器，通过 UDP 流式传输。连接方法：1. 开启 Galea 电源并将其连接到本地网络。2. 在设置 → OpenBCI 中选择 "Galea — 24ch · UDP"。3. 输入 Galea IP 地址（或留空接受来自任何发送者的数据）。4. 点击连接。通道 1–8 为 EEG（驱动实时分析）；9–16 为 EMG；17–24 为 AUX。所有 24 个通道保存至 CSV。',
  "helpFaq.q44": "我可以同时使用两个 BCI 设备吗？",
  "helpFaq.a44":
    "可以——NeuroSkill™ 可以同时从两者流式传输。先连接的设备驱动实时仪表盘、频段功率显示和 ZUNA 嵌入管线。第二个设备的数据记录到 CSV 以供离线分析。实时管线中的同时多设备分析计划在未来版本中推出。",
  "helpFaq.q45": "为什么 Cyton 的 8 个通道中只有 4 个用于实时分析？",
  "helpFaq.a45":
    "实时分析管线（滤波器、频段功率、ZUNA 嵌入、信号质量点）目前设计为 4 通道输入，以匹配 Muse 头戴设备格式。对于 Cyton（8 通道）和 Cyton+Daisy（16 通道），通道 1–4 供实时管线使用；所有通道写入 CSV 供离线使用。完整的多通道管线支持在路线图中。",
  "helpFaq.q46": "如何改善 OpenBCI 开发板的信号质量？",
  "helpFaq.a46":
    "1. 在每个电极位点涂抹导电凝胶或膏体，拨开头发使其直接接触头皮。2. 录制前使用 OpenBCI GUI 阻抗检查验证阻抗——目标 < 20 kΩ。3. 将 SRB 偏置电极连接到乳突（耳后）以获得稳固的参考。4. 保持电极线缆短且远离电源。5. 在设置 → 信号处理中使用陷波滤波器（欧洲 50 Hz，美洲 60 Hz）。6. 对于 Ganglion BLE：将开发板远离 USB 3.0 端口，因其发射 2.4 GHz 干扰。",
  "helpFaq.q47": "我的 OpenBCI 连接反复中断——如何稳定它？",
  "helpFaq.a47":
    "Ganglion BLE：将开发板保持在 2 米范围内；将主机的 BLE 适配器插入 USB 2.0 端口（USB 3.0 发射 2.4 GHz 噪声可能干扰 BLE）。Cyton USB：使用短的高质量 USB 线缆，直接连接到计算机而非通过集线器。WiFi Shield：确保 Shield 的 2.4 GHz 频道不与路由器重叠；将开发板移近。所有开发板：录制期间避免运行其他无线密集型应用（视频通话、文件同步）。",
  "helpFaq.q48": "活动追踪到底记录了什么？",
  "helpFaq.a48":
    '活动窗口追踪在每次前台应用或窗口标题变化时向 activity.sqlite 写入一行。每行包含：应用显示名称（如 "Safari"、"VS Code"）、二进制文件或应用包的完整路径、窗口标题（如文档名称或网页标题——沙盒应用可能为空），以及其变为活动状态的 Unix 秒时间戳。键盘和鼠标追踪每 60 秒写入一次周期性样本，但仅在自上次刷新以来有活动时才写入。每个样本存储两个 Unix 秒时间戳——上次键盘事件和上次鼠标/触控板事件。它不记录您按了哪些键、输入了什么文本、光标在哪里或点击了哪些按钮。两项功能默认启用，可在设置 → 活动追踪中独立关闭。',
  "helpFaq.q49": "为什么 macOS 要求辅助功能权限来进行输入追踪？",
  "helpFaq.a49": `键盘和鼠标追踪使用 CGEventTap——一种 macOS API，在输入事件到达各个应用之前拦截系统级输入事件。Apple 要求任何读取全局输入的应用都需要辅助功能权限，无论该应用如何处理这些数据。没有辅助功能权限，tap 会静默失败：NeuroSkill 继续正常工作，但上次键盘和上次鼠标时间戳保持为零。授予方法：系统设置 → 隐私与安全性 → 辅助功能 → 找到 NeuroSkill → 开启。如果您不想授予权限，请在设置中禁用"追踪键盘和鼠标活动"开关——这可以阻止钩子被安装。活动窗口追踪（应用名称和路径）使用 AppleScript/osascript，不需要辅助功能权限。`,
  "helpFaq.q50": "如何清除或删除活动追踪数据？",
  "helpFaq.a50":
    "所有活动追踪数据存储在单个文件中：~/.skill/activity.sqlite。要删除所有内容：退出 NeuroSkill，删除该文件，然后重新启动——下次启动时会自动创建空数据库。要停止未来的收集而不触及现有数据，请关闭设置 → 活动追踪中的两个开关；更改立即生效，无需重启。要选择性地删除行，可以在任何 SQLite 浏览器（如 DB Browser for SQLite）中打开文件并从 active_windows 或 input_activity 中 DELETE。",
  "helpFaq.q51": "为什么 {app} 要求 macOS 上的辅助功能权限？",
  "helpFaq.a51":
    "{app} 使用 macOS CGEventTap API 记录上次按键或鼠标移动的时间。这用于计算活动追踪面板中显示的键盘和鼠标活动时间戳。仅存储时间戳——不记录按键、不记录光标位置。如果未授予权限，该功能会静默降级。",
  "helpFaq.q52": "{app} 需要蓝牙权限吗？",
  "helpFaq.a52":
    "需要。{app} 使用蓝牙低功耗（BLE）连接您的 BCI 头戴设备。在 macOS 上，应用首次尝试扫描时系统会显示一次性蓝牙权限提示。在 Linux 和 Windows 上不需要显式蓝牙权限。",
  "helpFaq.q53": "如何在 macOS 上授予辅助功能权限？",
  "helpFaq.a53": `打开系统设置 → 隐私与安全性 → 辅助功能。在列表中找到 {app} 并开启。您也可以在应用内的权限选项卡中点击"打开辅助功能设置"`,
  "helpFaq.q54": "如果我拒绝辅助功能权限会怎样？",
  "helpFaq.a54":
    "键盘和鼠标活动时间戳将不会被记录，并保持为零。所有其他功能——EEG 流式传输、频段功率、校准、TTS、搜索——继续正常工作。您可以在设置 → 活动追踪中完全禁用该功能。",
  "helpFaq.q55": "我可以在授予权限后撤销吗？",
  "helpFaq.a55":
    "可以。打开系统设置 → 隐私与安全性 → 辅助功能（或通知）并关闭 {app}。相关功能将立即停止工作，无需重启。",

  "helpTabs.llm": "LLM",

  "helpLlm.overviewSection": "概述",
  "helpLlm.overviewSectionDesc":
    "NeuroSkill 附带一个可选的本地 LLM 服务器，让您拥有一个私人的、OpenAI 兼容的 AI 助手，无需向云端发送任何数据。",
  "helpLlm.whatIsTitle": "什么是 LLM 功能？",
  "helpLlm.whatIsBody":
    "LLM 功能在应用内嵌入了一个由 llama.cpp 驱动的推理服务器。启用后，它在与 WebSocket API 相同的本地端口上提供 OpenAI 兼容端点（/v1/chat/completions、/v1/completions、/v1/embeddings、/v1/models、/health）。您可以将任何 OpenAI 兼容客户端——Chatbot UI、Continue、Open Interpreter 或您自己的脚本——指向它。",
  "helpLlm.privacyTitle": "隐私与离线使用",
  "helpLlm.privacyBody":
    "所有推理在您的设备上运行。没有令牌、提示或补全离开 localhost。唯一的网络活动是从 HuggingFace Hub 的初始模型下载。模型本地缓存后，您可以完全断开互联网。",
  "helpLlm.compatTitle": "OpenAI 兼容 API",
  "helpLlm.compatBody":
    "服务器使用与 OpenAI API 相同的协议。任何接受 base_url 参数的库（openai-python、openai-node、LangChain、LlamaIndex 等）都可以直接使用。将 base_url 设为 http://localhost:<port>/v1，API 密钥留空，除非您在推理设置中配置了密钥。",
  "helpLlm.modelsSection": "模型管理",
  "helpLlm.modelsSectionDesc": "浏览、下载和激活内置目录中 GGUF 量化的语言模型。",
  "helpLlm.catalogTitle": "模型目录",
  "helpLlm.catalogBody":
    "目录列出精选的模型系列（如 Qwen、Llama、Gemma、Phi），每个系列有多种量化变体。使用系列下拉菜单浏览，然后选择特定量化版本下载。标有 ★ 的模型是该系列的推荐默认选项。",
  "helpLlm.quantsTitle": "量化级别",
  "helpLlm.quantsBody":
    "每个模型有多种 GGUF 量化级别（Q4_K_M、Q5_K_M、Q6_K、Q8_0 等）。较低的量化更小更快，但会牺牲一些质量。Q4_K_M 通常是最佳权衡。Q8_0 接近无损但需要大约两倍内存。BF16/F16/F32 是未量化的参考权重。",
  "helpLlm.hardwareFitTitle": "硬件适配徽章",
  "helpLlm.hardwareFitBody":
    "每个量化行显示一个颜色编码的徽章，估计它与您硬件的适配程度：🟢 运行极佳——完全适配 GPU VRAM 且有余量。🟡 运行良好——适配 VRAM 但余量较紧。🟠 紧凑适配——可能需要部分 CPU 卸载或减小上下文大小。🔴 无法适配——对于可用内存来说太大。估算考虑了 GPU VRAM、系统 RAM、模型大小和上下文开销。",
  "helpLlm.visionTitle": "视觉/多模态模型",
  "helpLlm.visionBody":
    "标记为视觉或多模态的系列包含一个可选的多模态投影器（mmproj）文件。下载文本模型及其投影器以在聊天窗口中启用图像输入。投影器扩展文本模型——它不是独立模型。",
  "helpLlm.downloadTitle": "下载与删除",
  "helpLlm.downloadBody": `点击"下载"从 HuggingFace Hub 获取模型。进度条显示实时下载状态。您可以随时取消。下载的模型存储在本地，可以删除以释放磁盘空间。如果手动修改了模型目录，请使用"刷新缓存"按钮重新扫描目录。`,
  "helpLlm.inferenceSection": "推理设置",
  "helpLlm.inferenceSectionDesc": "微调服务器加载和运行模型的方式。",
  "helpLlm.gpuLayersTitle": "GPU 层数",
  "helpLlm.gpuLayersBody": `控制卸载到 GPU 的 Transformer 层数。如果模型适配 VRAM，设为"全部"以获得最大速度。设为 0 进行纯 CPU 推理。中间值将模型分配到 GPU 和 CPU——适用于模型略微超出 VRAM 容量的情况。`,
  "helpLlm.ctxSizeTitle": "上下文大小",
  "helpLlm.ctxSizeBody": `KV 缓存大小（以 token 为单位）。"自动"根据模型大小和量化选择适合您 GPU/RAM 的最大上下文。更大的上下文让模型记住更多对话历史，但消耗更多内存。选项限于模型的训练最大值。如果遇到内存不足错误，请尝试减小上下文大小。`,
  "helpLlm.parallelTitle": "并行请求",
  "helpLlm.parallelBody":
    "最大并发解码循环数。较高的值允许多个客户端共享服务器，但增加峰值内存使用。对于大多数单用户设置，1 就足够了。",
  "helpLlm.apiKeyTitle": "API 密钥",
  "helpLlm.apiKeyBody":
    "每个 /v1/* 请求所需的可选 Bearer 令牌。留空表示在 localhost 上开放访问。如果您在局域网上暴露端口并希望限制访问，请设置密钥。",
  "helpLlm.toolsSection": "内置工具",
  "helpLlm.toolsSectionDesc": "LLM 聊天可以调用本地工具来收集信息或代您执行操作。",
  "helpLlm.toolsOverviewTitle": "工具如何工作",
  "helpLlm.toolsOverviewBody":
    "启用工具使用后，模型可以在对话中请求调用一个或多个工具。应用在本地执行工具并将结果反馈给模型，以便它将真实信息纳入响应中。工具仅在模型明确请求时调用——它们从不在后台运行。",
  "helpLlm.toolsSafeTitle": "安全工具",
  "helpLlm.toolsSafeBody":
    "日期、位置、网页搜索、网页获取和读取文件是只读工具，不会修改您的系统。日期返回当前本地日期和时间。位置提供基于 IP 的近似地理位置。网页搜索运行 DuckDuckGo 即时回答查询。网页获取检索公共 URL 的文本正文。读取文件读取本地文件，支持可选分页。",
  "helpLlm.toolsDangerTitle": "特权工具（⚠️）",
  "helpLlm.toolsDangerBody":
    "Bash、写入文件和编辑文件可以修改您的系统。Bash 以与应用相同的权限执行 shell 命令。写入文件在磁盘上创建或覆盖文件。编辑文件执行查找和替换编辑。这些默认禁用并显示警告徽章。仅在您了解风险时才启用它们。",
  "helpLlm.toolsExecModeTitle": "执行模式与限制",
  "helpLlm.toolsExecModeBody": `并行模式让模型同时调用多个工具（更快）。顺序模式一次运行一个（对有副作用的工具更安全）。"最大轮次"限制每条消息允许的工具调用/工具结果往返次数。"每轮最大调用数"限制同时工具调用的数量。`,
  "helpLlm.chatSection": "聊天与日志",
  "helpLlm.chatSectionDesc": "与模型交互并监控服务器活动。",
  "helpLlm.chatWindowTitle": "聊天窗口",
  "helpLlm.chatWindowBody":
    "从 LLM 服务器卡片或托盘菜单打开聊天窗口。它提供熟悉的聊天界面，支持 Markdown 渲染、代码高亮和工具调用可视化。对话是临时的——不保存到磁盘。支持视觉功能的模型通过拖放或附件按钮接受图像附件。",
  "helpLlm.chatApiTitle": "使用外部客户端",
  "helpLlm.chatApiBody":
    "由于服务器兼容 OpenAI，您可以使用任何外部聊天前端。将其指向 http://localhost:<port>/v1，如果配置了 API 密钥则设置密钥，并从 /v1/models 选择任何模型名称。热门选项包括 Open WebUI、Chatbot UI、Continue（VS Code）以及用于脚本的 curl / httpie。",
  "helpLlm.serverLogsTitle": "服务器日志",
  "helpLlm.serverLogsBody": `LLM 设置面板底部的日志查看器实时流式传输服务器输出。它显示模型加载进度、token 生成速度和任何错误。在高级部分启用"详细"模式以获得详细的 llama.cpp 诊断输出。日志自动滚动，但您可以通过手动向上滚动暂停。`,

  "helpTabs.hooks": "钩子",

  "helpHooks.overviewSection": "概述",
  "helpHooks.overviewSectionDesc": "主动钩子让应用在您最近的 EEG 模式匹配特定关键词或脑状态时自动触发操作。",
  "helpHooks.whatIsTitle": "什么是主动钩子？",
  "helpHooks.whatIsBody":
    "主动钩子是一条实时监控您最近 EEG 标签嵌入的规则。当您最近脑状态嵌入与钩子关键词嵌入之间的余弦距离低于配置的阈值时，钩子触发——发送命令、显示通知、触发 TTS 或广播 WebSocket 事件。钩子让您无需编写代码即可构建闭环神经反馈自动化。",
  "helpHooks.howItWorksTitle": "工作原理",
  "helpHooks.howItWorksBody":
    "应用每隔几秒从您最近的脑数据中计算 EEG 嵌入。这些嵌入通过 HNSW 索引上的余弦相似度与每个活跃钩子中定义的关键词嵌入进行比较。如果任何钩子的距离阈值被满足，钩子就会触发。冷却时间防止同一钩子在短时间内重复触发。匹配完全在本地进行——没有数据离开您的设备。",
  "helpHooks.scenariosTitle": "场景",
  "helpHooks.scenariosBody": `每个钩子可以限定到一个场景——认知、情感、身体或任意。认知钩子针对专注、分心或精神疲劳等心理状态。情感钩子针对压力、平静或沮丧等情感状态。身体钩子针对嗜睡或身体疲劳等身体状态。"任意"无论推断的场景类别如何都会匹配。`,
  "helpHooks.configSection": "配置钩子",
  "helpHooks.configSectionDesc": "每个钩子有几个字段控制其触发时机和方式。",
  "helpHooks.nameTitle": "钩子名称",
  "helpHooks.nameBody": `钩子的描述性名称（如"深度工作守卫"、"平静恢复"）。名称用于历史日志和 WebSocket 事件中。它在所有钩子中必须唯一。`,
  "helpHooks.keywordsTitle": "关键词",
  "helpHooks.keywordsBody": `一个或多个描述您想要检测的脑状态的关键词或短语（如"专注"、"深度工作"、"压力"、"疲劳"）。这些使用与 EEG 标签相同的 sentence-transformer 模型嵌入。当最近的 EEG 嵌入在共享向量空间中接近这些关键词嵌入时，钩子触发。`,
  "helpHooks.keywordSugTitle": "关键词建议",
  "helpHooks.keywordSugBody": `输入关键词时，应用使用模糊字符串匹配和语义嵌入相似度从您现有的标签历史中建议相关术语。建议显示来源徽章——"fuzzy"表示基于字符串的匹配，"semantic"表示基于嵌入的匹配，或"fuzzy+semantic"表示两者兼有。使用 ↑/↓ 方向键和 Enter 快速接受建议。`,
  "helpHooks.distanceTitle": "距离阈值",
  "helpHooks.distanceBody":
    "最近 EEG 嵌入与钩子关键词嵌入之间的最大余弦距离（0–1），达到此值钩子触发。较低的值要求更接近的匹配（更严格），较高的值更频繁触发（更宽松）。典型值范围从 0.08（非常严格）到 0.25（宽松）。从 0.12–0.16 开始，根据建议工具进行调整。",
  "helpHooks.distanceSugTitle": "距离建议工具",
  "helpHooks.distanceSugBody": `点击"建议阈值"以分析您录制的 EEG 数据与钩子关键词的匹配情况。该工具计算距离分布（min、p25、p50、p75、max）并推荐一个平衡灵敏度和特异性的阈值。可视百分位条显示您当前和建议的阈值在分布中的位置。点击"应用"使用建议值。`,
  "helpHooks.recentLimitTitle": "最近参考数",
  "helpHooks.recentLimitBody":
    "与钩子关键词进行比较的最近 EEG 嵌入样本数量（默认：12）。较高的值平滑瞬态尖峰但增加检测延迟。较低的值反应更快但可能因短暂伪影而触发。有效范围：10–20。",
  "helpHooks.commandTitle": "命令",
  "helpHooks.commandBody":
    "钩子触发时在 WebSocket 事件中广播的可选命令字符串（如 'focus_reset'、'calm_breath'）。在 WebSocket 上监听的外部自动化工具可以对此命令做出反应，触发特定应用的操作、通知或脚本。",
  "helpHooks.textTitle": "负载文本",
  "helpHooks.textBody": `钩子触发事件中包含的可选人类可读消息（如"休息 2 分钟。"）。此文本显示在通知中，如果启用了语音引导，还可以通过 TTS 朗读。`,
  "helpHooks.advancedSection": "高级",
  "helpHooks.advancedSectionDesc": "提示、历史和与外部工具的集成。",
  "helpHooks.examplesTitle": "快速示例",
  "helpHooks.examplesBody": `"快速示例"面板提供常见用例的现成钩子模板：深度工作守卫（认知专注重置）、平静恢复（情感压力缓解）和身体休息（身体疲劳）。点击任何示例以添加为带预填关键词、场景、阈值和负载的新钩子。调整值以匹配您的个人 EEG 模式。`,
  "helpHooks.historyTitle": "钩子触发历史",
  "helpHooks.historyBody":
    "钩子面板底部的可折叠历史日志记录每次钩子触发事件，包含时间戳、匹配标签、余弦距离、命令和触发时的关键词。使用它来审计钩子行为、验证阈值和调试误报。展开任何行可查看完整详情。分页控件让您浏览旧事件。",
  "helpHooks.wsEventsTitle": "WebSocket 事件",
  "helpHooks.wsEventsBody":
    "当钩子触发时，应用通过 WebSocket API 广播一个 JSON 事件，包含钩子名称、命令、文本、匹配标签、距离和时间戳。外部客户端可以监听这些事件以构建自定义自动化——例如调暗灯光、暂停音乐、发送 Slack 消息或记录到个人仪表盘。",
  "helpHooks.tipsTitle": "调优提示",
  "helpHooks.tipsBody": `从一个钩子和几个与您已录制标签匹配的关键词开始。使用距离建议工具设置初始阈值。监控历史日志一天并进行调整：如果看到误报则降低阈值，如果钩子从未触发则提高阈值。添加更具体的关键词（如"深度专注阅读"而非"专注"）通常可提高精确度。避免使用非常短或通用的单个词关键词，除非您想要广泛匹配。`,
};

export default help;
