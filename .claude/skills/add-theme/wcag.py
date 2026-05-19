#!/usr/bin/env python3
"""WCAG 2.x コントラスト比計算ツール。

使い方:
    python3 wcag.py "#FG" "#BG" ["#FG" "#BG" ...]

fg/bg のペアを渡すと各ペアのコントラスト比を計算し、4.5:1（WCAG AA）
を満たすか判定する。1 ペアでも未達なら exit code 1 を返す。

mdview のテーマ追加時、検索ハイライト等のフォアグラウンド/バックグラウンド
ペアが AA を満たすかの確認に使う（CLAUDE.md「テーマ機能メンテナンスガイド」）。
"""
import sys

AA_THRESHOLD = 4.5


def _channel(c: int) -> float:
    s = c / 255
    return s / 12.92 if s <= 0.03928 else ((s + 0.055) / 1.055) ** 2.4


def luminance(hexstr: str) -> float:
    h = hexstr.lstrip("#")
    if len(h) != 6:
        raise ValueError(f"6 桁の hex を指定してください: {hexstr!r}")
    r, g, b = (int(h[i:i + 2], 16) for i in (0, 2, 4))
    return 0.2126 * _channel(r) + 0.7152 * _channel(g) + 0.0722 * _channel(b)


def contrast_ratio(fg: str, bg: str) -> float:
    lo, hi = sorted((luminance(fg), luminance(bg)))
    return (hi + 0.05) / (lo + 0.05)


def main(argv: list[str]) -> int:
    if len(argv) < 2 or len(argv) % 2 != 0:
        print("usage: python3 wcag.py <fg> <bg> [<fg> <bg> ...]", file=sys.stderr)
        return 2
    failed = False
    for i in range(0, len(argv), 2):
        fg, bg = argv[i], argv[i + 1]
        ratio = contrast_ratio(fg, bg)
        ok = ratio >= AA_THRESHOLD
        failed = failed or not ok
        verdict = "PASS" if ok else f"FAIL (需 >= {AA_THRESHOLD})"
        print(f"{fg} / {bg}: {ratio:.2f}:1  {verdict}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
