---
name: apply powertoys specs
overview: PowerToys 사양 01~05를 현재 프로젝트에 맞게 선별 적용합니다. 기존 Rust/Win32 구조를 유지하면서 트레이 안정성, GIF 녹화, 스냅샷, 화면 주석 기능 순서로 확장합니다.
todos:
  - id: gif-stability
    content: "GIF 녹화 안정화: delay, 0프레임 삭제, 짝수 보정, 콤보 UI 문제 수정"
    status: completed
  - id: tray-recovery
    content: "트레이 사양 보강: Explorer 재시작 복구와 클릭 동작 분리"
    status: completed
  - id: snip-select
    content: Snip 전용 영역 선택 UI 정리
    status: pending
  - id: snip-basic
    content: Snip 전용 선택 UI를 사용해 영역 스냅샷/클립보드 복사 기능 추가
    status: pending
  - id: draw-mode
    content: ZoomIt Draw 사양 기반 화면 주석 모드 추가
    status: completed
  - id: settings-cleanup
    content: 추가된 GIF/Snip/Draw/트레이 옵션을 설정창에 정리
    status: completed
isProject: false
---

# PowerToys 사양 적용 계획

## 적용 원칙
- 현재 앱의 목표는 경량 Rust Windows 앱이므로, WinUI 3 전환은 바로 진행하지 않습니다.
- 이미 있는 `src/tray.rs`, `src/settings.rs`, `src/capture_ui.rs`, `src/recorder.rs`를 살려서 사양의 실용적인 부분부터 적용합니다.
- 우선순위는 현재 사용자 기능과 직접 연결되는 `04 Record GIF`, `01 Tray`, `05 Snip`, `03 Draw` 순서로 둡니다.

## 1단계: GIF 녹화 안정화
대상: [src/recorder.rs](src/recorder.rs), [src/capture_ui.rs](src/capture_ui.rs), [src/config.rs](src/config.rs), [src/settings.rs](src/settings.rs)

- GIF delay를 실제 프레임 간격에 맞춰 기록해 재생 속도와 녹화 시간이 어긋나는 문제를 줄입니다.
- 0프레임/취소/실패 결과 파일을 남기지 않도록 완료 처리를 정리합니다.
- GIF 출력 크기 짝수 보정 옵션을 설정에 추가합니다.
- 녹화창 콤보가 한 번 선택 후 다시 열리지 않는 UI 문제를 수정합니다.
- 현재 GDI `BitBlt` + `gif` 크레이트 구조는 유지합니다.

## 2단계: 트레이 사양 보강
대상: [src/tray.rs](src/tray.rs), [src/main.rs](src/main.rs)

- Explorer 재시작 시 트레이 아이콘이 복구되도록 `TaskbarCreated` 메시지를 처리합니다.
- 좌클릭/더블클릭/우클릭 동작을 사양처럼 분리할 수 있는 구조로 정리합니다.
- 메뉴 표시 직전 상태 갱신 흐름을 명확히 해서 녹화 중 메뉴 라벨 같은 동적 메뉴 처리를 안정화합니다.

## 3단계: Snip 전용 영역 선택 UI 정리
대상: 신규 `src/select_rect.rs` 후보

- GIF 녹화창은 기존 `src/capture_ui.rs` 전용 UI로 유지합니다.
- Snip은 한 번 드래그해서 영역을 선택하고 바로 종료되는 전용 선택 UI를 사용합니다.
- ESC 취소, 최소 크기, 선택 완료 시 `RECT` 반환 계약만 간단하게 유지합니다.

## 4단계: Snip 기능 추가
대상: 신규 `src/snip.rs`, Snip 전용 선택 모듈, [src/tray.rs](src/tray.rs)

- 선택 영역을 GDI로 캡처해 클립보드 `CF_BITMAP`에 복사하는 기본 스냅샷 기능을 추가합니다.
- 필요하면 PNG 저장은 후속 단계로 분리합니다.
- 트레이 메뉴에 `스냅샷` 또는 `영역 캡처` 항목을 추가합니다.

## 5단계: Draw 화면 주석 기능 추가
대상: 신규 `src/draw.rs`, 공용 선택/오버레이 모듈 후보, [src/tray.rs](src/tray.rs), [src/settings.rs](src/settings.rs)

- `03 ZoomIt Draw` 사양을 이 프로그램에 필요한 기능으로 적용합니다.
- 화면 위에 주석을 그릴 수 있는 오버레이 모드를 추가합니다.
- 1차 구현은 자유 곡선, 기본 색상, 펜 두께, ESC 종료를 중심으로 시작합니다.
- 이후 직선/사각형/타원/화살표, 하이라이트, 실행 취소는 단계적으로 확장합니다.
- 클릭 효과용 렌더링과 섞이지 않도록 Draw 전용 상태와 윈도우 프로시저를 분리합니다.

## 6단계: 설정창 정리
대상: [src/settings.rs](src/settings.rs), [src/config.rs](src/config.rs)

- 새로 추가되는 GIF/Snip/Draw/트레이 옵션을 기존 Win32 설정창 안에서 정리합니다.
- PowerToys 사양의 WinUI 3 전체 전환은 하지 않고, 현재 구조에서 필요한 설정만 추가합니다.

## 확인 방법
- 각 단계마다 `cargo check`를 실행합니다.
- GIF 녹화는 실제 녹화 파일 재생 시간, 빈 파일 삭제, 콤보 반복 선택을 수동 확인합니다.
- 트레이는 Explorer 재시작 후 아이콘 복구와 메뉴 동작을 확인합니다.
- Snip은 선택 영역과 클립보드 이미지 크기가 일치하고, 취소 시 아무 파일/클립보드 변경이 없는지 확인합니다.
- Draw는 자유 곡선 입력, ESC 종료, 클릭 효과와의 충돌 여부를 확인합니다.