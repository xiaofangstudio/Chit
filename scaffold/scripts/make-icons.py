# -*- coding: utf-8 -*-
"""
make-icons.py —— 生成 Chit 的应用图标（超椭圆圆角）

规范（小方工作室跨项目图标规范）：
  - 外形：超椭圆（n=5）圆角裁切，外面透明；
  - 界面内部的 Logo 也用同一张圆角图，不许直接放直角原图。

用法：python make-icons.py
输出：../res/chit.ico （含 16/24/32/48/64/128/256 七个尺寸）
"""
import math
import os
from PIL import Image, ImageDraw

# ---------- 参数 ----------
BLUE = (43, 87, 154, 255)      # Word 蓝 #2B579A
BLUE_DARK = (33, 68, 122, 255) # 底部微暗一层，增加一点立体感（纯色渐变仅两层，保持克制）
WHITE = (255, 255, 255, 255)
SUPER_N = 5                    # 超椭圆指数
S = 1024                       # 超采样画布
SIZES = [256, 128, 64, 48, 32, 24, 16]

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.normpath(os.path.join(HERE, "..", "res", "chit.ico"))


def superellipse_mask(size: int) -> Image.Image:
    """n=5 超椭圆蒙版：|x/a|^n + |y/b|^n = 1"""
    m = Image.new("L", (size, size), 0)
    d = ImageDraw.Draw(m)
    n = SUPER_N
    half = size / 2
    # 逐点描出超椭圆边界（超采样足够密时平滑）
    pts = []
    steps = 720
    for i in range(steps):
        t = 2 * math.pi * i / steps
        ct, st = math.cos(t), math.sin(t)
        # 参数化：x = a * sign(cos t)*|cos t|^(2/n)
        x = half * math.copysign(abs(ct) ** (2 / n), ct)
        y = half * math.copysign(abs(st) ** (2 / n), st)
        pts.append((half + x, half + y))
    d.polygon(pts, fill=255)
    return m


def draw_glyph(img: Image.Image):
    """白色便签纹样：圆角纸面 + 三行文字线 + 折角"""
    d = ImageDraw.Draw(img)
    s = S

    def rr(box, radius, **kw):
        d.rounded_rectangle(box, radius=radius, **kw)

    # 纸面（白色圆角矩形，占图标中央约 56%）
    pad = s * 0.26
    x0, y0, x1, y1 = pad, pad * 0.86, s - pad, s - pad * 0.86
    radius = s * 0.07
    # 先画纸的投影感：略深蓝一层在右下偏移（克制、两层以内）
    rr([x0 + s*0.02, y0 + s*0.02, x1 + s*0.02, y1 + s*0.02], radius, fill=BLUE_DARK)
    rr([x0, y0, x1, y1], radius, fill=WHITE)

    # 文字线（蓝色），左边距 12%，行高均匀
    line_x0 = x0 + s * 0.09
    line_x1 = x1 - s * 0.14   # 最后一行短一点
    th = s * 0.045            # 线粗
    ys = [y0 + s * 0.16, y0 + s * 0.30, y0 + s * 0.44]
    widths = [s * 0.42, s * 0.30, s * 0.18]  # 递减，像文本
    for y, w in zip(ys, widths):
        rr([line_x0, y - th / 2, line_x0 + w, y + th / 2], th / 2, fill=BLUE)

    # 右上折角（把纸的右上角切出蓝色三角，模拟折页）
    fold = s * 0.10
    fx, fy = x1 - fold, y0
    d.polygon([(fx, fy), (x1, fy + fold), (x1, fy), (fx + 0.001, fy)], fill=BLUE)


def build() -> Image.Image:
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    # 背景：纯色蓝（超椭圆裁切后直接使用）
    bg = Image.new("RGBA", (S, S), BLUE)
    draw_glyph(bg)
    # 底层超椭圆背景（保证 glyph 没盖住的地方也是蓝）
    base = Image.new("RGBA", (S, S), BLUE)
    base.alpha_composite(bg)
    img = base
    # 超椭圆蒙版裁切
    mask = superellipse_mask(S)
    img.putalpha(mask)
    return img


def save_ico(img: Image.Image, path: str):
    imgs = [img.resize((sz, sz), Image.LANCZOS) for sz in SIZES]
    imgs[0].save(path, format="ICO", sizes=[(s, s) for s in SIZES],
                 append_images=imgs[1:])
    print("已写出:", path)


if __name__ == "__main__":
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    icon = build()
    save_ico(icon, OUT)
    # 顺带导出一张 256 预览图，便于核对
    prev = os.path.join(os.path.dirname(OUT), "chit-preview.png")
    icon.resize((256, 256), Image.LANCZOS).save(prev)
    print("预览:", prev)
