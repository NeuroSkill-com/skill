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
  "onboarding.step.permissions": "권한",
  "onboarding.step.extensions": "확장 프로그램",
  "onboarding.step.enable_bluetooth": "Bluetooth 활성화",
  "onboarding.step.done": "완료",
  "onboarding.newBadge": "새로 추가",
  "onboarding.fontSizeLabel": "텍스트 크기",
  "onboarding.fontSizeDecrease": "텍스트 크기 줄이기",
  "onboarding.fontSizeIncrease": "텍스트 크기 키우기",
  "onboarding.welcomeBackTitle": "{app}에 다시 오신 것을 환영합니다",
  "onboarding.whatsNewTitle": "지난 설정 이후 새로운 내용",
  "onboarding.whatsNewBody":
    '마법사를 마지막으로 실행한 이후 몇 가지 새로운 단계가 추가되었습니다. 기존 설정(Bluetooth, 보정, 모델)은 변경되지 않았으므로 가볍게 훑어보시면 됩니다. 새 단계는 여기에 표시되며 진행률 표시줄에 "새로 추가"로 태그됩니다:',
  "onboarding.trayHint": "메뉴 표시줄 / 트레이에서 앱 아이콘 찾기",
  "onboarding.permissionsHint": "선택 사항: 활성 앱, 파일, 클립보드 캡처 허용",
  "onboarding.extensionsHint": "선택 사항: VS Code, 브라우저, 셸 도우미 설치",
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
  "onboarding.btStep2": "헤드셋을 머리에 씌우세요 — 센서가 귀 뒤와 이마에 위치해야 합니다.",
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
  "onboarding.fitTip1": "귀 센서 (TP9/TP10): 귀 뒤 약간 위에 밀착시키세요. 센서를 가리는 머리카락을 치워주세요.",
  "onboarding.fitTip2": "이마 센서 (AF7/AF8): 깨끗한 피부에 평평하게 밀착시키세요 — 필요시 마른 천으로 닦으세요.",
  "onboarding.fitTip3": "접촉이 좋지 않다면 센서를 젖은 손가락으로 살짝 적시세요. 전도성이 향상됩니다.",
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
  "onboarding.models.qwenDesc": "권장 채팅 모델. 대부분의 노트북에서 최적의 품질/속도 균형을 위해 Q4_K_M을 사용합니다.",
  "onboarding.models.zunaTitle": "ZUNA EEG 인코더",
  "onboarding.models.zunaDesc": "EEG 임베딩, 시맨틱 기록, 하위 뇌 상태 분석에 필요합니다.",
  "onboarding.models.neuttsTitle": "NeuTTS (Nano Q4)",
  "onboarding.models.neuttsDesc": "더 나은 품질과 음성 복제를 지원하는 권장 다국어 음성 엔진.",
  "onboarding.models.kittenTitle": "Kitten TTS",
  "onboarding.models.kittenDesc": "가벼운 고속 음성 백엔드, 빠른 대체 수단 및 저사양 시스템에 유용합니다.",
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
  "onboarding.tray.menu": "아이콘을 우클릭(Windows/Linux에서는 좌클릭)하여 빠른 작업 — 연결, 라벨, 캘리브레이션 등.",
  "onboarding.extensionsTitle": "동반 확장 프로그램",
  "onboarding.extensionsBody":
    "{app}는 편집기, 브라우저, 터미널에서 추가 컨텍스트를 가져올 수 있습니다. 각 통합은 독립적으로 설치하거나 건너뛸 수 있는 별도의 구성 요소입니다 — EEG 기능 작동에는 어느 것도 필요하지 않습니다.",
  "onboarding.extensionsPrivacy":
    "다른 모든 것과 동일한 개인정보 보호 보장: 각 확장 프로그램은 localhost 포트를 통해 로컬 데몬에 보고하며, 그 데이터는 이 컴퓨터의 activity.sqlite에 기록됩니다. NeuroSkill이나 다른 어디에도 아무것도 업로드되지 않습니다.",
  "onboarding.extensionsSkip":
    "모두 선택 사항입니다. 나중에 설정 → 확장 프로그램 및 설정 → 터미널에서 언제든 설치, 업데이트 또는 제거할 수 있습니다.",
  "onboarding.extensions.vscodeTitle": "VS Code 계열 편집기",
  "onboarding.extensions.vscodeDesc":
    "파일별 편집 추적, AI 인라인 제안, 개발 루프 통합을 추가합니다. VS Code, VSCodium, Cursor, Windsurf, Trae, Positron에서 작동 — 설치된 포크는 자동 감지됩니다.",
  "onboarding.extensions.browserTitle": "브라우저 확장 프로그램",
  "onboarding.extensions.browserDesc":
    "브라우저에서 활성 탭, 페이지 포커스 시간, 읽기 패턴을 기록합니다. Chrome, Firefox, Edge, Safari에 사이드로드 가능 (Safari는 추가 서명 단계가 필요).",
  "onboarding.extensions.terminalTitle": "터미널 / 셸 후크",
  "onboarding.extensions.terminalDesc":
    "셸에 작은 preexec/precmd 후크를 추가하여 앱이 명령 타이밍을 집중 상태와 연관시킬 수 있도록 합니다. zsh, bash, fish 또는 PowerShell 중 선택 — rc 파일에 source 한 줄만 추가하며, 나중에 완전히 제거할 수 있습니다.",

  "onboarding.permissionsTitle": "선택 가능한 활동 추적",
  "onboarding.permissionsBody":
    '{app}는 무엇에 작업하고 있었는지 기록하여 EEG/집중 데이터를 실제 맥락과 연결할 수 있습니다 — 단순히 "오후 3시에 집중력이 떨어졌다"가 아니라 "이 PR을 작성하다가 집중력을 잃었다"고 알 수 있습니다. 기본적으로 꺼져 있으며 완전히 선택 사항입니다.',
  "onboarding.permissionsPrivacy":
    "모든 것이 이 컴퓨터에만 머무릅니다. 기록된 활동은 로컬 activity.sqlite 파일에 기록되며 어떤 서버에도 — NeuroSkill에도, 다른 누구에게도 — 전송되지 않습니다. 언제든 각 옵션을 끌 수 있으며, 기록된 데이터는 삭제할 때까지 디스크에 남아 있습니다.",
  "onboarding.permissionsSkip":
    "기본적으로 모두 꺼져 있습니다. 나중에 설정 → 활동 추적에서 언제든 활성화할 수 있습니다.",
  "onboarding.permissionsActiveWindowDesc":
    "전면 앱, 창 제목, 활성 브라우저 탭, 열린 편집기 파일 경로를 캡처합니다. macOS는 각 브라우저와 편집기에 대해 손쉬운 사용 / 자동화 접근을 요청합니다.",
  "onboarding.permissionsInputDesc":
    "키보드/마우스 사용의 타임스탬프만 기록 — 어떤 키였는지, 위치, 내용은 절대 기록하지 않습니다. OS 권한이 필요 없습니다.",
  "onboarding.permissionsFileDesc":
    "Documents, Desktop, Downloads 및 자주 사용하는 개발 폴더의 생성/수정/삭제 이벤트를 감시합니다. 경로와 타임스탬프만 기록 — 파일 내용은 절대 읽지 않습니다. macOS는 전체 디스크 접근을 요청할 수 있습니다.",
  "onboarding.permissionsScreenshotsDesc":
    '화면을 일정 간격으로 캡처하고, 텍스트에 OCR을 실행하며, 둘 다 시각 검색과 "오후 3시에 화면에 무엇이 있었는지" 조회를 위해 색인화합니다. macOS는 화면 기록을 요청합니다. 간격, 품질, OCR은 설정 → 스크린샷에서 조정하세요.',
  "onboarding.permissionsLocationDesc":
    "기기 위치를 집중 블록과 함께 기록(집 vs 사무실 vs 카페)하여 장소 전환을 집중 상태와 연관시킬 수 있도록 합니다. macOS는 위치 서비스를 요청합니다. 로컬에 저장되며 절대 업로드되지 않습니다.",
  "onboarding.permissionsCalendarDesc":
    "캘린더 이벤트 메타데이터(제목, 시간, 지속 시간, 참석자 수)를 읽어 회의 밀도와 집중력 저하를 연관시킵니다. macOS는 처음 사용 시 캘린더 접근을 요청합니다. 이벤트 내용은 절대 업로드되지 않습니다.",
  "onboarding.permissionsClipboardDesc":
    "클립보드가 변경되는 시점(어떤 앱, 콘텐츠 유형, 크기)을 기록합니다. 내용 자체는 절대 읽지 않습니다. macOS 전용. 자동화 접근을 요청합니다.",
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
