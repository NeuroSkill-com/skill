// SPDX-License-Identifier: GPL-3.0-only
/** ZH "virtual-eeg" namespace translations. */
const virtualEeg: Record<string, string> = {
  "settingsTabs.virtualEeg": "虚拟 EEG",

  "veeg.title": "虚拟脑电设备",
  "veeg.desc": "模拟 EEG 头戴设备，用于测试、演示和开发。生成的合成数据会流经完整的信号处理管线。",

  "veeg.status": "状态",
  "veeg.running": "运行中",
  "veeg.stopped": "已停止",
  "veeg.start": "启动",
  "veeg.stop": "停止",

  "veeg.channels": "通道数",
  "veeg.channelsDesc": "要模拟的 EEG 电极数量。",
  "veeg.sampleRate": "采样率 (Hz)",
  "veeg.sampleRateDesc": "每通道每秒采样数。",

  "veeg.template": "信号模板",
  "veeg.templateDesc": "选择要生成的合成信号类型。",
  "veeg.templateSine": "正弦波",
  "veeg.templateSineDesc": "标准频段（delta、theta、alpha、beta、gamma）的纯净正弦波。",
  "veeg.templateGoodQuality": "高质量 EEG",
  "veeg.templateGoodQualityDesc": "逼真的静息态 EEG，具有主导 alpha 节律和粉红噪声背景。",
  "veeg.templateBadQuality": "低质量 EEG",
  "veeg.templateBadQualityDesc": "带有肌肉伪迹、50/60 Hz 工频干扰和电极跳变的噪声信号。",
  "veeg.templateInterruptions": "间歇性连接",
  "veeg.templateInterruptionsDesc": "良好信号伴有周期性断连，模拟电极松动或无线干扰。",
  "veeg.templateFile": "从文件加载",
  "veeg.templateFileDesc": "从 CSV 或 EDF 文件回放采样数据。",

  "veeg.quality": "信号质量",
  "veeg.qualityDesc": "调整信噪比。数值越高，信号越干净。",
  "veeg.qualityPoor": "差",
  "veeg.qualityFair": "一般",
  "veeg.qualityGood": "良好",
  "veeg.qualityExcellent": "优秀",

  "veeg.chooseFile": "选择文件",
  "veeg.noFile": "未选择文件",
  "veeg.fileLoaded": "{name}（{channels}通道，{samples} 个采样）",

  "veeg.advanced": "高级选项",
  "veeg.amplitudeUv": "幅值 (µV)",
  "veeg.amplitudeDesc": "生成信号的峰峰值幅度。",
  "veeg.noiseUv": "噪声基底 (µV)",
  "veeg.noiseDesc": "叠加高斯噪声的均方根幅度。",
  "veeg.lineNoise": "工频干扰",
  "veeg.lineNoiseDesc": "添加 50 Hz 或 60 Hz 电网干扰。",
  "veeg.lineNoise50": "50 Hz",
  "veeg.lineNoise60": "60 Hz",
  "veeg.lineNoiseNone": "无",
  "veeg.dropoutProb": "信号丢失概率",
  "veeg.dropoutDesc": "每秒信号丢失的概率（0 = 无，1 = 持续丢失）。",

  "veeg.preview": "信号预览",
  "veeg.previewDesc": "前 4 个通道的实时预览。",

  // ── 虚拟设备窗口 ──────────────────────────────────────────────────────────
  "window.title.virtualDevices": "{app} – 虚拟设备",

  "vdev.title": "虚拟设备",
  "vdev.desc": "无需实体 EEG 硬件即可测试 NeuroSkill。选择一个与真实设备匹配的预设，或自定义合成信号源。",

  "vdev.presets": "设备预设",
  "vdev.statusRunning": "虚拟设备正在推流",
  "vdev.statusStopped": "没有运行中的虚拟设备",
  "vdev.selected": "就绪",
  "vdev.configure": "配置",
  "vdev.customConfig": "自定义配置",

  "vdev.presetMuse": "Muse S",
  "vdev.presetMuseDesc": "4 通道头带布局 — TP9、AF7、AF8、TP10。",
  "vdev.presetCyton": "OpenBCI Cyton",
  "vdev.presetCytonDesc": "8 通道研究级信号，完整额/中央导联组合。",
  "vdev.presetCap32": "32 通道 EEG 电极帽",
  "vdev.presetCap32Desc": "完整 10-20 国际系统，32 个电极。",
  "vdev.presetAlpha": "强 Alpha 节律",
  "vdev.presetAlphaDesc": "显著的 10 Hz alpha 节律 — 放松闭眼基线。",
  "vdev.presetArtifact": "伪迹测试",
  "vdev.presetArtifactDesc": "带有肌肉伪迹和 50 Hz 工频干扰的噪声信号。",
  "vdev.presetDropout": "信号丢失测试",
  "vdev.presetDropoutDesc": "周期性信号丢失，模拟电极松动。",
  "vdev.presetMinimal": "最小化（1 通道）",
  "vdev.presetMinimalDesc": "单通道正弦波 — 最轻量的负载。",
  "vdev.presetCustom": "自定义",
  "vdev.presetCustomDesc": "自行定义通道数、采样率、模板和噪声级别。",

  "vdev.lslSourceTitle": "虚拟 LSL 源",
  "vdev.lslRunning": "正在通过 LSL 推送合成 EEG 数据",
  "vdev.lslStopped": "虚拟 LSL 源已停止",
  "vdev.lslDesc": "启动本地 Lab Streaming Layer 源，以便测试 LSL 流发现和连接。",
  "vdev.lslHint": '打开主设置 → LSL 标签页，点击"扫描网络"即可在流列表中看到 SkillVirtualEEG，然后连接它。',
  "vdev.lslStarted": "虚拟 LSL 源现已在本地网络上推流。",

  // 状态面板
  "vdev.statusSource": "LSL 源",
  "vdev.statusSession": "会话",
  "vdev.sessionConnected": "已连接",
  "vdev.sessionConnecting": "连接中…",
  "vdev.sessionDisconnected": "已断开",
  "vdev.startBtn": "启动虚拟设备",
  "vdev.stopBtn": "停止虚拟设备",
  "vdev.autoConnect": "自动连接到仪表盘",
  "vdev.autoConnectDesc": "启动后立即将仪表盘连接到此信号源。",

  // 预览
  "vdev.previewOffline": "信号预览（离线）",
  "vdev.previewOfflineDesc": "客户端波形预览 — 在连接前显示信号形状。尚未推送任何数据。",

  // 自定义预设 — 通道 / 采样率
  "vdev.cfgChannels": "通道数",
  "vdev.cfgChannelsDesc": "要模拟的 EEG 电极数量。",
  "vdev.cfgRate": "采样率",
  "vdev.cfgRateDesc": "每通道每秒采样数。",

  // 自定义预设 — 信号质量
  "vdev.cfgQuality": "信号质量",
  "vdev.cfgQualityDesc": "信噪比。数值越高，信号越干净。",

  // 自定义预设 — 模板
  "vdev.cfgTemplate": "信号模板",
  "vdev.cfgTemplateSine": "正弦波",
  "vdev.cfgTemplateSineDesc": "delta、theta、alpha、beta 和 gamma 频段的纯正弦波。",
  "vdev.cfgTemplateGood": "高质量 EEG",
  "vdev.cfgTemplateGoodDesc": "逼真的静息态信号，具有主导 alpha 节律和粉红噪声背景。",
  "vdev.cfgTemplateBad": "低质量 EEG",
  "vdev.cfgTemplateBadDesc": "带有肌肉伪迹、工频干扰和电极跳变的噪声信号。",
  "vdev.cfgTemplateInterruptions": "间歇性连接",
  "vdev.cfgTemplateInterruptionsDesc": "良好信号伴有周期性断连，模拟电极松动。",

  // 自定义预设 — 高级
  "vdev.cfgAdvanced": "高级选项",
  "vdev.cfgAmplitude": "幅值 (µV)",
  "vdev.cfgAmplitudeDesc": "模拟信号的峰峰值幅度。",
  "vdev.cfgNoise": "噪声基底 (µV)",
  "vdev.cfgNoiseDesc": "叠加高斯背景噪声的均方根幅度。",
  "vdev.cfgLineNoise": "工频干扰",
  "vdev.cfgLineNoiseDesc": "注入 50 Hz 或 60 Hz 电网干扰。",
  "vdev.cfgLineNoiseNone": "无",
  "vdev.cfgLineNoise50": "50 Hz",
  "vdev.cfgLineNoise60": "60 Hz",
  "vdev.cfgDropout": "信号丢失概率",
  "vdev.cfgDropoutDesc": "每秒信号丢失的概率（0 = 从不，1 = 持续丢失）。",
};

export default virtualEeg;
