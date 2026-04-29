// SPDX-License-Identifier: GPL-3.0-only
/** ZH — "validation" namespace. */
const validation: Record<string, string> = {
  "settingsTabs.validation": "验证",
  "validation.title": "验证与研究",
  "validation.intro": "选择启用的研究工具，用外部测量校准休息教练和专注度评分。使用 NeuroSkill 不需要它们。",
  "validation.disclaimer": "仅研究工具 — 非医疗器械。未经 FDA、CE 或任何监管机构批准。不适用于临床使用。",

  "validation.master.title": "全局门控",
  "validation.master.respectFlow": "尊重心流状态",
  "validation.master.respectFlowDesc": "进入心流后，下面所有提示都会被抑制。默认启用 — 保持启用。",
  "validation.master.quietBefore": "安静时间开始",
  "validation.master.quietAfter": "安静时间结束",
  "validation.master.quietDesc": "本地时间。此窗口外不发提示。开始 = 结束 完全禁用安静时间。",

  "validation.kss.title": "卡罗林斯卡嗜睡量表 (KSS)",
  "validation.kss.desc": "瞬间困倦的 5 秒自我报告 (1-9)。用于将休息教练校准到主观状态。",
  "validation.kss.enabled": "启用 KSS 提示",
  "validation.kss.maxPerDay": "每天最大提示数",
  "validation.kss.minInterval": "提示间最小分钟数",
  "validation.kss.triggerBreakCoach": "休息教练检测到疲劳时触发",
  "validation.kss.triggerRandom": "偶尔触发随机对照样本",
  "validation.kss.triggerRandomDesc": "计算 ROC/AUC 所需 — 没有阴性样本只能看到疲劳阳性病例。",
  "validation.kss.randomWeight": "随机样本权重 (0-1)",

  "validation.tlx.title": "NASA-TLX (工作负荷，原始 6 量表)",
  "validation.tlx.desc": "工作单元结束后的 60 秒 6 子量表工作负荷自我报告。测量负荷 — 与 KSS 困倦互补。",
  "validation.tlx.enabled": "启用 NASA-TLX 提示",
  "validation.tlx.maxPerDay": "每天最大提示数",
  "validation.tlx.minTaskMin": "询问的最小任务长度 (分钟)",
  "validation.tlx.endOfDay": "也触发一天结束的工作负荷摘要",

  "validation.tlx.form.title": "评估您刚完成的任务",
  "validation.tlx.mental": "脑力需求",
  "validation.tlx.physical": "体力需求",
  "validation.tlx.temporal": "时间需求",
  "validation.tlx.performance": "绩效",
  "validation.tlx.effort": "努力",
  "validation.tlx.frustration": "挫折",

  "validation.pvt.title": "精神运动警觉性任务 (PVT)",
  "validation.pvt.desc": "3 分钟反应时间任务。客观警觉性测量 — 收集慢但文献中信号最强。",
  "validation.pvt.enabled": "启用每周 PVT 提醒",
  "validation.pvt.weeklyReminder": "本周没有 PVT 时显示一行提醒",
  "validation.pvt.runNow": "立即运行 PVT (3 分钟)",
  "validation.pvt.task.start": "开始",
  "validation.pvt.task.cancel": "取消",
  "validation.pvt.task.close": "关闭",

  "validation.eeg.title": "EEG 疲劳指数 (Jap et al. 2009)",
  "validation.eeg.desc": "连接 NeuroSkill 头戴设备时，从频段功率流中持续计算。公式: (α + θ) / β。被动 — 无成本。",
  "validation.eeg.enabled": "计算 EEG 疲劳指数",
  "validation.eeg.windowSecs": "滚动窗口 (秒)",
  "validation.eeg.current": "当前值",
  "validation.eeg.noHeadset": "没有 EEG 头戴设备在传输",

  "validation.calibrationWeek.title": "校准周",
  "validation.calibrationWeek.desc":
    "选择启用的 7 天高频采样脉冲。将 KSS 增加到每天 8 次，每个 ≥20 分钟的心流块后触发 TLX，本周中要求一次 PVT。第 8 天自动恢复正常设置。",
  "validation.calibrationWeek.start": "开始校准周",

  "validation.results.title": "最近结果",
  "validation.save.saved": "已保存",
};
export default validation;
