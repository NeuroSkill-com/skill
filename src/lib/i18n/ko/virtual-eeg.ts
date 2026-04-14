// SPDX-License-Identifier: GPL-3.0-only
/** Korean "virtual-eeg" namespace. */
const virtualEeg: Record<string, string> = {
  "settingsTabs.virtualEeg": "가상 EEG",

  "veeg.title": "가상 EEG 기기",
  "veeg.desc":
    "테스트, 데모, 개발을 위한 EEG 헤드셋 시뮬레이션. 전체 신호 파이프라인을 통해 흐르는 합성 데이터를 생성합니다.",

  "veeg.status": "상태",
  "veeg.running": "실행 중",
  "veeg.stopped": "중지됨",
  "veeg.start": "시작",
  "veeg.stop": "중지",

  "veeg.channels": "채널",
  "veeg.channelsDesc": "시뮬레이션할 EEG 전극 수.",
  "veeg.sampleRate": "샘플레이트 (Hz)",
  "veeg.sampleRateDesc": "채널당 초당 샘플 수.",

  "veeg.template": "신호 템플릿",
  "veeg.templateDesc": "생성할 합성 신호 유형을 선택하세요.",
  "veeg.templateSine": "사인파",
  "veeg.templateSineDesc": "표준 주파수 대역(델타, 세타, 알파, 베타, 감마)의 깨끗한 사인파.",
  "veeg.templateGoodQuality": "양호한 품질 EEG",
  "veeg.templateGoodQualityDesc": "지배적 알파 리듬과 핑크 노이즈 배경을 가진 사실적인 안정 상태 EEG.",
  "veeg.templateBadQuality": "불량한 품질 EEG",
  "veeg.templateBadQualityDesc": "근육 아티팩트, 50/60 Hz 라인 노이즈, 전극 팝이 있는 노이즈 신호.",
  "veeg.templateInterruptions": "간헐적 연결",
  "veeg.templateInterruptionsDesc":
    "느슨한 전극이나 무선 간섭을 시뮬레이션하는 주기적 드롭아웃이 있는 양호한 신호.",
  "veeg.templateFile": "파일에서",
  "veeg.templateFileDesc": "CSV 또는 EDF 파일에서 샘플을 재생합니다.",

  "veeg.quality": "신호 품질",
  "veeg.qualityDesc": "신호 대 잡음비를 조정합니다. 높을수록 깨끗한 신호.",
  "veeg.qualityPoor": "불량",
  "veeg.qualityFair": "보통",
  "veeg.qualityGood": "양호",
  "veeg.qualityExcellent": "우수",

  "veeg.chooseFile": "파일 선택",
  "veeg.noFile": "파일이 선택되지 않음",
  "veeg.fileLoaded": "{name} ({channels}ch, {samples}개 샘플)",

  "veeg.advanced": "고급",
  "veeg.amplitudeUv": "진폭 (µV)",
  "veeg.amplitudeDesc": "생성된 신호의 피크-투-피크 진폭.",
  "veeg.noiseUv": "노이즈 플로어 (µV)",
  "veeg.noiseDesc": "가산 가우시안 노이즈의 RMS 진폭.",
  "veeg.lineNoise": "라인 노이즈",
  "veeg.lineNoiseDesc": "50 Hz 또는 60 Hz 전원 간섭을 추가합니다.",
  "veeg.lineNoise50": "50 Hz",
  "veeg.lineNoise60": "60 Hz",
  "veeg.lineNoiseNone": "없음",
  "veeg.dropoutProb": "드롭아웃 확률",
  "veeg.dropoutDesc": "초당 신호 드롭아웃 확률 (0 = 없음, 1 = 항상).",

  "veeg.preview": "신호 미리보기",
  "veeg.previewDesc": "처음 4개 채널의 실시간 미리보기.",

  // ── Virtual Devices window ────────────────────────────────────────────────
  "window.title.virtualDevices": "{app} – 가상 기기",

  "vdev.title": "가상 기기",
  "vdev.desc":
    "물리적 EEG 하드웨어 없이 NeuroSkill을 테스트하세요. 실제 기기에 맞는 프리셋을 선택하거나 자체 합성 신호 소스를 구성하세요.",

  "vdev.presets": "기기 프리셋",
  "vdev.statusRunning": "가상 기기 스트리밍 중",
  "vdev.statusStopped": "실행 중인 가상 기기 없음",
  "vdev.selected": "준비 완료",
  "vdev.configure": "구성",
  "vdev.customConfig": "사용자 정의 구성",

  "vdev.presetMuse": "Muse S",
  "vdev.presetMuseDesc": "4채널 헤드밴드 레이아웃 — TP9, AF7, AF8, TP10.",
  "vdev.presetCyton": "OpenBCI Cyton",
  "vdev.presetCytonDesc": "8채널 연구용 신호, 전두/중앙 몽타주.",
  "vdev.presetCap32": "32채널 EEG 캡",
  "vdev.presetCap32Desc": "전체 10-20 국제 시스템, 32개 전극.",
  "vdev.presetAlpha": "강한 알파",
  "vdev.presetAlphaDesc": "두드러진 10 Hz 알파 리듬 — 눈 감은 이완 베이스라인.",
  "vdev.presetArtifact": "아티팩트 테스트",
  "vdev.presetArtifactDesc": "근육 아티팩트와 50 Hz 라인 노이즈가 있는 노이즈 신호.",
  "vdev.presetDropout": "드롭아웃 테스트",
  "vdev.presetDropoutDesc": "느슨한 전극을 시뮬레이션하는 주기적 신호 손실.",
  "vdev.presetMinimal": "최소 (1ch)",
  "vdev.presetMinimalDesc": "단일 채널 사인파 — 가장 가벼운 부하.",
  "vdev.presetCustom": "사용자 정의",
  "vdev.presetCustomDesc": "채널 수, 샘플레이트, 템플릿, 노이즈 수준을 직접 정의하세요.",

  "vdev.lslSourceTitle": "가상 LSL 소스",
  "vdev.lslRunning": "LSL을 통해 합성 EEG 스트리밍 중",
  "vdev.lslStopped": "가상 LSL 소스 중지됨",
  "vdev.lslDesc": "LSL 스트림 검색 및 연결을 테스트할 수 있도록 로컬 Lab Streaming Layer 소스를 시작합니다.",
  "vdev.lslHint":
    '메인 설정 → LSL 탭에서 "네트워크 스캔"을 클릭하면 스트림 목록에서 SkillVirtualEEG를 볼 수 있으며, 그것에 연결하세요.',
  "vdev.lslStarted": "가상 LSL 소스가 로컬 네트워크에서 스트리밍 중입니다.",

  // Status panel
  "vdev.statusSource": "LSL 소스",
  "vdev.statusSession": "세션",
  "vdev.sessionConnected": "연결됨",
  "vdev.sessionConnecting": "연결 중…",
  "vdev.sessionDisconnected": "연결 끊김",
  "vdev.startBtn": "가상 기기 시작",
  "vdev.stopBtn": "가상 기기 중지",
  "vdev.autoConnect": "대시보드에 자동 연결",
  "vdev.autoConnectDesc": "시작 직후 대시보드를 이 소스에 연결합니다.",

  // Preview
  "vdev.previewOffline": "신호 미리보기 (오프라인)",
  "vdev.previewOfflineDesc":
    "클라이언트 측 파형 미리보기 — 연결 전 신호 형태를 보여줍니다. 아직 데이터가 스트리밍되지 않습니다.",

  // Custom preset — channel / rate
  "vdev.cfgChannels": "채널",
  "vdev.cfgChannelsDesc": "시뮬레이션할 EEG 전극 수.",
  "vdev.cfgRate": "샘플레이트",
  "vdev.cfgRateDesc": "채널당 초당 샘플 수.",

  // Custom preset — signal quality
  "vdev.cfgQuality": "신호 품질",
  "vdev.cfgQualityDesc": "신호 대 잡음비. 높을수록 깨끗한 신호.",

  // Custom preset — template
  "vdev.cfgTemplate": "신호 템플릿",
  "vdev.cfgTemplateSine": "사인파",
  "vdev.cfgTemplateSineDesc": "델타, 세타, 알파, 베타, 감마 주파수의 순수 사인파.",
  "vdev.cfgTemplateGood": "양호한 품질 EEG",
  "vdev.cfgTemplateGoodDesc": "지배적 알파와 핑크 노이즈 배경을 가진 사실적인 안정 상태.",
  "vdev.cfgTemplateBad": "불량한 품질 EEG",
  "vdev.cfgTemplateBadDesc": "근육 아티팩트, 라인 노이즈, 전극 팝이 있는 노이즈 신호.",
  "vdev.cfgTemplateInterruptions": "간헐적 연결",
  "vdev.cfgTemplateInterruptionsDesc": "느슨한 전극을 시뮬레이션하는 주기적 드롭아웃이 있는 양호한 신호.",

  // Custom preset — advanced
  "vdev.cfgAdvanced": "고급",
  "vdev.cfgAmplitude": "진폭 (µV)",
  "vdev.cfgAmplitudeDesc": "시뮬레이션된 신호의 피크-투-피크 진폭.",
  "vdev.cfgNoise": "노이즈 플로어 (µV)",
  "vdev.cfgNoiseDesc": "가산 가우시안 배경 노이즈의 RMS 진폭.",
  "vdev.cfgLineNoise": "라인 노이즈",
  "vdev.cfgLineNoiseDesc": "50 Hz 또는 60 Hz 전원 간섭을 주입합니다.",
  "vdev.cfgLineNoiseNone": "없음",
  "vdev.cfgLineNoise50": "50 Hz",
  "vdev.cfgLineNoise60": "60 Hz",
  "vdev.cfgDropout": "드롭아웃 확률",
  "vdev.cfgDropoutDesc": "초당 신호 드롭아웃 확률 (0 = 없음, 1 = 항상).",
};

export default virtualEeg;
