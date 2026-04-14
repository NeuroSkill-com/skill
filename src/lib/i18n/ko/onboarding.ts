// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** Korean "onboarding" namespace. */
const onboarding: Record<string, string> = {
  "onboarding.title": "{app}에 오신 것을 환영합니다",
  "onboarding.step.welcome": "환영",
  "onboarding.step.bluetooth": "Bluetooth",
  "onboarding.step.fit": "착용 확인",
  "onboarding.step.calibration": "캘리브레이션",
  "onboarding.step.models": "모델",
  "onboarding.step.tray": "트레이",
  "onboarding.step.enable_bluetooth": "Bluetooth 활성화",
  "onboarding.step.done": "완료",
  "onboarding.welcomeTitle": "{app}에 오신 것을 환영합니다",
  "onboarding.welcomeBody":
    "{app}은(는) 지원되는 BCI 기기에서 EEG 데이터를 녹화, 분석, 인덱싱합니다. 몇 단계로 설정을 완료하겠습니다.",
  "onboarding.bluetoothHint": "BCI 기기를 연결하세요",
  "onboarding.fitHint": "센서 접촉 품질을 확인하세요",
  "onboarding.calibrationHint": "빠른 캘리브레이션 세션을 실행하세요",
  "onboarding.modelsHint": "권장 로컬 AI 모델을 다운로드하세요",
  "onboarding.bluetoothTitle": "BCI 기기 연결",
  "onboarding.bluetoothBody":
    "BCI 기기의 전원을 켜고 착용하세요. {app}이(가) 근처 기기를 스캔하고 자동으로 연결합니다.",
  "onboarding.enableBluetoothTitle": "Mac에서 Bluetooth 활성화",
  "onboarding.enableBluetoothBody":
    "{app}이(가) BCI 기기를 찾고 연결하려면 Mac의 Bluetooth 어댑터가 켜져 있어야 합니다. 꺼져 있다면 시스템 설정에서 Bluetooth를 활성화하세요.",
  "onboarding.enableBluetoothStatus": "Bluetooth 어댑터",
  "onboarding.enableBluetoothHint":
    "Bluetooth 설정을 열고 Bluetooth를 켜세요. 터미널을 통한 개발 시에는 시스템 어댑터가 활성화되어 있는지 확인하세요.",
  "onboarding.enableBluetoothOpen": "Bluetooth 설정 열기",
  "onboarding.btConnected": "{name}에 연결됨",
  "onboarding.btScanning": "스캔 중…",
  "onboarding.btReady": "스캔 준비 완료",
  "onboarding.btScan": "스캔",
  "onboarding.btInstructions": "연결 방법",
  "onboarding.btStep1":
    "BCI 기기의 전원을 켜세요 (헤드셋에 따라 전원 버튼을 길게 누르거나, 스위치를 올리거나, 버튼을 누르세요).",
  "onboarding.btStep2":
    "헤드셋을 머리에 씌우세요 — 센서가 귀 뒤와 이마에 위치해야 합니다.",
  "onboarding.btStep3": "위의 스캔을 클릭하세요. {app}이(가) 가장 가까운 BCI 기기를 자동으로 찾아 연결합니다.",
  "onboarding.btSuccess": "헤드셋이 연결되었습니다! 계속 진행하세요.",
  "onboarding.fitTitle": "헤드셋 착용 확인",
  "onboarding.fitBody":
    "깨끗한 EEG 데이터를 위해서는 센서 접촉이 좋아야 합니다. 네 개의 센서 모두 녹색 또는 노란색이어야 합니다.",
  "onboarding.sensorQuality": "실시간 센서 품질",
  "onboarding.quality.good": "양호",
  "onboarding.quality.fair": "보통",
  "onboarding.quality.poor": "불량",
  "onboarding.quality.no_signal": "신호 없음",
  "onboarding.fitNeedsBt": "실시간 센서 데이터를 보려면 먼저 헤드셋을 연결하세요.",
  "onboarding.fitTips": "더 나은 접촉을 위한 팁",
  "onboarding.fitTip1":
    "귀 센서 (TP9/TP10): 귀 뒤 약간 위에 밀착시키세요. 센서를 가리는 머리카락을 치워주세요.",
  "onboarding.fitTip2":
    "이마 센서 (AF7/AF8): 깨끗한 피부에 평평하게 밀착시키세요 — 필요시 마른 천으로 닦으세요.",
  "onboarding.fitTip3":
    "접촉이 좋지 않다면 센서를 젖은 손가락으로 살짝 적시세요. 전도성이 향상됩니다.",
  "onboarding.fitGood": "착용 상태가 좋습니다! 모든 센서의 접촉이 양호합니다.",
  "onboarding.calibrationTitle": "캘리브레이션 실행",
  "onboarding.calibrationBody":
    "캘리브레이션은 두 가지 정신 상태를 번갈아 수행하면서 라벨이 지정된 EEG를 녹화합니다. {app}이(가) 뇌의 기본 패턴을 학습하는 데 도움이 됩니다.",
  "onboarding.openCalibration": "캘리브레이션 열기",
  "onboarding.calibrationNeedsBt": "캘리브레이션을 실행하려면 먼저 헤드셋을 연결하세요.",
  "onboarding.calibrationSkip": "건너뛰고 나중에 트레이 메뉴나 설정에서 캘리브레이션할 수 있습니다.",
  "onboarding.modelsTitle": "권장 모델 다운로드",
  "onboarding.modelsBody":
    "최상의 로컬 경험을 위해 지금 다운로드하세요: Qwen3.5 4B (Q4_K_M), ZUNA 인코더, NeuTTS, Kitten TTS.",
  "onboarding.models.downloadAll": "권장 세트 다운로드",
  "onboarding.models.download": "다운로드",
  "onboarding.models.downloading": "다운로드 중…",
  "onboarding.models.downloaded": "다운로드됨",
  "onboarding.models.qwenTitle": "Qwen3.5 4B (Q4_K_M)",
  "onboarding.models.qwenDesc":
    "권장 채팅 모델. 대부분의 노트북에서 최적의 품질/속도 균형을 위해 Q4_K_M을 사용합니다.",
  "onboarding.models.zunaTitle": "ZUNA EEG 인코더",
  "onboarding.models.zunaDesc": "EEG 임베딩, 시맨틱 기록, 하위 뇌 상태 분석에 필요합니다.",
  "onboarding.models.neuttsTitle": "NeuTTS (Nano Q4)",
  "onboarding.models.neuttsDesc": "더 나은 품질과 음성 복제를 지원하는 권장 다국어 음성 엔진.",
  "onboarding.models.kittenTitle": "Kitten TTS",
  "onboarding.models.kittenDesc":
    "가벼운 고속 음성 백엔드, 빠른 대체 수단 및 저사양 시스템에 유용합니다.",
  "onboarding.models.ocrTitle": "OCR 모델",
  "onboarding.models.ocrDesc":
    "스크린샷에서 텍스트를 추출하기 위한 텍스트 감지 + 인식 모델. 캡처한 화면에서 텍스트 검색이 가능합니다 (각 ~10 MB).",
  "onboarding.screenRecTitle": "화면 녹화 권한",
  "onboarding.screenRecDesc":
    "스크린샷 시스템에서 다른 앱의 창을 캡처하려면 macOS에서 필요합니다. 이 권한이 없으면 스크린샷이 빈 화면일 수 있습니다.",
  "onboarding.screenRecOpen": "설정 열기",
  "onboarding.trayTitle": "트레이에서 앱 찾기",
  "onboarding.trayBody":
    "{app}은(는) 백그라운드에서 조용히 실행됩니다. 설정 후에는 메뉴 바(macOS) 또는 시스템 트레이(Windows/Linux)의 아이콘이 앱으로 돌아가는 진입점입니다.",
  "onboarding.tray.states": "아이콘 색상이 상태를 나타냅니다:",
  "onboarding.tray.grey": "회색 — 연결 끊김",
  "onboarding.tray.amber": "황색 — 스캔 또는 연결 중",
  "onboarding.tray.green": "녹색 — 연결 및 녹화 중",
  "onboarding.tray.red": "빨간색 — Bluetooth 꺼짐",
  "onboarding.tray.open": "트레이 아이콘을 클릭하여 언제든지 메인 대시보드를 표시하거나 숨기세요.",
  "onboarding.tray.menu":
    "아이콘을 우클릭(Windows/Linux에서는 좌클릭)하여 빠른 작업 — 연결, 라벨, 캘리브레이션 등.",
  "onboarding.downloadsComplete": "모든 다운로드 완료!",
  "onboarding.downloadsCompleteBody":
    "권장 모델이 다운로드되어 사용 준비가 되었습니다. 더 많은 모델을 다운로드하거나 다른 모델로 전환하려면",
  "onboarding.downloadMoreSettings": "앱 설정",
  "onboarding.doneTitle": "모든 준비가 완료되었습니다!",
  "onboarding.doneBody": "{app}이(가) 메뉴 바에서 실행 중입니다. 알아두면 좋은 점:",
  "onboarding.doneTip.tray": "{app}은(는) 메뉴 바 트레이에 있습니다. 아이콘을 클릭하여 대시보드를 표시/숨기세요.",
  "onboarding.doneTip.shortcuts": "⌘K로 명령 팔레트를 열거나 ?로 모든 키보드 단축키를 확인하세요.",
  "onboarding.doneTip.help": "트레이 메뉴에서 도움말을 열어 모든 기능에 대한 전체 참조를 확인하세요.",
  "onboarding.back": "뒤로",
  "onboarding.next": "다음",
  "onboarding.getStarted": "시작하기",
  "onboarding.finish": "완료",
};

export default onboarding;
