// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** Korean "perm" namespace. */
const perm: Record<string, string> = {
  "perm.intro":
    "{app}은(는) 키보드/마우스 활동 타임스탬프 및 알림과 같은 기능을 위해 소수의 선택적 OS 권한을 사용합니다. 모든 데이터는 기기에 저장됩니다.",
  "perm.granted": "허용됨",
  "perm.denied": "허용되지 않음",
  "perm.unknown": "알 수 없음",
  "perm.notRequired": "필요 없음",
  "perm.systemManaged": "OS에서 관리",
  "perm.accessibility": "손쉬운 사용",
  "perm.accessibilityDesc":
    "키보드 및 마우스 활동 추적은 macOS의 CGEventTap을 사용하여 마지막 키 입력과 마우스 이벤트의 타임스탬프를 기록합니다. 키 입력이나 커서 위치는 저장되지 않으며 Unix 초 타임스탬프만 저장됩니다. macOS에서는 손쉬운 사용 권한이 필요합니다.",
  "perm.accessibilityOk": "권한이 허용되었습니다. 키보드 및 마우스 활동 타임스탬프가 기록되고 있습니다.",
  "perm.accessibilityPending": "권한 상태 확인 중…",
  "perm.howToGrant": "이 권한을 허용하는 방법:",
  "perm.accessStep1": '아래의 "손쉬운 사용 설정 열기"를 클릭하세요.',
  "perm.accessStep2": "목록에서 {app}을(를) 찾으세요 (또는 + 버튼으로 추가).",
  "perm.accessStep3": "활성화하세요.",
  "perm.accessStep4": "여기로 돌아오면 상태가 자동 업데이트됩니다.",
  "perm.openAccessibilitySettings": "손쉬운 사용 설정 열기",
  "perm.bluetooth": "Bluetooth",
  "perm.bluetoothDesc":
    "Bluetooth는 BCI 헤드셋(Muse, MW75 Neuro, OpenBCI Ganglion, IDUN Guardian 등)에 연결하는 데 사용됩니다. macOS에서는 앱이 처음 스캔할 때 시스템이 Bluetooth 접근을 요청합니다. Linux와 Windows에서는 별도 권한이 필요하지 않습니다.",
  "perm.openBluetoothSettings": "Bluetooth 설정 열기",
  "perm.notifications": "알림",
  "perm.notificationsDesc":
    "알림은 일일 녹화 목표 달성 시, 그리고 소프트웨어 업데이트가 있을 때 알려드리는 데 사용됩니다. macOS와 Windows에서는 첫 알림 전송 시 OS가 권한을 요청합니다.",
  "perm.openNotificationsSettings": "알림 설정 열기",
  "perm.matrix": "권한 요약",
  "perm.feature": "기능",
  "perm.matrixBluetooth": "Bluetooth (BCI 기기)",
  "perm.matrixKeyboardMouse": "키보드 & 마우스 타임스탬프",
  "perm.matrixActiveWindow": "활성 창 추적",
  "perm.matrixNotifications": "알림",
  "perm.matrixNone": "권한 불필요",
  "perm.matrixAccessibility": "손쉬운 사용 필요",
  "perm.matrixOsPrompt": "첫 사용 시 OS가 요청",
  "perm.legendNone": "권한 불필요",
  "perm.legendRequired": "OS 권한 필요 — 없으면 조용히 기능 저하",
  "perm.legendPrompt": "첫 사용 시 OS가 요청",
  "perm.why": "{app}에 이 권한이 필요한 이유는?",
  "perm.whyBluetooth": "Bluetooth",
  "perm.whyBluetoothDesc": "BLE를 통해 BCI 헤드셋을 검색하고 데이터를 스트리밍합니다.",
  "perm.whyAccessibility": "손쉬운 사용",
  "perm.whyAccessibilityDesc":
    "활동 컨텍스트를 위한 키보드 및 마우스 이벤트 타임스탬프. 이벤트 시간만 저장되며 입력 내용이나 커서 위치는 절대 저장되지 않습니다.",
  "perm.whyNotifications": "알림",
  "perm.whyNotificationsDesc": "일일 녹화 목표 달성 및 업데이트 준비 시 알림.",
  "perm.privacyNote":
    "모든 데이터는 기기에 로컬로 저장되며 어떤 서버에도 전송되지 않습니다. 설정 → 활동 추적에서 기능을 비활성화할 수 있습니다.",
  "perm.screenRecording": "화면 녹화",
  "perm.screenRecordingDesc":
    "스크린샷 임베딩 시스템에서 다른 앱의 창을 캡처하는 데 필요합니다. macOS는 이 권한 없이 창 콘텐츠를 가립니다.",
  "perm.screenRecordingOk": "화면 녹화 권한이 허용되었습니다. 스크린샷 캡처가 정상 작동합니다.",
  "perm.screenRecordingStep1": "시스템 설정 → 개인정보 보호 및 보안 → 화면 및 시스템 오디오 녹음을 여세요",
  "perm.screenRecordingStep2": "목록에서 NeuroSkill™을 찾아 활성화하세요",
  "perm.screenRecordingStep3": "변경 사항을 적용하려면 앱을 종료하고 다시 실행해야 할 수 있습니다",
  "perm.openScreenRecordingSettings": "화면 녹화 설정 열기",
  "perm.whyScreenRecording": "화면 녹화",
  "perm.whyScreenRecordingDesc":
    "시각적 유사성 검색과 크로스모달 EEG 상관 분석을 위해 활성 창을 캡처합니다. 선택한 스크린샷만 저장됩니다 — 연속 녹화는 하지 않습니다.",
  "perm.matrixScreenRecording": "스크린샷 캡처",
  "perm.matrixScreenRecordingReq": "화면 녹화 필요",
  "perm.calendar": "캘린더",
  "perm.calendarDesc":
    "캘린더 도구는 일정 컨텍스트를 위해 이벤트를 읽을 수 있습니다. 필요 시 macOS가 권한을 요청합니다.",
  "perm.requestCalendarPermission": "캘린더 권한 요청",
  "perm.openCalendarSettings": "캘린더 개인정보 설정 열기",
  "perm.location": "위치 서비스",
  "perm.locationDesc":
    "macOS에서는 위치 서비스가 CoreLocation(GPS / Wi-Fi / 셀룰러)을 사용하여 고정밀 위치를 제공합니다. Linux와 Windows에서는 권한이 필요 없는 IP 기반 지오로케이션을 사용합니다. 위치 서비스가 거부되거나 사용할 수 없으면 자동으로 IP 지오로케이션으로 대체됩니다.",
  "perm.locationOk": "위치 권한이 허용되었습니다. CoreLocation이 고정밀 위치에 사용됩니다.",
  "perm.locationFallback": "위치가 허가되지 않음 — 앱이 IP 기반 지오로케이션(도시 수준 정확도)을 사용합니다.",
  "perm.locationStep1": "시스템 설정 → 개인정보 보호 및 보안 → 위치 서비스를 여세요",
  "perm.locationStep2": "목록에서 {app}을(를) 찾아 활성화하세요",
  "perm.locationStep3": "여기로 돌아오면 상태가 자동 업데이트됩니다",
  "perm.requestLocationPermission": "위치 권한 요청",
  "perm.openLocationSettings": "위치 설정 열기",
  "perm.whyLocation": "위치",
  "perm.whyLocationDesc":
    "LLM에 정밀한 위치 컨텍스트를 제공하고 건강 데이터와 함께 GPS 정보를 저장합니다. 거부되면 IP 지오로케이션으로 대체됩니다.",
  "perm.matrixLocation": "위치 (GPS / IP)",
  "perm.matrixLocationReq": "위치 서비스 (선택적 — IP로 대체)",
  "perm.openInputMonitoringSettings": "입력 모니터링 설정 열기",
  "perm.openFocusSettings": "집중 모드 설정 열기",
  "perm.fullDiskAccess": "전체 디스크 접근",
  "perm.fullDiskAccessDesc":
    "시스템 데이터베이스를 통한 직접 집중 모드 감지에 필요합니다. 없으면 더 느린 레거시 방식으로 대체됩니다. 안정적인 방해 금지 통합을 위해 권장됩니다.",
  "perm.fullDiskAccessStep1": "시스템 설정 → 개인정보 보호 및 보안 → 전체 디스크 접근을 여세요",
  "perm.fullDiskAccessStep2": "목록에서 NeuroSkill™ (또는 데몬을 실행하는 터미널)을 찾아 활성화하세요",
  "perm.fullDiskAccessStep3": "변경 사항을 적용하려면 앱을 종료하고 다시 실행해야 할 수 있습니다",
  "perm.openFullDiskAccessSettings": "전체 디스크 접근 설정 열기",
  "perm.whyCalendar": "캘린더",
  "perm.whyCalendarDesc":
    "AI가 예정된 일정을 참조할 수 있도록 LLM 도구에 일정 컨텍스트를 제공합니다.",
  "perm.matrixCalendar": "캘린더 이벤트",
  "perm.matrixCalendarReq": "캘린더 접근 필요",
};

export default perm;
