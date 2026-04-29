// SPDX-License-Identifier: GPL-3.0-only
/** KO — "validation" namespace. */
const validation: Record<string, string> = {
  "settingsTabs.validation": "검증",
  "validation.title": "검증 및 연구",
  "validation.intro":
    "외부 측정과 비교하여 휴식 코치 및 집중 점수를 보정하는 옵트인 연구 도구. NeuroSkill 사용에는 필수가 아닙니다.",
  "validation.disclaimer":
    "연구용 도구일 뿐 — 의료기기가 아님. FDA, CE 또는 어떤 규제기관의 승인을 받지 않음. 임상용으로 사용 불가.",

  "validation.master.title": "전역 게이트",
  "validation.master.respectFlow": "몰입 상태 존중",
  "validation.master.respectFlowDesc":
    "몰입에 들어가면 아래의 모든 알림이 억제됩니다. 기본적으로 활성화 — 그대로 두세요.",
  "validation.master.quietBefore": "조용한 시간 시작",
  "validation.master.quietAfter": "조용한 시간 종료",
  "validation.master.quietDesc":
    "로컬 시간. 이 창 외부에서는 알림이 발생하지 않습니다. 시작 = 종료 시 조용한 시간 전체 비활성화.",

  "validation.kss.title": "카롤린스카 졸음 척도 (KSS)",
  "validation.kss.desc": "순간 졸음에 대한 5초 자기 보고 (1-9). 주관적 상태 대비 휴식 코치 보정에 사용.",
  "validation.kss.enabled": "KSS 알림 활성화",
  "validation.kss.maxPerDay": "하루 최대 알림 수",
  "validation.kss.minInterval": "알림 간 최소 분",
  "validation.kss.triggerBreakCoach": "휴식 코치가 피로 감지 시 발동",
  "validation.kss.triggerRandom": "가끔 무작위 대조 샘플 발동",
  "validation.kss.triggerRandomDesc": "ROC/AUC 계산에 필요 — 음성이 없으면 피로 양성 케이스만 보입니다.",
  "validation.kss.randomWeight": "무작위 샘플 가중치 (0-1)",

  "validation.tlx.title": "NASA-TLX (작업 부하, 원시 6 스케일)",
  "validation.tlx.desc": "작업 단위 후 60초 6 하위 스케일 작업 부하 자기 보고. 부하를 측정 — KSS 졸음을 보완.",
  "validation.tlx.enabled": "NASA-TLX 알림 활성화",
  "validation.tlx.maxPerDay": "하루 최대 알림 수",
  "validation.tlx.minTaskMin": "묻기 최소 작업 길이 (분)",
  "validation.tlx.endOfDay": "하루 종료 작업 부하 요약도 발동",

  "validation.tlx.form.title": "방금 끝난 작업을 평가하세요",
  "validation.tlx.mental": "정신적 요구",
  "validation.tlx.physical": "신체적 요구",
  "validation.tlx.temporal": "시간적 요구",
  "validation.tlx.performance": "수행도",
  "validation.tlx.effort": "노력",
  "validation.tlx.frustration": "좌절",

  "validation.pvt.title": "정신운동 각성 과제 (PVT)",
  "validation.pvt.desc": "3분 반응 시간 과제. 객관적 각성 측정 — 수집은 느리지만 문헌에서 가장 강한 신호.",
  "validation.pvt.enabled": "주간 PVT 알림 활성화",
  "validation.pvt.weeklyReminder": "이번 주 PVT가 없을 때 한 줄 알림 표시",
  "validation.pvt.runNow": "PVT 지금 실행 (3분)",
  "validation.pvt.task.start": "시작",
  "validation.pvt.task.cancel": "취소",
  "validation.pvt.task.close": "닫기",

  "validation.eeg.title": "EEG 피로 지수 (Jap et al. 2009)",
  "validation.eeg.desc":
    "NeuroSkill 헤드셋이 연결된 경우 대역 전력 스트림에서 지속적으로 계산됩니다. 공식: (α + θ) / β. 수동적 — 비용 없음.",
  "validation.eeg.enabled": "EEG 피로 지수 계산",
  "validation.eeg.windowSecs": "롤링 윈도우 (초)",
  "validation.eeg.current": "현재 값",
  "validation.eeg.noHeadset": "EEG 헤드셋이 스트리밍되지 않음",

  "validation.calibrationWeek.title": "보정 주간",
  "validation.calibrationWeek.desc":
    "고빈도 샘플링의 옵트인 7일 버스트. KSS를 하루 8회로 증가, 20분 이상의 몰입 블록마다 TLX 발동, 주중 PVT 1회 요청. 8일째 일반 설정으로 자동 복귀.",
  "validation.calibrationWeek.start": "보정 주간 시작",

  "validation.results.title": "최근 결과",
  "validation.save.saved": "저장됨",
};
export default validation;
