// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** Korean "screenshots" namespace. */
const screenshots: Record<string, string> = {
  "screenshots.title": "스크린샷 캡처",
  "screenshots.enableToggle": "스크린샷 캡처 활성화",
  "screenshots.enableDesc":
    "활성 창을 주기적으로 캡처하고 비전 모델로 임베딩하여 시각적 유사성 검색을 수행합니다.",
  "screenshots.sessionOnlyToggle": "세션 전용",
  "screenshots.sessionOnlyDesc": "활성 EEG 녹화 세션 중에만 캡처합니다.",
  "screenshots.interval": "캡처 간격",
  "screenshots.intervalDesc":
    "EEG 임베딩 에폭(각 5초)에 맞춰 정렬됩니다. 1× = 매 에폭, 2× = 격 에폭, 최대 12× (60초).",
  "screenshots.intervalUnit": "초",
  "screenshots.intervalEpoch": "에폭",
  "screenshots.imageSize": "이미지 크기",
  "screenshots.imageSizeDesc":
    "중간 해상도(px). 캡처된 창이 저장 및 임베딩 전에 이 정사각형에 맞게 크기 조정됩니다.",
  "screenshots.imageSizeUnit": "px",
  "screenshots.imageSizeRecommended": "현재 모델 권장:",
  "screenshots.quality": "WebP 품질",
  "screenshots.qualityDesc": "WebP 압축 품질(0–100). 낮을수록 파일이 작아집니다.",
  "screenshots.embeddingModel": "임베딩 모델",
  "screenshots.embeddingModelDesc": "유사성 검색을 위한 이미지 임베딩 생성에 사용되는 비전 모델.",
  "screenshots.backendFastembed": "fastembed (로컬 ONNX)",
  "screenshots.backendMmproj": "mmproj (LLM 비전 프로젝터)",
  "screenshots.backendLlmVlm": "LLM VLM (비전 모델로 임베딩 + OCR)",
  "screenshots.modelClip": "CLIP ViT-B/32 — 512d (빠름, 기본값)",
  "screenshots.modelNomic": "Nomic Embed Vision v1.5 — 768d",
  "screenshots.reembed": "스크린샷 재임베딩",
  "screenshots.reembedDesc": "현재 모델을 사용하여 모든 기존 스크린샷의 임베딩을 다시 계산합니다.",
  "screenshots.reembedBtn": "재임베딩 & 재인덱싱",
  "screenshots.reembedNowBtn": "지금 재임베딩",
  "screenshots.reembedding": "임베딩 중…",
  "screenshots.stale": "오래됨",
  "screenshots.unembedded": "미임베딩",
  "screenshots.estimate": "예상 시간:",
  "screenshots.modelChanged": "임베딩 모델 변경됨",
  "screenshots.modelChangedDesc":
    "스크린샷이 다른 모델로 임베딩되었습니다. 일관된 검색 결과를 위해 재임베딩하세요.",
  "screenshots.privacyNote":
    "모든 스크린샷은 로컬에만 저장되며 전송되지 않습니다. 기본적으로 옵트인, 세션 제한입니다.",
  "screenshots.storagePath": "저장 경로: ~/.skill/screenshots/",
  "screenshots.permissionRequired": "화면 녹화 권한 필요",
  "screenshots.permissionDesc":
    "macOS에서는 다른 앱의 창을 캡처하려면 화면 및 시스템 오디오 녹음 권한이 필요합니다. 이 권한이 없으면 스크린샷이 빈 화면이거나 자체 앱만 표시될 수 있습니다.",
  "screenshots.permissionGranted": "화면 녹화 권한이 허용되었습니다.",
  "screenshots.openPermissionSettings": "화면 녹화 설정 열기",
  "screenshots.ocrToggle": "OCR 텍스트 추출",
  "screenshots.ocrToggleDesc":
    "텍스트 기반 검색을 위해 스크린샷에서 텍스트를 추출합니다. 다운사이징 전에 전체 해상도 이미지에서 실행됩니다.",
  "screenshots.gpuToggle": "GPU 가속",
  "screenshots.gpuToggleDesc":
    "이미지 임베딩과 OCR에 GPU를 사용합니다. CPU 추론을 강제하려면 비활성화하세요(LLM/EEG를 위해 GPU 확보).",
  "screenshots.ocrEngineSelect": "OCR 엔진",
  "screenshots.ocrEngineAppleVision": "Apple Vision — GPU / 뉴럴 엔진 (macOS 권장)",
  "screenshots.ocrEngineOcrs": "ocrs — 로컬 rten 기반 CPU (크로스 플랫폼)",
  "screenshots.ocrAppleVisionHint": "⚡ Apple Vision은 GPU/ANE에서 실행되며 macOS에서 ocrs보다 ~10배 빠릅니다",
  "screenshots.ocrActiveModels": "활성 모델",
  "screenshots.ocrInference": "추론",
  "screenshots.ocrTitle": "OCR 텍스트 추출",
  "screenshots.ocrEngine": "온디바이스 OCR",
  "screenshots.ocrDesc":
    "ocrs 엔진을 사용하여 다운사이징 전 전체 해상도에서 각 스크린샷의 텍스트를 추출합니다. 추출된 텍스트는 BGE-Small-EN-v1.5로 임베딩되고 시맨틱 텍스트 검색을 위해 별도의 HNSW 인덱스에 인덱싱됩니다. OCR 모델(각 ~10 MB)은 첫 사용 시 자동 다운로드됩니다.",
  "screenshots.ocrDetModel": "감지 모델",
  "screenshots.ocrRecModel": "인식 모델",
  "screenshots.ocrTextEmbed": "텍스트 임베딩",
  "screenshots.ocrIndex": "텍스트 인덱스",
  "screenshots.ocrSearchHint": "검색 창 → 이미지 탭에서 스크린샷 텍스트를 검색하세요.",
  "screenshots.ocrSearchTitle": "화면 텍스트로 검색",
  "screenshots.ocrSearchPlaceholder": "스크린샷에 보이는 텍스트를 검색…",
  "screenshots.ocrSearchBtn": "검색",
  "screenshots.ocrModeSubstring": "텍스트 일치",
  "screenshots.ocrModeSemantic": "시맨틱",
  "screenshots.ocrNoResults": "일치하는 스크린샷이 없습니다.",
  "screenshots.perfTitle": "파이프라인 성능",
  "screenshots.perfCapture": "캡처 스레드",
  "screenshots.perfEmbed": "임베딩 스레드",
  "screenshots.perfTotal": "합계",
  "screenshots.perfWindowCapture": "창 캡처",
  "screenshots.perfOcr": "OCR 추출",
  "screenshots.perfResize": "크기 조정 + 패딩",
  "screenshots.perfSave": "저장 + SQLite",
  "screenshots.perfIterTotal": "반복 합계",
  "screenshots.perfVisionEmbed": "비전 임베딩",
  "screenshots.perfTextEmbed": "텍스트 임베딩",
  "screenshots.perfQueue": "큐 깊이",
  "screenshots.perfDrops": "드롭",
  "screenshots.perfBackoff": "백오프",
  "screenshots.perfDropsHint": "임베딩 스레드가 느림 — 간격 자동 증가, 큐가 비면 복구됩니다",
  "screenshots.perfErrors": "오류",
  "screenshots.stats": "통계",
  "screenshots.totalCount": "총 스크린샷",
  "screenshots.embeddedCount": "임베딩됨",
  "screenshots.unembeddedCount": "미임베딩",
  "screenshots.staleCount": "오래됨 (다른 모델)",
};

export default screenshots;
