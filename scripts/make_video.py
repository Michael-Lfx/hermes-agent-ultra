"""
Hermes Agent Ultra — Weekly Progress Video (May 25 → Jun 2, 2026)
45 seconds @ 1080p30fps, programmatic frame generation
"""
import math, os, textwrap
import numpy as np
from PIL import Image, ImageDraw, ImageFont
import imageio.v3 as iio

# ── config ──────────────────────────────────────────────────────────
W, H = 1920, 1088          # 1088 for macro_block_size=16
FPS = 30
DURATION = 45
TOTAL_FRAMES = FPS * DURATION

BG_TOP = np.array([12, 12, 30], dtype=np.uint8)
BG_BOT = np.array([30, 20, 50], dtype=np.uint8)

ACCENT = (0, 200, 255)       # cyan
WHITE  = (240, 240, 255)
DIM    = (120, 120, 160)
GREEN  = (80, 255, 160)
ORANGE = (255, 180, 60)
PINK   = (255, 100, 180)

# ── data ────────────────────────────────────────────────────────────
# Each scene: (start_sec, end_sec, theme, subtitle, bullet_points, stat_label, stat_value, color)
SCENES = [
    (0, 5.5,
     "5月25–26日", "基础修复与对齐",
     ["结构化日志格式 + 时区修复",
      "会话持久化事务性写入",
      "上下文压缩引擎接入",
      "CLI 闪烁和重复光标修复"],
     "commits", "14", ACCENT),

    (5.5, 11,
     "5月27日", "新功能井喷",
     ["DuckDuckGo 搜索后端上线",
      "桌面会议录音管线",
      "Discord 网关 MVP 完成",
      "依赖检测交互式安装"],
     "新增工具", "5", GREEN),

    (11, 16,
     "5月28–29日", "Discord 深度完善",
     ["P1: 斜杠命令、表情、频道过滤",
      "P2: 回复模式、Markdown、流式编辑",
      "兴趣 POI 本地存储",
      "computer_use 桌面控制移植"],
     "Discord commits", "10", ORANGE),

    (16, 22,
     "5月29日", "性能优化日",
     ["网关复用 agent 循环",
      "LLM Provider 缓存",
      "文件搜索走 ripgrep 加速",
      "搜索去重 + glob 预编译"],
     "perf 提速", "8项", PINK),

    (22, 28,
     "5月30日", "系统深度优化",
     ["SQLite 增量消息写入",
      "压缩锁 + 会话续接",
      "系统 prompt 缓存",
      "微信二维码登录流程"],
     "关键突破", "QR登录", ACCENT),

    (28, 34,
     "5月31日–6月1日", "平台大扩展",
     ["微信网关完整 Rust 实现",
      "飞书工具注册 + 配置文档",
      "浏览器快照 + 辅助LLM摘要",
      "内容检索 playbook 工具链"],
     "新增平台", "3", GREEN),

    (34, 40,
     "6月2日", "极致性能",
     ["零拷贝 API 消息 (Arc 缓存)",
      "工具 schema 缓存 + 稳定排序",
      "Prompt cache 就地标记",
      "记忆预压缩 + 会话切换钩子"],
     "zero-copy", "✓", ORANGE),

    (40, 45,
     "一周总结", "May 25 → Jun 2, 2026",
     ["130+ commits 持续迭代",
      "微信/飞书/Discord 三平台网关",
      "性能: 零拷贝 + Arc + 缓存全覆盖",
      "从基础对齐到极致优化的飞跃"],
     "总 commits", "130+", (200, 200, 255)),
]

# ── fonts ───────────────────────────────────────────────────────────
def _font(size, bold=False):
    names = ["Microsoft YaHei", "SimHei", "Noto Sans CJK SC", "Arial"]
    for n in names:
        try:
            f = ImageFont.truetype(n + ".ttc", size) if not bold else ImageFont.truetype(n + " Bold.ttc", size)
            return f
        except Exception:
            try:
                return ImageFont.truetype(n + ".ttf", size)
            except Exception:
                continue
    return ImageFont.load_default()

FONT_THEME   = _font(72, bold=True)
FONT_DATE    = _font(48, bold=True)
FONT_SUB     = _font(36)
FONT_BULLET  = _font(28)
FONT_STAT_L  = _font(22)
FONT_STAT_V  = _font(96, bold=True)
FONT_FOOTER  = _font(20)
FONT_TITLE   = _font(56, bold=True)

# ── helpers ─────────────────────────────────────────────────────────
def gradient_bg():
    arr = np.zeros((H, W, 3), dtype=np.uint8)
    for y in range(H):
        t = y / H
        arr[y] = (BG_TOP * (1 - t) + BG_BOT * t).astype(np.uint8)
    return arr

BG = gradient_bg()

def ease_out(t):
    return 1 - (1 - t) ** 3

def lerp(a, b, t):
    return a + (b - a) * t

def draw_rounded_rect(draw, xy, radius, fill=None, outline=None, width=1):
    x0, y0, x1, y1 = xy
    draw.rounded_rectangle(xy, radius=radius, fill=fill, outline=outline, width=width)

# ── frame renderer ──────────────────────────────────────────────────
def render_frame(frame_idx):
    t_sec = frame_idx / FPS
    img = Image.fromarray(BG.copy())
    draw = ImageDraw.Draw(img)

    # ── find current scene ──
    scene = None
    scene_local_t = 0
    for s in SCENES:
        if s[0] <= t_sec < s[1]:
            scene = s
            scene_local_t = (t_sec - s[0]) / (s[1] - s[0])
            break

    if scene is None:
        # final hold
        scene = SCENES[-1]
        scene_local_t = 1.0

    _, _, theme, subtitle, bullets, stat_label, stat_value, color = scene

    # ── global progress bar ──
    progress = t_sec / DURATION
    bar_y = H - 16
    draw.rectangle([0, bar_y, W, H], fill=(20, 20, 40))
    draw.rectangle([0, bar_y, int(W * progress), H], fill=(*color, ))

    # ── date/timeline at top ──
    draw.text((60, 30), "HERMES AGENT ULTRA", font=FONT_FOOTER, fill=DIM)
    draw.text((W - 400, 30), f"2026.05.25 → 06.02", font=FONT_FOOTER, fill=DIM)

    # ── scene transitions ──
    fade_in = min(1.0, scene_local_t * 5)   # 0.2s fade in
    alpha_byte = int(255 * fade_in)

    # ── theme title ──
    draw.text((120, 90), theme, font=FONT_DATE, fill=(*color, ))

    # ── subtitle ──
    draw.text((120, 155), subtitle, font=FONT_SUB, fill=WHITE)

    # ── divider line ──
    line_w = min(int(600 * fade_in), 600)
    draw.rectangle([120, 210, 120 + line_w, 213], fill=color)

    # ── bullet points with stagger ──
    for i, bullet in enumerate(bullets):
        bullet_delay = 0.08 * i
        bullet_t = max(0, min(1, (scene_local_t - bullet_delay) * 4))
        if bullet_t <= 0:
            continue

        # slide in from left
        offset_x = int(40 * (1 - ease_out(bullet_t)))
        by = 250 + i * 52

        # bullet dot
        dot_alpha = ease_out(bullet_t)
        r = int(6 * dot_alpha)
        if r > 0:
            draw.ellipse([120 + offset_x, by + 8, 120 + offset_x + r*2, by + 8 + r*2], fill=color)

        # text
        txt_color = tuple(int(c * dot_alpha + 30 * (1 - dot_alpha)) for c in WHITE)
        draw.text((148 + offset_x, by), bullet, font=FONT_BULLET, fill=txt_color)

    # ── stat card on right ──
    card_x, card_y = W - 480, 100
    card_w, card_h = 380, 260
    card_fade = min(1.0, scene_local_t * 3)

    # card bg
    card_bg = (25, 25, 50, int(200 * card_fade))
    overlay = Image.new("RGBA", img.size, (0, 0, 0, 0))
    ov_draw = ImageDraw.Draw(overlay)
    draw_rounded_rect(ov_draw, (card_x, card_y, card_x + card_w, card_y + card_h),
                       radius=20, fill=card_bg, outline=(*color, int(180 * card_fade)), width=2)
    img = Image.alpha_composite(img.convert("RGBA"), overlay).convert("RGB")
    draw = ImageDraw.Draw(img)

    # stat value (count-up animation)
    if stat_value.isdigit():
        display_val = str(int(int(stat_value) * ease_out(min(1, scene_local_t * 2))))
    else:
        display_val = stat_value

    val_color = tuple(int(c * card_fade) for c in color)
    draw.text((card_x + 40, card_y + 30), display_val, font=FONT_STAT_V, fill=val_color)
    draw.text((card_x + 40, card_y + 170), stat_label, font=FONT_STAT_L, fill=DIM)

    # ── floating particles ──
    for i in range(8):
        px = int((i * 237 + frame_idx * 1.5) % W)
        py = int((i * 179 + frame_idx * 0.8) % H)
        pr = 2 + int(2 * math.sin(frame_idx * 0.05 + i))
        p_alpha = int(60 + 40 * math.sin(frame_idx * 0.03 + i * 1.7))
        draw.ellipse([px-pr, py-pr, px+pr, py+pr], fill=(*color, ), )

    # ── scene indicator dots at bottom ──
    total_scenes = len(SCENES)
    dot_start_x = W // 2 - total_scenes * 16
    current_scene_idx = 0
    for idx, s in enumerate(SCENES):
        if s[0] <= t_sec < s[1]:
            current_scene_idx = idx
            break
    else:
        current_scene_idx = total_scenes - 1

    for idx in range(total_scenes):
        dx = dot_start_x + idx * 32
        dy = H - 48
        if idx == current_scene_idx:
            draw.ellipse([dx, dy, dx + 10, dy + 10], fill=color)
        elif idx < current_scene_idx:
            draw.ellipse([dx, dy, dx + 10, dy + 10], fill=DIM)
        else:
            draw.ellipse([dx, dy, dx + 8, dy + 8], outline=DIM)

    return np.array(img)

# ── render ──────────────────────────────────────────────────────────
print(f"Rendering {TOTAL_FRAMES} frames at {W}x{H}@{FPS}fps ...")
frames = []
for i in range(TOTAL_FRAMES):
    if i % (FPS * 5) == 0:
        print(f"  {i}/{TOTAL_FRAMES} ({i/FPS:.0f}s)")
    frames.append(render_frame(i))

out_path = os.path.join(os.path.dirname(__file__), "..", "hermes_weekly.mp4")
out_path = os.path.abspath(out_path)
print(f"Writing to {out_path} ...")
iio.imwrite(out_path, frames, fps=FPS, codec="libx264", quality=8,
            output_params=["-pix_fmt", "yuv420p"])
print(f"Done! {out_path}")
print(f"Duration: {DURATION}s, Resolution: {W}x{H}, FPS: {FPS}")