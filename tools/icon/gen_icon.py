"""앱/트레이 아이콘 생성기 — assets/icon001-1.svg 스케치를 단색 선 아이콘으로.

스케치의 9개 선(사각형, 가로선, 긴 사선, 커서 화살표 5획, 원)을 좌표 그대로 직선화해 그린다.
채움 없이 선만 사용하며 검정(밝은 작업표시줄·exe)과 흰색(어두운 작업표시줄) 두 벌을 만든다.
16~48px는 1~2px 선을 픽셀 격자에 맞춰 트레이에서 선명하게 보이도록 한다.

사용: python tools/icon/gen_icon.py
출력: assets/app-icon.png, assets/app-icon-{16,20,24,32,40,48}.png (검정)
      assets/app-icon-dark.png, assets/app-icon-dark-{16,...,48}.png (흰색)
"""

from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "assets"

BLACK = (0, 0, 0, 230)        # 시스템 트레이 아이콘 톤 (약 90%)
WHITE = (255, 255, 255, 255)
# 픽셀 격자에 맞춰 따로 그리는 크기 → 선 두께(px). 트레이: 100%=16, 125%=20, 150%=24, 200%=32
# (32는 구성이 촘촘해 2px면 위쪽 칸이 뭉쳐서 1px)
SMALL_STROKES = {16: 1.0, 20: 1.0, 24: 1.0, 32: 1.0, 40: 2.0, 48: 2.0}
SMALL_SIZES = tuple(SMALL_STROKES)

# ── 스케치 좌표 (icon001-1.svg viewBox 단위, 손떨림만 직선화) ───────────
RECT = (10.3, 10.2, 132.1, 212.4)          # 모서리 각진 사각형
LINES = [
    ((11.0, 53.5), (131.7, 57.1)),         # 가로선 (버튼 이음선)
    ((65.5, 10.7), (18.7, 211.9)),         # 긴 사선 (위 → 아래 왼쪽)
    ((56.3, 55.5), (54.5, 185.3)),         # 화살표 왼쪽 변
    ((56.4, 55.3), (130.8, 157.2)),        # 화살표 오른쪽 변
    ((54.6, 185.3), (88.1, 157.8)),        # 화살표 홈
    ((88.3, 157.7), (108.5, 211.6)),       # 화살표 꼬리 (아래 변까지)
    ((88.5, 157.7), (130.4, 157.4)),       # 화살표 날개
]
CIRCLE = ((119.9, 22.9), 6.8)              # 오른쪽 위 원


def render(px, color):
    small = px in SMALL_SIZES
    ss = 16                                # 슈퍼샘플링 배율
    n = px * ss

    # 선 두께(px): 작은 크기는 1~2px(시스템 트레이 아이콘과 동일), 큰 크기는 비례
    if small:
        stroke = SMALL_STROKES[px]
    else:
        stroke = max(2.0, px / 28)

    # 스케치 외곽(사각형)을 캔버스 높이에 맞추고 가로는 비율 유지·가운데 정렬
    x0, y0, x1, y1 = RECT
    margin = stroke / 2 + (0 if small else px * 0.03)
    scale = (px - 2 * margin) / (y1 - y0)
    offset_x = (px - (x1 - x0) * scale) / 2 - x0 * scale
    offset_y = margin - y0 * scale

    def snap(v):
        # 1px 선이 두 픽셀에 번지지 않도록 픽셀 중심(홀수 두께) / 경계(짝수)에 맞춤
        if not small:
            return v
        half = (stroke % 2) / 2
        return round(v - half) + half

    def pt(p):
        return (snap(p[0] * scale + offset_x) * ss, snap(p[1] * scale + offset_y) * ss)

    canvas = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    draw = ImageDraw.Draw(canvas)
    w = round(stroke * ss)
    r = w / 2

    def line(a, b):
        draw.line([a, b], fill=color, width=w)
        # 둥근 끝 / 이음 (스케치의 stroke-linecap=round)
        for x, y in (a, b):
            draw.ellipse([x - r, y - r, x + r, y + r], fill=color)

    # 1. 사각형 (각진 모서리)
    tl, br = pt((x0, y0)), pt((x1, y1))
    draw.rectangle([tl[0] - r, tl[1] - r, br[0] + r, br[1] + r], outline=color, width=w)

    # 2~8. 선들
    for a, b in LINES:
        line(pt(a), pt(b))

    # 9. 원 — 작은 크기에선 최소 판독 크기로 키우고 사각형·가로선 안쪽에 들어가게 보정
    (cx, cy), cr = CIRCLE
    c = pt((cx, cy))
    radius = cr * scale * ss
    # 선이 두꺼워 속이 메워지지 않게, 링으로 보일 최소 반경 확보
    radius = max(radius, w * 1.6) if not small else radius
    if small:
        radius = max(radius, stroke * ss)
    # 사각형 테두리와 붙지 않게 안쪽으로 (간격 = 선 두께 절반 이상)
    gap = max(0.5 * ss, r)
    right_limit = br[0] - r - radius - r - gap
    top_limit = tl[1] + r + radius + r + gap
    c = (min(c[0], right_limit), max(c[1], top_limit))
    draw.ellipse([c[0] - radius, c[1] - radius, c[0] + radius, c[1] + radius],
                 outline=color, width=w)

    # BOX(면적 평균): LANCZOS는 날카로운 선 옆에 회색 링잉을 남긴다
    return canvas.resize((px, px), Image.BOX)


def main():
    for suffix, color in (("", BLACK), ("-dark", WHITE)):
        render(1024, color).save(ASSETS / f"app-icon{suffix}.png")
        for px in SMALL_SIZES:
            render(px, color).save(ASSETS / f"app-icon{suffix}-{px}.png")
    print("written: assets/app-icon[-dark].png +", "/".join(map(str, SMALL_SIZES)))


if __name__ == "__main__":
    main()
