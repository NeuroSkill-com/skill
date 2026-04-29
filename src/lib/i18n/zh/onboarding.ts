// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** ZH "onboarding" namespace translations. */
const onboarding: Record<string, string> = {
  "onboarding.title": "欢迎使用 {app}",
  "onboarding.step.welcome": "欢迎",
  "onboarding.step.bluetooth": "Bluetooth",
  "onboarding.step.fit": "佩戴检查",
  "onboarding.step.calibration": "校准",
  "onboarding.step.models": "模型",
  "onboarding.step.tray": "托盘",
  "onboarding.step.permissions": "权限",
  "onboarding.step.extensions": "扩展",
  "onboarding.step.enable_bluetooth": "启用 Bluetooth",
  "onboarding.step.done": "完成",
  "onboarding.newBadge": "新",
  "onboarding.fontSizeLabel": "文字大小",
  "onboarding.fontSizeDecrease": "缩小文字",
  "onboarding.fontSizeIncrease": "放大文字",
  "onboarding.welcomeBackTitle": "欢迎回到 {app}",
  "onboarding.whatsNewTitle": "自上次设置以来的新内容",
  "onboarding.whatsNewBody":
    '自从你上次运行此向导以来，我们添加了一些新步骤。你现有的设置（Bluetooth、校准、模型）保持不变 — 可以快速浏览。新步骤在此处标记，并在进度条中标有"新"标签：',
  "onboarding.trayHint": "在菜单栏 / 托盘中找到应用图标",
  "onboarding.permissionsHint": "可选：允许捕获活动应用、文件、剪贴板",
  "onboarding.extensionsHint": "可选：安装 VS Code、浏览器和 shell 助手",
  "onboarding.welcomeTitle": "欢迎使用 {app}",
  "onboarding.welcomeBody":
    "{app} 可以记录、分析和索引来自任何受支持 BCI 设备的 EEG 数据。让我们通过几个简单的步骤完成设置。",
  "onboarding.bluetoothHint": "连接您的 BCI 设备",
  "onboarding.fitHint": "检查传感器接触质量",
  "onboarding.calibrationHint": "运行快速校准会话",
  "onboarding.modelsHint": "下载推荐的本地 AI 模型",
  "onboarding.bluetoothTitle": "连接您的 BCI 设备",
  "onboarding.bluetoothBody": "打开您的 BCI 设备并佩戴好。{app} 将扫描附近的设备并自动连接。",
  "onboarding.enableBluetoothTitle": "在您的 Mac 上启用 Bluetooth",
  "onboarding.enableBluetoothBody":
    "{app} 需要您的 Mac 的 Bluetooth 适配器处于开启状态，以便查找和连接您的 BCI 设备。如果 Bluetooth 已关闭，请在系统设置中启用。",
  "onboarding.enableBluetoothStatus": "Bluetooth 适配器",
  "onboarding.enableBluetoothHint":
    "打开 Bluetooth 设置并开启 Bluetooth。如果通过终端进行开发，请确保系统适配器已启用。",
  "onboarding.enableBluetoothOpen": "打开 Bluetooth 设置",
  "onboarding.btConnected": "已连接到 {name}",
  "onboarding.btScanning": "正在扫描…",
  "onboarding.btReady": "准备扫描",
  "onboarding.btScan": "扫描",
  "onboarding.btInstructions": "如何连接",
  "onboarding.btStep1": "打开您的 BCI 设备（根据您的头戴设备，长按电源按钮、拨动开关或按下按钮）。",
  "onboarding.btStep2": "将头戴设备戴在头上——传感器应贴合在耳后和前额位置。",
  "onboarding.btStep3": `点击上方的"扫描"。{app} 将自动查找并连接到最近的 BCI 设备。`,
  "onboarding.btSuccess": "头戴设备已连接！您可以继续。",
  "onboarding.fitTitle": "检查头戴设备佩戴",
  "onboarding.fitBody": "良好的传感器接触对于获取干净的 EEG 数据至关重要。所有四个传感器应显示为绿色或黄色。",
  "onboarding.sensorQuality": "实时传感器质量",
  "onboarding.quality.good": "良好",
  "onboarding.quality.fair": "一般",
  "onboarding.quality.poor": "较差",
  "onboarding.quality.no_signal": "无信号",
  "onboarding.fitNeedsBt": "请先连接头戴设备以查看实时传感器数据。",
  "onboarding.fitTips": "改善接触的建议",
  "onboarding.fitTip1": "耳部传感器（TP9/TP10）：放置在耳后稍偏上方。拨开覆盖传感器的头发。",
  "onboarding.fitTip2": "前额传感器（AF7/AF8）：应平贴在干净的皮肤上——如有需要，用干布擦拭。",
  "onboarding.fitTip3": "如果接触不良，用湿润的手指轻轻擦拭传感器。这可以改善导电性。",
  "onboarding.fitGood": "佩戴良好！所有传感器接触正常。",
  "onboarding.calibrationTitle": "运行校准",
  "onboarding.calibrationBody":
    "校准会在您交替进行两种心理状态时记录带标签的 EEG 数据。这有助于 {app} 学习您的大脑基线模式。",
  "onboarding.openCalibration": "打开校准",
  "onboarding.calibrationNeedsBt": "请先连接头戴设备以运行校准。",
  "onboarding.calibrationSkip": "您可以跳过此步骤，稍后从托盘菜单或设置中进行校准。",
  "onboarding.modelsTitle": "下载推荐模型",
  "onboarding.modelsBody":
    "为获得最佳本地体验，请立即下载以下默认模型：Qwen3.5 4B (Q4_K_M)、ZUNA 编码器、NeuTTS 和 Kitten TTS。",
  "onboarding.models.downloadAll": "下载推荐模型集",
  "onboarding.models.download": "下载",
  "onboarding.models.downloading": "正在下载…",
  "onboarding.models.downloaded": "已下载",
  "onboarding.models.qwenTitle": "Qwen3.5 4B (Q4_K_M)",
  "onboarding.models.qwenDesc": "推荐的对话模型。使用 Q4_K_M 量化，在大多数笔记本电脑上实现最佳质量与速度的平衡。",
  "onboarding.models.zunaTitle": "ZUNA EEG 编码器",
  "onboarding.models.zunaDesc": "用于 EEG 嵌入、语义历史记录和下游脑状态分析。",
  "onboarding.models.neuttsTitle": "NeuTTS (Nano Q4)",
  "onboarding.models.neuttsDesc": "推荐的多语言语音引擎，具有更好的质量和语音克隆支持。",
  "onboarding.models.kittenTitle": "Kitten TTS",
  "onboarding.models.kittenDesc": "轻量级快速语音后端，可作为快速备选方案，适用于低资源系统。",
  "onboarding.models.ocrTitle": "OCR 模型",
  "onboarding.models.ocrDesc": "文本检测和识别模型，用于从截图中提取文字。支持跨捕获屏幕的文本搜索（每个约 10 MB）。",
  "onboarding.screenRecTitle": "屏幕录制权限",
  "onboarding.screenRecDesc":
    "在 macOS 上需要此权限才能捕获其他应用程序窗口以用于截图系统。没有此权限，截图可能为空白。",
  "onboarding.screenRecOpen": "打开设置",
  "onboarding.trayTitle": "在托盘中找到应用",
  "onboarding.trayBody":
    "{app} 在后台静默运行。设置完成后，菜单栏（macOS）或系统托盘（Windows/Linux）中的图标是您返回应用的入口。",
  "onboarding.tray.states": "图标会变色以显示状态：",
  "onboarding.tray.grey": "灰色 — 未连接",
  "onboarding.tray.amber": "琥珀色 — 正在扫描或连接",
  "onboarding.tray.green": "绿色 — 已连接并正在记录",
  "onboarding.tray.red": "红色 — Bluetooth 已关闭",
  "onboarding.tray.open": "随时点击托盘图标即可显示或隐藏主仪表板。",
  "onboarding.tray.menu": "右键点击图标（或在 Windows/Linux 上左键点击）可快速操作——连接、标注、校准等。",
  "onboarding.extensionsTitle": "配套扩展",
  "onboarding.extensionsBody":
    "{app} 可以从你的编辑器、浏览器和终端中获取额外的上下文。每个集成都是可独立安装或跳过的单独组件 — EEG 功能的运行不需要其中任何一个。",
  "onboarding.extensionsPrivacy":
    "与其他所有内容相同的隐私保证：每个扩展通过 localhost 端口向本地守护进程报告，这些数据写入这台电脑上的 activity.sqlite。不会上传到 NeuroSkill 或其他任何人。",
  "onboarding.extensionsSkip": "全部可选。你可以稍后在 设置 → 扩展 和 设置 → 终端 中安装、更新或移除任何扩展。",
  "onboarding.extensions.vscodeTitle": "VS Code 系列编辑器",
  "onboarding.extensions.vscodeDesc":
    "添加按文件的编辑跟踪、AI 内联建议以及与开发循环的集成。适用于 VS Code、VSCodium、Cursor、Windsurf、Trae、Positron — 自动检测已安装的分支。",
  "onboarding.extensions.browserTitle": "浏览器扩展",
  "onboarding.extensions.browserDesc":
    "记录浏览器中的活动标签页、页面焦点时间和阅读模式。支持 Chrome、Firefox、Edge 和 Safari 旁加载（Safari 需要额外的签名步骤）。",
  "onboarding.extensions.terminalTitle": "终端 / Shell 钩子",
  "onboarding.extensions.terminalDesc":
    "向你的 shell 添加一个小的 preexec/precmd 钩子，让应用能够将命令时间与专注状态关联起来。可选 zsh、bash、fish 或 PowerShell — 在 rc 文件中只添加一行 source，之后可以完全移除。",

  "onboarding.permissionsTitle": "可选的活动跟踪",
  "onboarding.permissionsBody":
    '{app} 可以记录你正在做什么，将 EEG／专注数据与实际上下文关联起来 — "我在写这个 PR 时分心了"，而不仅仅是"下午 3 点分心了"。默认关闭，完全可选。',
  "onboarding.permissionsPrivacy":
    "所有内容都保留在这台电脑上。记录的活动写入本地 activity.sqlite 文件，从不发送到任何服务器 — 不发送到 NeuroSkill，也不发送到任何人。你可以随时关闭每一项；已记录的数据会保留在磁盘上，直到你删除它。",
  "onboarding.permissionsSkip": "默认全部关闭。你可以稍后在 设置 → 活动跟踪 中启用任何选项。",
  "onboarding.permissionsActiveWindowDesc":
    "捕获前台应用、窗口标题、活动浏览器标签页和编辑器中打开的文件路径。macOS 会针对每个浏览器和编辑器请求辅助功能 / 自动化访问权限。",
  "onboarding.permissionsInputDesc":
    "仅记录键盘／鼠标使用的时间戳 — 从不记录按了哪些键、位置或内容。无需操作系统权限。",
  "onboarding.permissionsFileDesc":
    "监视 Documents、Desktop、Downloads 和常用开发文件夹的创建/修改/删除事件。仅记录路径和时间戳 — 文件内容从不被读取。macOS 可能会请求完全磁盘访问权限。",
  "onboarding.permissionsScreenshotsDesc":
    '按间隔捕获屏幕，对文本运行 OCR，并将两者编入索引以进行视觉搜索和"下午 3 点屏幕上有什么"查询。macOS 会请求屏幕录制。在 设置 → 截屏 中调整间隔、质量和 OCR。',
  "onboarding.permissionsLocationDesc":
    "将设备位置与专注块一起记录（家与办公室与咖啡厅），以便将场所切换与你的专注状态相关联。macOS 会请求位置服务。本地存储，从不上传。",
  "onboarding.permissionsCalendarDesc":
    "读取日历事件元数据（标题、时间、持续时间、参与人数），将会议密度与专注力下降相关联。macOS 在首次使用时会请求日历访问。事件内容从不上传。",
  "onboarding.permissionsClipboardDesc":
    "记录剪贴板何时变化（哪个应用、内容类型、大小）。内容本身从不被读取。仅限 macOS；将请求自动化访问权限。",
  "onboarding.downloadsComplete": "所有下载已完成！",
  "onboarding.downloadsCompleteBody": "推荐模型已下载完毕，可以使用。如需下载更多模型或切换到其他模型，请打开",
  "onboarding.downloadMoreSettings": "应用设置",
  "onboarding.doneTitle": "一切就绪！",
  "onboarding.doneBody": "{app} 正在菜单栏中运行。以下是一些须知事项：",
  "onboarding.doneTip.tray": "{app} 位于菜单栏托盘中。点击图标即可显示/隐藏仪表板。",
  "onboarding.doneTip.shortcuts": "使用 ⌘K 打开命令面板，或按 ? 查看所有键盘快捷键。",
  "onboarding.doneTip.help": `从托盘菜单中打开"帮助"以查看所有功能的完整参考。`,
  "onboarding.back": "上一步",
  "onboarding.next": "下一步",
  "onboarding.getStarted": "开始使用",
  "onboarding.finish": "完成",
};

export default onboarding;
