# 개요

## 실시간 스트리밍
{app}은 로컬 WebSocket 서버를 통해 파생된 EEG 지표와 디바이스 상태를 실시간으로 스트리밍합니다. 브로드캐스트 이벤트에는 eeg-bands(~4 Hz — 60개 이상의 점수), device-status(~1 Hz — 배터리, 연결 상태), label-created가 포함됩니다. 원시 EEG/PPG/IMU 샘플은 WebSocket API를 통해 제공되지 않습니다. 서비스는 Bonjour/mDNS를 통해 _skill._tcp으로 광고되므로 클라이언트가 자동으로 검색할 수 있습니다.

## 명령어
클라이언트는 WebSocket을 통해 JSON 명령어를 전송할 수 있습니다: status(전체 시스템 스냅샷), calibrate(캘리브레이션 열기), label(주석 제출), search(최근접 이웃 쿼리), sessions(녹화 목록), compare(A/B 지표 + 수면 + UMAP), sleep(수면 단계 분류), umap/umap_poll(3D 임베딩 프로젝션). 응답은 동일한 연결에서 "ok" 불리언이 포함된 JSON으로 전달됩니다.

# 명령어 레퍼런스

## status
_(없음)_

디바이스 상태, 세션 정보, 임베딩 수(오늘 및 전체), 라벨 수, 마지막 캘리브레이션 타임스탬프, 채널별 신호 품질을 반환합니다.

## calibrate
_(없음)_

캘리브레이션 창을 엽니다. 연결되어 스트리밍 중인 디바이스가 필요합니다.

## label
text (string, 필수); label_start_utc (u64, 선택 — 기본값: 현재 시간)

타임스탬프가 지정된 라벨을 라벨 데이터베이스에 삽입합니다. 새로운 label_id를 반환합니다.

## search
start_utc, end_utc (u64, 필수); k, ef (u64, 선택)

주어진 시간 범위 내에서 HNSW 임베딩 인덱스의 k개 최근접 이웃을 검색합니다.

## compare
a_start_utc, a_end_utc, b_start_utc, b_end_utc (u64, 필수)

두 시간 범위를 비교하여 각 범위에 대한 집계된 대역 전력 지표(상대 전력, 이완/집중 점수, FAA)를 반환합니다. { a: SessionMetrics, b: SessionMetrics }를 반환합니다.

## sessions
_(없음)_

일일 eeg.sqlite 데이터베이스에서 발견된 모든 임베딩 세션을 나열합니다. 세션은 연속적인 녹화 범위입니다(갭 > 2분 = 새 세션). 최신순으로 반환됩니다.

## sleep
start_utc, end_utc (u64, 필수)

시간 범위 내의 각 임베딩 에포크를 대역 전력 비율을 사용하여 수면 단계(Wake/N1/N2/N3/REM)로 분류하고, 단계별 요약이 포함된 수면 다이어그램을 반환합니다.

## umap
a_start_utc, a_end_utc, b_start_utc, b_end_utc (u64, 필수)

두 세션의 임베딩에 대한 3D UMAP 프로젝션을 대기열에 추가합니다. 폴링용 job_id를 반환합니다. 비차단 방식입니다.

## umap_poll
job_id (string, 필수)

이전에 대기열에 추가된 UMAP 작업의 결과를 폴링합니다. { status: 'pending' | 'done', points?: [...] }를 반환합니다.

## say
text: string (필수)

디바이스 내 TTS를 통해 텍스트를 음성으로 출력합니다. 발사 후 잊기(fire-and-forget) 방식으로, 오디오가 백그라운드에서 재생되는 동안 즉시 반환됩니다. 첫 번째 호출 시 TTS 엔진을 초기화합니다.
