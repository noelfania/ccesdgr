"""앱/트레이 아이콘 생성기 (스케치 icon001-1.svg 기반).

구성: 마우스 몸체(대각선 분할 2톤) + 버튼 이음선 + 커서 화살표 + 빨간 클릭 점(+리플).
큰 크기(>=32)는 디테일 포함, 작은 크기(16~24)는 트레이용으로 단순화·굵게 그린다.

사용: python tools/icon/gen_icon.py
출력: assets/app-icon.png (1024 마스터), assets/app-icon-{16,20,24}.png (소형 전용)
"""

from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "assets"

# ── 색 ────────────────────────────────────────────────────────────────────
BODY_TOP = (66, 133, 255)      # 몸체 그라데이션 위
BODY_BOTTOM = (29, 78, 216)    # 몸체 그라데이션 아래
FACET_LIGHT = (255, 255, 255, 34)  # 대각선 왼쪽 면 하이라이트
SEAM = (16, 42, 120, 150)      # 버튼 이음선
OUTLINE = (12, 28, 74, 255)    # 화살표 외곽선 / 몸체 테두리
ARROW = (255, 255, 255, 255)
DOT = (255, 71, 87, 255)       # 클릭/녹화 점
DOT_RING = (255, 255, 255, 255)

# ── 기하 (0~1 정규화, 정사각 캔버스 기준) ───────────────────────────────
BODY = (0.19, 0.045, 0.81, 0.955)   # left, top, right, bottom
BODY_RADIUS = 0.29                  # 캔버스 대비 모서리 반경
SEAM_Y = 0.255
# 스케치의 대각선: 위쪽 중앙 → 아래 왼쪽
DIAG_TOP = (0.475, BODY[1])
DIAG_BOTTOM = (0.27, BODY[3])
# 커서 화살표 (팁 기준, 높이 1 정규화)
ARROW_SHAPE = [
    (0.00, 0.00),
    (0.00, 0.80),
    (0.185, 0.635),
    (0.305, 0.925),
    (0.415, 0.88),
    (0.295, 0.60),
    (0.535, 0.60),
]
DOT_CENTER = (0.645, 0.15)


def lerp(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def body_mask(size, box, radius):
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle(box, radius=radius, fill=255)
    return mask


def vertical_gradient(size, top, bottom, y0, y1):
    grad = Image.new("RGBA", (size, size))
    draw = ImageDraw.Draw(grad)
    for y in range(size):
        t = min(max((y - y0) / max(y1 - y0, 1), 0.0), 1.0)
        draw.line([(0, y), (size, y)], fill=lerp(top, bottom, t) + (255,))
    return grad


def render(px, detail):
    """px 크기 아이콘. detail=False면 트레이용 단순화 버전."""
    ss = 16 if px <= 32 else 4          # 슈퍼샘플링 배율
    n = px * ss
    s = lambda v: v * n                 # 정규화 → 픽셀

    # 작은 크기는 몸체를 꽉 채워 존재감을 키운다
    body = BODY if detail else (0.15, 0.02, 0.85, 0.98)
    box = [s(body[0]), s(body[1]), s(body[2]), s(body[3])]
    radius = s(BODY_RADIUS if detail else 0.27)

    canvas = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    mask = body_mask(n, box, radius)

    # 그림자 (큰 크기만)
    if detail:
        shadow = Image.new("RGBA", (n, n), (0, 0, 0, 0))
        shadow_mask = mask.filter(ImageFilter.GaussianBlur(s(0.018)))
        shadow.putalpha(shadow_mask.point(lambda a: a * 70 // 255))
        canvas.alpha_composite(shadow, (0, round(s(0.012))))

    # 몸체 그라데이션
    grad = vertical_gradient(n, BODY_TOP, BODY_BOTTOM, box[1], box[3])
    body_layer = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    body_layer.paste(grad, (0, 0), mask)

    # 대각선 분할 — 왼쪽 면을 밝게 (스케치의 사선)
    facet = Image.new("L", (n, n), 0)
    dt = (s(DIAG_TOP[0]) if detail else s(0.47), box[1])
    db = (s(DIAG_BOTTOM[0]) if detail else s(0.24), box[3])
    ImageDraw.Draw(facet).polygon([(0, 0), dt, db, (0, n)], fill=255)
    facet = ImageChops.multiply(facet, mask)
    light = Image.new("RGBA", (n, n), FACET_LIGHT)
    body_layer.alpha_composite(Image.composite(light, Image.new("RGBA", (n, n)), facet))

    # 상단 광택 (큰 크기만)
    if detail:
        gloss = Image.new("L", (n, n), 0)
        ImageDraw.Draw(gloss).rounded_rectangle(
            [box[0] + s(0.02), box[1] + s(0.012), box[2] - s(0.02), box[1] + s(0.20)],
            radius=radius * 0.85,
            fill=60,
        )
        gloss = ImageChops.multiply(gloss.filter(ImageFilter.GaussianBlur(s(0.02))), mask)
        white = Image.new("RGBA", (n, n), (255, 255, 255, 255))
        white.putalpha(gloss)
        body_layer.alpha_composite(white)

    canvas.alpha_composite(body_layer)
    draw = ImageDraw.Draw(canvas)

    # 버튼 이음선 + 대각선 경계(이음선 위 구간 = 좌우 버튼 분리)
    if detail:
        # 반투명 선은 별도 레이어에 그려 합성 (직접 그리면 몸체 alpha가 덮어써져 구멍이 남)
        seam_layer = Image.new("RGBA", (n, n), (0, 0, 0, 0))
        sd = ImageDraw.Draw(seam_layer)
        seam_w = round(s(0.016))
        sd.line([(box[0], s(SEAM_Y)), (box[2], s(SEAM_Y))], fill=SEAM, width=seam_w)
        # 이음선 위의 사선 구간 = 좌우 버튼 경계
        t = (s(SEAM_Y) - dt[1]) / (db[1] - dt[1])
        mid = (dt[0] + (db[0] - dt[0]) * t, s(SEAM_Y))
        sd.line([dt, mid], fill=SEAM, width=seam_w)
        seam_layer.putalpha(ImageChops.multiply(seam_layer.getchannel("A"), mask))
        canvas.alpha_composite(seam_layer)
        draw = ImageDraw.Draw(canvas)

    # 커서 화살표
    if detail:
        tip = (s(0.415), s(SEAM_Y + 0.035))
        h = s(0.64)
        outline_w = s(0.026)
    else:
        tip = (s(0.30), s(0.14))
        h = s(0.86)
        outline_w = s(0.055)  # 작은 크기: 1px 남짓 외곽선
    pts = [(tip[0] + x * h, tip[1] + y * h) for x, y in ARROW_SHAPE]

    if detail:
        arrow_shadow = Image.new("RGBA", (n, n), (0, 0, 0, 0))
        ImageDraw.Draw(arrow_shadow).polygon(
            [(x + s(0.012), y + s(0.018)) for x, y in pts], fill=(5, 15, 50, 110)
        )
        arrow_shadow = arrow_shadow.filter(ImageFilter.GaussianBlur(s(0.012)))
        canvas.alpha_composite(arrow_shadow)
        draw = ImageDraw.Draw(canvas)

    draw.polygon(pts, fill=ARROW)
    draw.line(pts + [pts[0]], fill=OUTLINE, width=round(outline_w), joint="curve")
    # 꼭짓점을 둥글게 (선 끝 뭉툭함 보정)
    r = outline_w / 2
    for x, y in pts:
        draw.ellipse([x - r, y - r, x + r, y + r], fill=OUTLINE)
    # 외곽선이 안쪽 흰색을 잠식하지 않도록 내부 재도장
    inner = shrink(pts, outline_w * 0.5)
    draw.polygon(inner, fill=ARROW)

    # 클릭 점 + 리플
    if detail:
        c = (s(DOT_CENTER[0]), s(DOT_CENTER[1]))
        dr = s(0.052)
        # 리플은 몸체 안으로 잘라 넣는다
        ripple = Image.new("RGBA", (n, n), (0, 0, 0, 0))
        rd = ImageDraw.Draw(ripple)
        for k, alpha in ((2.1, 60), (1.6, 130)):
            rr = dr * k
            rd.ellipse([c[0] - rr, c[1] - rr, c[0] + rr, c[1] + rr],
                       outline=(255, 255, 255, alpha), width=round(s(0.011)))
        clipped = Image.new("RGBA", (n, n), (0, 0, 0, 0))
        clipped.paste(ripple, (0, 0), ImageChops.multiply(ripple.getchannel("A"), mask))
        canvas.alpha_composite(clipped)
        draw = ImageDraw.Draw(canvas)
        ring = dr + s(0.016)
        draw.ellipse([c[0] - ring, c[1] - ring, c[0] + ring, c[1] + ring], fill=DOT_RING)
        draw.ellipse([c[0] - dr, c[1] - dr, c[0] + dr, c[1] + dr], fill=DOT)
    else:
        c = (s(0.70), s(0.19))
        dr = s(0.105)
        ring = dr + s(0.045)
        draw.ellipse([c[0] - ring, c[1] - ring, c[0] + ring, c[1] + ring], fill=DOT_RING)
        draw.ellipse([c[0] - dr, c[1] - dr, c[0] + dr, c[1] + dr], fill=DOT)

    return canvas.resize((px, px), Image.LANCZOS)


def shrink(points, d):
    """다각형을 무게중심 쪽으로 d만큼 대략 줄인다 (외곽선 안쪽 정리용)."""
    cx = sum(p[0] for p in points) / len(points)
    cy = sum(p[1] for p in points) / len(points)
    out = []
    for x, y in points:
        vx, vy = cx - x, cy - y
        length = (vx * vx + vy * vy) ** 0.5 or 1
        out.append((x + vx / length * d, y + vy / length * d))
    return out


def main():
    render(1024, detail=True).save(ASSETS / "app-icon.png")
    for px in (16, 20, 24):
        render(px, detail=False).save(ASSETS / f"app-icon-{px}.png")
    print("written:", ASSETS / "app-icon.png", "+ 16/20/24")


if __name__ == "__main__":
    main()
