# 기기 내 음성 안내 (TTS)

## 기기 내 음성 안내 (TTS)
NeuroSkill™에는 완전한 기기 내 영어 음성 합성 엔진이 포함되어 있습니다. 캘리브레이션 단계를 음성으로 안내하며(동작 라벨, 휴식, 완료), WebSocket 또는 HTTP API를 통해 모든 스크립트에서 원격으로 트리거할 수 있습니다. 모든 합성은 로컬에서 실행됩니다 — ~30 MB 모델을 한 번 다운로드한 후에는 인터넷이 필요하지 않습니다.

## 작동 방식
텍스트 전처리 → 문장 분할(≤400자) → libespeak-ng를 통한 음소화(C 라이브러리, 인프로세스, en-us 음성) → 토큰화(IPA → 정수 ID) → ONNX 추론(KittenTTS 모델: input_ids + style + speed → f32 파형) → 1초 무음 패딩 → rodio가 시스템 기본 오디오 출력에서 재생.

## 모델
HuggingFace Hub의 KittenML/kitten-tts-mini-0.8. 음성: Jasper(English en-us). 샘플 레이트: 24,000 Hz 모노 float32. INT8 ONNX 양자화 — CPU 전용, GPU 불필요. 첫 다운로드 후 ~/.cache/huggingface/hub/에 캐시됩니다.

## 요구 사항
espeak-ng가 설치되어 PATH에 있어야 합니다 — 인프로세스 IPA 음소화를 제공합니다(C 라이브러리로 링크, 서브프로세스로 실행되지 않음). macOS: brew install espeak-ng. Ubuntu/Debian: apt install libespeak-ng-dev. Alpine: apk add espeak-ng-dev. Fedora: dnf install espeak-ng-devel.

## 캘리브레이션 통합
캘리브레이션 세션이 시작되면 엔진이 백그라운드에서 사전 워밍됩니다(필요한 경우 모델 다운로드). 각 단계에서 캘리브레이션 창이 동작 라벨, 휴식 안내, 완료 메시지, 취소 알림으로 tts_speak를 호출합니다. 음성은 캘리브레이션을 차단하지 않습니다 — 모든 TTS 호출은 발사 후 잊기(fire-and-forget) 방식입니다.

## API — say 명령어
외부 스크립트, 자동화 도구, 또는 LLM 에이전트에서 음성을 트리거합니다. 명령어는 오디오가 재생되는 동안 즉시 반환됩니다. WebSocket: {"command":"say","text":"메시지 내용"}. HTTP: POST /say, 본문 {"text":"메시지 내용"}. CLI(curl): curl -X POST http://localhost:<port>/say -d '{"text":"hello"}' -H 'Content-Type: application/json'.

## 디버그 로깅
설정 → 음성에서 TTS 합성 로깅을 활성화하면 이벤트(발화 텍스트, 샘플 수, 추론 지연)가 NeuroSkill™ 로그 파일에 기록됩니다. 지연 측정 및 문제 진단에 유용합니다.

## 여기서 테스트하기
아래 위젯을 사용하여 이 도움말 창에서 직접 TTS 엔진을 테스트할 수 있습니다.
