// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** ZH "tts" namespace translations. */
const tts: Record<string, string> = {
  "ttsTab.backendSection": "语音引擎",
  "ttsTab.backendKitten": "KittenTTS",
  "ttsTab.backendKittenTag": "ONNX · 英语 · ~30 MB",
  "ttsTab.backendKittenDesc": "紧凑的 ONNX 模型，适用于任何 CPU，速度快，仅支持英语。",
  "ttsTab.backendNeutts": "NeuTTS",
  "ttsTab.backendNeuttsTag": "GGUF · 语音克隆 · 多语言",
  "ttsTab.backendNeuttsDesc":
    "基于 GGUF LLM 骨干网络和 NeuCodec 解码器。可克隆任意语音；支持英语、德语、法语、西班牙语。",
  "ttsTab.statusSection": "引擎状态",
  "ttsTab.statusReady": "就绪",
  "ttsTab.statusLoading": "加载中…",
  "ttsTab.statusIdle": "空闲",
  "ttsTab.statusUnloaded": "已卸载",
  "ttsTab.statusError": "失败",
  "ttsTab.preloadButton": "预加载",
  "ttsTab.retryButton": "重试",
  "ttsTab.unloadButton": "卸载",
  "ttsTab.errorTitle": "加载错误",
  "ttsTab.preloadOnStartup": "启动时预加载引擎",
  "ttsTab.preloadOnStartupDesc": "应用启动时在后台预热活动引擎",
  "ttsTab.requirements": "需要 PATH 中存在 espeak-ng",
  "ttsTab.requirementsDesc": "macOS: brew install espeak-ng · Ubuntu: apt install espeak-ng",
  "ttsTab.kittenConfigSection": "KittenTTS 设置",
  "ttsTab.kittenVoiceLabel": "语音",
  "ttsTab.kittenModelInfo": "KittenML/kitten-tts-mini-0.8 · 24 kHz · ~30 MB",
  "ttsTab.neuttsConfigSection": "NeuTTS 设置",
  "ttsTab.neuttsModelLabel": "骨干模型",
  "ttsTab.neuttsModelDesc": "较小的 GGUF 速度更快；较大的更自然。大多数系统推荐使用 Q4。",
  "ttsTab.neuttsVoiceSection": "参考语音",
  "ttsTab.neuttsVoiceDesc": "选择预设语音或提供您自己的 WAV 片段进行语音克隆。",
  "ttsTab.neuttsPresetLabel": "预设语音",
  "ttsTab.neuttsCustomOption": "自定义 WAV…",
  "ttsTab.neuttsRefWavLabel": "参考 WAV",
  "ttsTab.neuttsRefWavNone": "未选择文件",
  "ttsTab.neuttsRefWavBrowse": "浏览…",
  "ttsTab.neuttsRefTextLabel": "转录文本",
  "ttsTab.neuttsRefTextPlaceholder": "请准确输入 WAV 片段中所说的内容",
  "ttsTab.neuttsSaveButton": "保存",
  "ttsTab.neuttsSaved": "已保存",
  "ttsTab.voiceJo": "Jo",
  "ttsTab.voiceDave": "Dave",
  "ttsTab.voiceGreta": "Greta",
  "ttsTab.voiceJuliette": "Juliette",
  "ttsTab.voiceMateo": "Mateo",
  "ttsTab.voiceCustom": "自定义…",
  "ttsTab.testSection": "语音测试",
  "ttsTab.testDesc": '输入任意文本，然后按"朗读"试听当前引擎的效果。',
  "ttsTab.startupSection": "启动",
  "ttsTab.loggingSection": "调试日志",
  "ttsTab.loggingLabel": "TTS 合成日志",
  "ttsTab.loggingDesc": "将合成事件（文本、采样数、延迟）写入日志文件。",
  "ttsTab.apiSection": "API",
  "ttsTab.apiDesc": "通过 WebSocket 或 HTTP API 从任何脚本或工具触发语音：",
  "ttsTab.apiExampleWs": 'WebSocket:  {"command":"say","text":"Eyes closed."}',
  "ttsTab.apiExampleHttp": 'HTTP (curl): POST /say  body: {"text":"Eyes closed."}',

  "helpTts.overviewTitle": "设备端语音引导 (TTS)",
  "helpTts.overviewBody":
    "NeuroSkill™ 内置完全在设备端运行的英语文本转语音引擎。它会在校准阶段朗读提示（动作标签、休息、完成），并可通过 WebSocket 或 HTTP API 从任何脚本远程触发。所有合成均在本地运行——下载约 30 MB 的模型后无需联网。",
  "helpTts.howItWorksTitle": "工作原理",
  "helpTts.howItWorksBody":
    "文本预处理 → 句子分块（≤400 字符）→ 通过 libespeak-ng（C 库，进程内，en-us 语音）进行音素化 → 分词（IPA → 整数 ID）→ ONNX 推理（KittenTTS 模型：input_ids + style + speed → f32 波形）→ 1 秒静音填充 → rodio 在系统默认音频输出上播放。",
  "helpTts.modelTitle": "模型",
  "helpTts.modelBody":
    "来自 HuggingFace Hub 的 KittenML/kitten-tts-mini-0.8。语音：Jasper（英语 en-us）。采样率：24 000 Hz 单声道 float32。量化 INT8 ONNX——仅 CPU，无需 GPU。首次下载后缓存于 ~/.cache/huggingface/hub/。",
  "helpTts.requirementsTitle": "系统要求",
  "helpTts.requirementsBody":
    "espeak-ng 必须已安装并在 PATH 中——它提供进程内 IPA 音素化（作为 C 库链接，而非作为子进程启动）。macOS: brew install espeak-ng。Ubuntu/Debian: apt install libespeak-ng-dev。Alpine: apk add espeak-ng-dev。Fedora: dnf install espeak-ng-devel。",
  "helpTts.calibrationTitle": "校准集成",
  "helpTts.calibrationBody":
    "校准会话开始时，引擎会在后台预热（如需要则下载模型）。在每个阶段，校准窗口会调用 tts_speak 朗读动作标签、休息提示、完成消息或取消通知。语音不会阻塞校准——所有 TTS 调用均为即发即忘。",
  "helpTts.apiTitle": "API — say 命令",
  "helpTts.apiBody":
    '从任何外部脚本、自动化工具或 LLM 代理触发语音。命令立即返回，音频在后台播放。WebSocket: {"command":"say","text":"your message"}。HTTP: POST /say，请求体 {"text":"your message"}。CLI (curl): curl -X POST http://localhost:<port>/say -d \'{"text":"hello"}\' -H \'Content-Type: application/json\'。',
  "helpTts.loggingTitle": "调试日志",
  "helpTts.loggingBody":
    "在设置 → 语音中启用 TTS 合成日志，可将事件（朗读文本、采样数、推理延迟）写入 NeuroSkill™ 日志文件。有助于测量延迟和诊断问题。",
  "helpTts.testTitle": "在此测试",
  "helpTts.testBody": "使用下方的小工具直接从此帮助窗口测试 TTS 引擎。",
};

export default tts;
