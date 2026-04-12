# 设备端语音引导 (TTS)

## 设备端语音引导 (TTS)
NeuroSkill™ 内置完全在设备端运行的英语文本转语音引擎。它会在校准阶段朗读提示（动作标签、休息、完成），并可通过 WebSocket 或 HTTP API 从任何脚本远程触发。所有合成均在本地运行——下载约 30 MB 的模型后无需联网。

## 工作原理
文本预处理 → 句子分块（≤400 字符）→ 通过 libespeak-ng（C 库，进程内，en-us 语音）进行音素化 → 分词（IPA → 整数 ID）→ ONNX 推理（KittenTTS 模型：input_ids + style + speed → f32 波形）→ 1 秒静音填充 → rodio 在系统默认音频输出上播放。

## 模型
来自 HuggingFace Hub 的 KittenML/kitten-tts-mini-0.8。语音：Jasper（英语 en-us）。采样率：24 000 Hz 单声道 float32。量化 INT8 ONNX——仅 CPU，无需 GPU。首次下载后缓存于 ~/.cache/huggingface/hub/。

## 系统要求
espeak-ng 必须已安装并在 PATH 中——它提供进程内 IPA 音素化（作为 C 库链接，而非作为子进程启动）。macOS: brew install espeak-ng。Ubuntu/Debian: apt install libespeak-ng-dev。Alpine: apk add espeak-ng-dev。Fedora: dnf install espeak-ng-devel。

## 校准集成
校准会话开始时，引擎会在后台预热（如需要则下载模型）。在每个阶段，校准窗口会调用 tts_speak 朗读动作标签、休息提示、完成消息或取消通知。语音不会阻塞校准——所有 TTS 调用均为即发即忘。

## API — say 命令
从任何外部脚本、自动化工具或 LLM 代理触发语音。命令立即返回，音频在后台播放。WebSocket: {"command":"say","text":"your message"}。HTTP: POST /say，请求体 {"text":"your message"}。CLI (curl): curl -X POST http://localhost:<port>/say -d '{"text":"hello"}' -H 'Content-Type: application/json'。

## 调试日志
在设置 → 语音中启用 TTS 合成日志，可将事件（朗读文本、采样数、推理延迟）写入 NeuroSkill™ 日志文件。有助于测量延迟和诊断问题。

## 在此测试
使用下方的小工具直接从此帮助窗口测试 TTS 引擎。
