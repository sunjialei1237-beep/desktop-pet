# -*- coding: utf-8 -*-
"""P5 用户盲选偏好测试材料生成（Soul v2 方案 §4 P5）。

从 baseline 与 experiment 两份 prompt-quality 报告的明细表按 case id 配对，
抽 N 对乱序呈现（A/B 随机），输出：
  docs/review/user-blind-preference-2026-08-15.md   （给用户，无标注哪个是 v2）
  docs/review/user-blind-preference-2026-08-15.key.md（评分后再看的答案）

用法: python scripts/make_blind_pairs.py [N=15]
"""
import io
import os
import random
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASE = os.path.join(ROOT, "docs/review/prompt-quality-report-2026-08-15-baseline.md")
EXP = os.path.join(ROOT, "docs/review/prompt-quality-report-2026-08-15.md")
OUT = os.path.join(ROOT, "docs/review/user-blind-preference-2026-08-15.md")
KEY = os.path.join(ROOT, "docs/review/user-blind-preference-2026-08-15.key.md")


def parse_rows(path):
    rows = {}
    with io.open(path, encoding="utf-8") as f:
        for line in f:
            m = re.match(r"^\| (\d+) \| (.*?) \| (.*?) \| (.*?) \| (.*?) \|", line)
            if not m:
                continue
            cid, group, inp, route, reply = (x.strip() for x in m.groups())
            if not cid.isdigit():
                continue
            rows[int(cid)] = (group, inp, reply.replace("\\|", "|"))
    return rows


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 15
    base, exp = parse_rows(BASE), parse_rows(EXP)
    common = sorted(set(base) & set(exp))
    # 多样性抽样：每个大组最多 2 条
    picked, per_group = [], {}
    random.seed(20260815)
    random.shuffle(common)
    for cid in common:
        g = base[cid][0]
        if per_group.get(g, 0) >= 2:
            continue
        picked.append(cid)
        per_group[g] = per_group.get(g, 0) + 1
        if len(picked) >= n:
            break

    out = io.StringIO()
    key = io.StringIO()
    out.write(u"# 盲选：哪一条更像「她」?\n\n")
    out.write(u"每对来自同一个用户输入的两个版本（乱序 A/B）。请凭直觉选更像璃的那条"
              u"（判据：**不是哪句写得好，而是「如果不是璃，我会马上觉得不对」**）。\n\n")
    key.write(u"| # | 输入 | A | B |\n|---|---|---|---|\n")
    for i, cid in enumerate(picked, 1):
        g, inp, _ = base[cid]
        flip = random.random() < 0.5
        a_reply = exp[cid][2] if flip else base[cid][2]
        b_reply = base[cid][2] if flip else exp[cid][2]
        a_src = "v2" if flip else "v1"
        b_src = "v1" if flip else "v2"
        out.write(u"## 第 %d 对（%s）\n\n用户：**%s**\n\n- A：%s\n- B：%s\n\n你的选择：A / B\n\n---\n\n"
                  % (i, g, inp, a_reply, b_reply))
        key.write(u"| %d | %s | %s | %s |\n" % (i, inp, a_src, b_src))
    io.open(OUT, "w", encoding="utf-8").write(out.getvalue())
    io.open(KEY, "w", encoding="utf-8").write(key.getvalue())
    print("pairs written:", len(picked), "->", OUT)


if __name__ == "__main__":
    main()
