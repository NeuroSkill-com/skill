// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** Korean "tts" namespace. */
const tts: Record<string, string> = {
  "ttsTab.backendSection": "음성 엔진",
  "ttsTab.backendKitten": "KittenTTS",
  "ttsTab.backendKittenTag": "ONNX · 영어 · ~30 MB",
  "ttsTab.backendKittenDesc": "컴팩트 ONNX 모델, 모든 CPU에서 빠르게 작동, 영어 전용.",
  "ttsTab.backendNeutts": "NeuTTS",
  "ttsTab.backendNeuttsTag": "GGUF · 음성 복제 · 다국어",
  "ttsTab.backendNeuttsDesc":
    "NeuCodec 디코더가 포함된 GGUF LLM 백본. 모든 음성을 복제할 수 있으며, 영어, 독일어, 프랑스어, 스페인어를 지원합니다.",
  "ttsTab.statusSection": "엔진 상태",
  "ttsTab.statusReady": "준비 완료",
  "ttsTab.statusLoading": "로딩 중…",
  "ttsTab.statusIdle": "대기 중",
  "ttsTab.statusUnloaded": "언로드됨",
  "ttsTab.statusError": "실패",
  "ttsTab.preloadButton": "프리로드",
  "ttsTab.retryButton": "재시도",
  "ttsTab.unloadButton": "언로드",
  "ttsTab.errorTitle": "로드 오류",
  "ttsTab.preloadOnStartup": "시작 시 엔진 프리로드",
  "ttsTab.preloadOnStartupDesc": "앱 실행 시 백그라운드에서 활성 엔진을 워밍업합니다",
  "ttsTab.requirements": "PATH에 espeak-ng 필요",
  "ttsTab.requirementsDesc": "macOS: brew install espeak-ng · Ubuntu: apt install espeak-ng",
  "ttsTab.kittenConfigSection": "KittenTTS 설정",
  "ttsTab.kittenVoiceLabel": "음성",
  "ttsTab.kittenModelInfo": "KittenML/kitten-tts-mini-0.8 · 24 kHz · ~30 MB",
  "ttsTab.neuttsConfigSection": "NeuTTS 설정",
  "ttsTab.neuttsModelLabel": "백본 모델",
  "ttsTab.neuttsModelDesc": "소형 GGUF = 빠름; 대형 = 자연스러움. Q4가 대부분의 시스템에 권장됩니다.",
  "ttsTab.neuttsVoiceSection": "참조 음성",
  "ttsTab.neuttsVoiceDesc": "프리셋 음성을 선택하거나 음성 복제를 위한 WAV 클립을 제공하세요.",
  "ttsTab.neuttsPresetLabel": "프리셋 음성",
  "ttsTab.neuttsCustomOption": "사용자 정의 WAV…",
  "ttsTab.neuttsRefWavLabel": "참조 WAV",
  "ttsTab.neuttsRefWavNone": "파일이 선택되지 않음",
  "ttsTab.neuttsRefWavBrowse": "찾아보기…",
  "ttsTab.neuttsRefTextLabel": "트랜스크립트",
  "ttsTab.neuttsRefTextPlaceholder": "WAV 클립에서 말하는 내용을 정확히 입력하세요",
  "ttsTab.neuttsSaveButton": "저장",
  "ttsTab.neuttsSaved": "저장됨",
  "ttsTab.voiceJo": "Jo",
  "ttsTab.voiceDave": "Dave",
  "ttsTab.voiceGreta": "Greta",
  "ttsTab.voiceJuliette": "Juliette",
  "ttsTab.voiceMateo": "Mateo",
  "ttsTab.voiceCustom": "사용자 정의…",
  "ttsTab.testSection": "음성 테스트",
  "ttsTab.testDesc": "텍스트를 입력하고 말하기를 눌러 활성 엔진을 테스트하세요.",
  "ttsTab.startupSection": "시작",
  "ttsTab.loggingSection": "디버그 로깅",
  "ttsTab.loggingLabel": "TTS 합성 로깅",
  "ttsTab.loggingDesc": "합성 이벤트(텍스트, 샘플 수, 지연 시간)를 로그 파일에 기록합니다.",
  "ttsTab.apiSection": "API",
  "ttsTab.apiDesc": "WebSocket 또는 HTTP API를 통해 모든 스크립트나 도구에서 음성을 트리거하세요:",
  "ttsTab.apiExampleWs": 'WebSocket:  {"command":"say","text":"Eyes closed."}',
  "ttsTab.apiExampleHttp": 'HTTP (curl): POST /say  body: {"text":"Eyes closed."}',

  "helpTts.overviewTitle": "온디바이스 음성 안내 (TTS)",
  "helpTts.overviewBody":
    "NeuroSkill™에는 완전히 온디바이스로 작동하는 영어 텍스트-음성 변환 엔진이 포함되어 있습니다. 캘리브레이션 단계(동작 라벨, 휴식, 완료)를 음성으로 안내하며, WebSocket 또는 HTTP API를 통해 모든 스크립트에서 원격으로 트리거할 수 있습니다. 모든 합성은 로컬에서 실행됩니다 — ~30 MB 모델이 한 번 다운로드되면 인터넷이 필요하지 않습니다.",
  "helpTts.howItWorksTitle": "작동 방식",
  "helpTts.howItWorksBody":
    "텍스트 전처리 → 문장 청킹(≤400자) → libespeak-ng를 통한 음소화(C 라이브러리, 인프로세스, en-us 음성) → 토큰화(IPA → 정수 ID) → ONNX 추론(KittenTTS 모델: input_ids + style + speed → f32 파형) → 1초 무음 패딩 → rodio가 시스템 기본 오디오 출력에서 재생.",
  "helpTts.modelTitle": "모델",
  "helpTts.modelBody":
    "HuggingFace Hub의 KittenML/kitten-tts-mini-0.8. 음성: Jasper (영어 en-us). 샘플레이트: 24,000 Hz 모노 float32. 양자화 INT8 ONNX — CPU 전용, GPU 불필요. 첫 다운로드 후 ~/.cache/huggingface/hub/에 캐시됩니다.",
  "helpTts.requirementsTitle": "요구 사항",
  "helpTts.requirementsBody":
    "espeak-ng가 설치되어 PATH에 있어야 합니다 — 인프로세스 IPA 음소화를 제공합니다(하위 프로세스가 아닌 C 라이브러리로 연결). macOS: brew install espeak-ng. Ubuntu/Debian: apt install libespeak-ng-dev. Alpine: apk add espeak-ng-dev. Fedora: dnf install espeak-ng-devel.",
  "helpTts.calibrationTitle": "캘리브레이션 통합",
  "helpTts.calibrationBody":
    "캘리브레이션 세션이 시작되면 엔진이 백그라운드에서 프리워밍됩니다(필요시 모델 다운로드). 각 단계에서 캘리브레이션 창이 동작 라벨, 휴식 안내, 완료 메시지 또는 취소 알림으로 tts_speak을 호출합니다. 음성은 캘리브레이션을 차단하지 않습니다 — 모든 TTS 호출은 비동기입니다.",
  "helpTts.apiTitle": "API — say 명령",
  "helpTts.apiBody":
    '외부 스크립트, 자동화 도구 또는 LLM 에이전트에서 음성을 트리거하세요. 명령은 오디오가 재생되는 동안 즉시 반환됩니다. WebSocket: {"command":"say","text":"your message"}. HTTP: POST /say with body {"text":"your message"}. CLI (curl): curl -X POST http://localhost:<port>/say -d \'{"text":"hello"}\' -H \'Content-Type: application/json\'.',
  "helpTts.loggingTitle": "디버그 로깅",
  "helpTts.loggingBody":
    "설정 → 음성에서 TTS 합성 로깅을 활성화하여 이벤트(발화 텍스트, 샘플 수, 추론 지연)를 NeuroSkill™ 로그 파일에 기록하세요. 지연 시간 측정 및 문제 진단에 유용합니다.",
  "helpTts.testTitle": "여기서 테스트",
  "helpTts.testBody": "아래 위젯을 사용하여 이 도움말 창에서 직접 TTS 엔진을 테스트하세요.",
};

export default tts;
