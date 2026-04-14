// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** Korean "calibration" namespace. */
const calibration: Record<string, string> = {
  "calibration.title": "캘리브레이션",
  "calibration.profiles": "캘리브레이션 프로필",
  "calibration.newProfile": "새 프로필",
  "calibration.editProfile": "프로필 편집",
  "calibration.profileName": "프로필 이름",
  "calibration.profileNamePlaceholder": "예: 눈 뜨기 / 눈 감기",
  "calibration.addAction": "동작 추가",
  "calibration.actionLabel": "동작 이름…",
  "calibration.breakLabel": "휴식",
  "calibration.selectProfile": "프로필",
  "calibration.moveUp": "위로 이동",
  "calibration.moveDown": "아래로 이동",
  "calibration.removeAction": "동작 제거",
  "calibration.descriptionN": "이 프로토콜은 {actions}을(를) <strong>{count}</strong>회 반복합니다.",
  "calibration.timingDescN": "{loops}회 반복 · {actions}개 동작 · 각 동작 사이 {breakSecs}초 휴식",
  "calibration.notifActionBody": "{total}회 중 {loop}회",
  "calibration.notifBreakBody": "다음: {next}",
  "calibration.notifDoneBody": "총 {n}회 완료되었습니다.",
  "calibration.recording": "● 녹화 중",
  "calibration.neverCalibrated": "캘리브레이션한 적 없음",
  "calibration.lastAgo": "마지막: {ago}",
  "calibration.eegCalibration": "EEG 캘리브레이션",
  "calibration.description":
    '이 작업은 <strong class="text-blue-600 dark:text-blue-400">{action1}</strong>과(와) <strong class="text-violet-600 dark:text-violet-400">{action2}</strong>를 휴식을 포함하여 <strong>{count}</strong>회 반복합니다.',
  "calibration.timingDesc": "각 동작은 {actionSecs}초, 휴식은 {breakSecs}초입니다. 라벨은 자동 저장됩니다.",
  "calibration.startCalibration": "캘리브레이션 시작",
  "calibration.complete": "캘리브레이션 완료",
  "calibration.completeDesc": "총 {n}회 반복이 완료되었습니다. 각 동작 단계의 라벨이 저장되었습니다.",
  "calibration.runAgain": "다시 실행",
  "calibration.iteration": "반복",
  "calibration.break": "휴식",
  "calibration.nextAction": "다음: {action}",
  "calibration.secondsRemaining": "초 남음",
  "calibration.ready": "준비 완료",
  "calibration.lastCalibrated": "마지막 캘리브레이션",
  "calibration.lastAtAgo": "마지막: {date} ({ago})",
  "calibration.noPrevious": "이전 캘리브레이션 기록이 없습니다",
  "calibration.footer": "Esc로 닫기 · WebSocket으로 이벤트 전송",
  "calibration.presets": "빠른 프리셋",
  "calibration.presetsDesc":
    "목적, 연령, 사용 사례에 맞는 캘리브레이션 구성을 선택하세요. 아래에서 설정을 추가로 조정할 수 있습니다.",
  "calibration.applyPreset": "적용",
  "calibration.orCustom": "또는 수동 설정:",
  "calibration.preset.baseline": "눈 뜨기 / 눈 감기",
  "calibration.preset.baselineDesc":
    "기본 베이스라인: 눈을 뜬 상태와 감은 상태의 휴식 비교. 초보자와 첫 캘리브레이션에 적합합니다.",
  "calibration.preset.focus": "집중 / 이완",
  "calibration.preset.focusDesc": "뉴로피드백: 암산 vs. 차분한 호흡. 일반 용도입니다.",
  "calibration.preset.meditation": "명상",
  "calibration.preset.meditationDesc": "능동적 사고 vs. 마음챙김 명상. 명상 수련자에게 적합합니다.",
  "calibration.preset.sleep": "수면 전 / 졸림",
  "calibration.preset.sleepDesc": "각성 상태 vs. 졸림. 수면 연구 및 이완 추적에 적합합니다.",
  "calibration.preset.gaming": "게임 / 퍼포먼스",
  "calibration.preset.gamingDesc": "고강도 작업 vs. 수동적 휴식. e스포츠 및 최고 성능 바이오피드백에 적합합니다.",
  "calibration.preset.children": "어린이 / 짧은 집중",
  "calibration.preset.childrenDesc": "어린이 또는 집중 지속 시간이 짧은 사용자를 위한 짧은 단계(10초)입니다.",
  "calibration.preset.clinical": "임상 / 연구용",
  "calibration.preset.clinicalDesc": "연구 또는 임상 베이스라인을 위한 긴 동작 단계의 5회 반복 프로토콜입니다.",
  "calibration.preset.stress": "스트레스 / 불안",
  "calibration.preset.stressDesc": "안정된 휴식 vs. 가벼운 인지 스트레스. 불안 및 스트레스 반응 추적에 적합합니다.",
};

export default calibration;
