# 클릭 효과 위치 오차 수정 문서

## 문제

마우스를 클릭한 실제 위치와 클릭 효과가 표시되는 위치 사이에 큰 오차가 발생했다.

특히 Windows 디스플레이 배율이 100%가 아닌 환경이나, 모니터별 DPI 배율이 다른 환경에서 오차가 커질 수 있다.

## 원인

전역 마우스 훅(`WH_MOUSE_LL`)으로 얻는 클릭 좌표와 오버레이 윈도우가 사용하는 화면 좌표의 DPI 기준이 서로 달라질 수 있기 때문이다.

Windows에서 프로세스가 DPI 인식 상태가 아니면 일부 좌표가 논리 좌표로 보정될 수 있고, 클릭 효과를 그리는 전체 화면 오버레이와 실제 마우스 좌표가 같은 좌표계를 사용하지 않게 된다.

## 수정 내용

앱 시작 시 Per-Monitor DPI 인식을 활성화했다.

```rust
fn enable_dpi_awareness() {
    unsafe {
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) == 0 {
            SetProcessDPIAware();
        }
    }
}
```

`main()` 진입 직후 이 함수를 호출해, 마우스 훅 좌표와 오버레이 윈도우 좌표가 같은 DPI 기준을 사용하도록 했다.

```rust
fn main() {
    enable_dpi_awareness();

    let config = AppConfig::load();
    // ...
}
```

## 기대 효과

- 클릭 위치와 원형 파장 효과의 중심 위치가 일치한다.
- Windows 배율 설정이 100%가 아니어도 위치 오차가 줄어든다.
- 모니터별 배율이 다른 다중 모니터 환경에서도 좌표 기준이 안정된다.

## 관련 파일

- `src/main.rs`
- `src/effect.rs`
