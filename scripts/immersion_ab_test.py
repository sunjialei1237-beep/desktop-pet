# -*- coding: utf-8 -*-
"""小范围 A/B 实测：DeepSeek v4-flash thinking-off（现状）vs thinking-on + 角色沉浸指令。

镜像生产 prompt 结构（mind/grounding.rs 的消息布局）：
  [system 静态头: system.txt + [Persona] + Grounding Constraint]
  [history 若干轮]
  [system 尾部: [Relationship] + [Memories]]
  [system 近端: [Current time] + [Current Mood] + [Intent]]   ← 实验组的沉浸指令骑在这里
  [user: 场景消息]

测量：首字延迟（首个 content delta）、总延迟、reasoning/content 字符数、usage tokens。
产出：scripts/immersion_ab_report.md（含完整回复与思考原文，供人工对比）。

用法：python scripts/immersion_ab_test.py [--runs N]
"""
import json
import re
import sys
import time
import tomllib
import urllib.request
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8")

ROOT = Path(__file__).resolve().parent.parent
CONFIG = Path.home() / "AppData" / "Roaming" / "DesktopPet" / "config.toml"

# README 指令原文（INNER_OS_MARKER，逐字复制）
INNER_OS_MARKER = (
    "【角色沉浸要求】在你的思考过程（<think>标签内）中，请遵守以下规则：\n"
    "1. 请以角色第一人称进行内心独白，用括号包裹内心活动，例如\"（心想：……）\"或\"(内心OS：……)\"\n"
    "2. 用第一人称描写角色的内心感受，例如\"我心想\"\"我觉得\"\"我暗自\"等\n"
    "3. 思考内容应沉浸在角色中，通过内心独白分析剧情和规划回复"
)

MEMORY_CONSTRAINT = (
    "[Grounding Constraint]\n"
    "The following memories are what you actually retrieved. You may respond based on "
    "these memories about the user. If you have no relevant memory for something, "
    "say you are not sure rather than fabricating. Each memory below is annotated "
    "with its confidence level and source date. Do not present information as "
    "remembered unless it appears in the memories section below."
)

QA_MODE_PROMPT = (
    "[Direct-Answer Mode]\n"
    "用户这次问的是知识、技术或事实类问题，ta 想要的是答案，不是闲聊。直接、准确、简短地回答，"
    "像朋友随口解释一样自然，不要上课、不要绕圈子。不要引用记忆，不要追问，不要往自己或宠物相关话题上联想。"
    "不要假装记得用户说过什么，不要编造用户的过去、偏好或经历——你不知道的事就别说。"
    "通常一两句话就够，除非问题本身确实需要展开。不确定就老实说不知道。"
)

PERSONA_BLOCK = (
    "[Persona]\n"
    "Core personality: gentle, curious, perceptive\n"
    "Adaptive traits: playful\n"
    "用户的称呼: 小磊\n"
    "你的名字: 璃\n"
    "你性格的底子（ta 最初的心愿）: 温柔，又有点调皮\n"
    "与用户的关系设定: 日常陪伴的朋友"
)

MEMORIES_BLOCK = (
    "[Relationship]\n"
    "Relationship: closeness 42/100, known each other since 2026-07-16 (41 days), 168 conversations\n\n"
    "[Memories]\n"
    "- [Fact] work: 找实习 / 正在找暑期实习，投了几家还在等消息 (confidence: high, 25天前（7月31日）)\n"
    "- [Fact] fitness: 深蹲 / 深蹲练到100kg了 (confidence: high, 12天前（8月14日）)\n"
    "- [Episode] 用户带宠物狗糯米去看了流浪狗，觉得它们很可爱 (importance: medium, emotion: happy, 13天前（8月13日）)\n"
    "- [Episode] 用户说要早睡，又熬夜了 (importance: low, emotion: neutral, 3天前)\n"
    "（以上即全部记忆。只可引用已列出的内容；不得添加未列出的项目、人名、事件，"
    "也不得编造\"你上次说/提过/念叨\"之类的出处——记着就是记着，没有出处别硬安一个。"
    "每条记忆都标了它是多久前的事：提到它的时间必须照标注说——标着「4天前」的绝不能说成「昨天」，不确定就说\"之前\"；"
    "也不要主动报日期出处——「上个月你提到」不像聊天，像查档案，被问到再说。）"
)

# (场景名, 近端指令类型, user 消息, [Intent] 行)
SCENARIOS = [
    ("S1 情感分享", "engage",
     "我面试过了！", "goal: engage\ntone: playful\n(engage: react specifically to what they just shared — prove you listened with something concrete. You may ask ONE genuine follow-up if you're actually curious, but often a single heartfelt line with no question is more natural. Never ask a generic '怎么样'.)"),
    ("S2 日常疲惫", "converse",
     "今天好累，什么都不想干", "goal: converse\ntone: gentle"),
    ("S3 事实问答", "qa",
     "什么是地心引力？", None),
    ("S4 记忆关联", "converse",
     "我最近都在忙啥来着", "goal: converse\ntone: gentle"),
]

HISTORY = [
    ("user", "早"),
    ("assistant", "早。昨晚睡得咋样？"),
    ("user", "还行吧，就是做了个奇怪的梦"),
    ("assistant", "什么梦，说来听听。"),
]

CLICHES = ["不是…而是", "不是……而是", "稳稳接住", "总而言之", "这就够了", "基石", "深深地看着", "眼里闪着"]
LEAK_PATTERNS = ["（心想", "(内心OS", "(内心os", "<think", "（内心"]


def load_config():
    with open(CONFIG, "rb") as f:
        cfg = tomllib.load(f)
    llm = cfg["llm"]
    base = llm["base_url"].rstrip("/")
    # platform.deepseek.com is the web console host (POST → 405); the API host
    # is api.deepseek.com. Probe-verified 2026-08-26.
    base = base.replace("platform.deepseek.com", "api.deepseek.com")
    if not base.endswith("/chat/completions"):
        base += "/v1/chat/completions" if "/v" not in base.split("//", 1)[1] else "/chat/completions"
    return base, llm["main_model"], llm["api_key"]


def near_end(scenario_type, intent_line):
    import datetime
    now = datetime.datetime.now()
    wd = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"][now.weekday()]
    tod = "上午" if now.hour < 12 else ("下午" if now.hour < 18 else "晚上")
    time_sec = f"[Current time]\n现在 {now:%H:%M} {wd} {now:%Y-%m-%d}\n时段：{tod}"
    if scenario_type == "qa":
        return QA_MODE_PROMPT + "\n\n" + time_sec + "\n\n[Current Mood] 平静 (mood 0.5, energy 0.6, social 0.5, stress 0.3)\n\n[Intent] goal: converse"
    mood = "[Current Mood] 开心 (mood 0.7, energy 0.6, social 0.5, stress 0.3)" if scenario_type == "engage" else \
           "[Current Mood] 平静 (mood 0.5, energy 0.5, social 0.5, stress 0.4)"
    hint = "\n（如果和此刻话题不冲突：心情不错的话，语气可以带点雀跃）" if scenario_type == "engage" else ""
    return time_sec + "\n\n" + mood + hint + "\n\n[Intent] " + intent_line


def build_messages(scenario_type, user_msg, intent_line, immersion):
    system_txt = (ROOT / "src-tauri" / "resources" / "prompts" / "system.txt").read_text(encoding="utf-8")
    msgs = [{"role": "system", "content": system_txt + "\n\n" + PERSONA_BLOCK + "\n\n" + MEMORY_CONSTRAINT}]
    for role, content in HISTORY:
        msgs.append({"role": role, "content": content})
    msgs.append({"role": "system", "content": MEMORIES_BLOCK})
    msgs.append({"role": "system", "content": near_end(scenario_type, intent_line)})
    if immersion:
        msgs.append({"role": "system", "content": INNER_OS_MARKER})
    msgs.append({"role": "user", "content": user_msg})
    return msgs


def call_api(url, model, key, messages, thinking_on, effort=None):
    payload = {
        "model": model,
        "messages": messages,
        "temperature": 0.8,
        "max_tokens": 4096,
        "stream": True,
        "stream_options": {"include_usage": True},
        "thinking": {"type": "enabled" if thinking_on else "disabled"},
    }
    if effort:
        payload["reasoning_effort"] = effort
    req = urllib.request.Request(
        url, data=json.dumps(payload).encode("utf-8"),
        headers={"Content-Type": "application/json", "Authorization": f"Bearer {key}"},
    )
    t0 = time.perf_counter()
    t_first_content = None
    t_first_reasoning = None
    content, reasoning = [], []
    usage = {}
    with urllib.request.urlopen(req, timeout=180) as resp:
        for raw in resp:
            line = raw.decode("utf-8", errors="replace").strip()
            if not line.startswith("data: "):
                continue
            data = line[6:]
            if data == "[DONE]":
                break
            try:
                chunk = json.loads(data)
            except json.JSONDecodeError:
                continue
            if chunk.get("usage"):
                usage = chunk["usage"]
            if not chunk.get("choices"):
                continue
            delta = chunk["choices"][0].get("delta", {})
            if delta.get("reasoning_content"):
                if t_first_reasoning is None:
                    t_first_reasoning = time.perf_counter() - t0
                reasoning.append(delta["reasoning_content"])
            if delta.get("content"):
                if t_first_content is None:
                    t_first_content = time.perf_counter() - t0
                content.append(delta["content"])
    return {
        "ttft_content": t_first_content,
        "ttft_reasoning": t_first_reasoning,
        "total": time.perf_counter() - t0,
        "content": "".join(content),
        "reasoning": "".join(reasoning),
        "usage": usage,
    }


def count_sentences(text):
    return len([s for s in re.split(r"[。！？!?\n]", text) if s.strip()])


def main():
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--arm", choices=["A", "B", "C"],
                        help="A=thinking-off, B=thinking-on+immersion, C=B+reasoning_effort:low. "
                             "省略则全量跑 A+B。")
    args = parser.parse_args()
    arms = {"A": ("A thinking-off", False, None),
            "B": ("B thinking-on+immersion", True, None),
            "C": ("C thinking-on+immersion+effort-low", True, "low")}
    selected = [args.arm] if args.arm else ["A", "B"]

    url, model, key = load_config()
    print(f"endpoint: {url}  model: {model}")
    results = []
    for name, sc_type, user_msg, intent_line in SCENARIOS:
        for arm_key in selected:
            arm_label, immersion, effort = arms[arm_key]
            msgs = build_messages(sc_type, user_msg, intent_line, immersion)
            r = call_api(url, model, key, msgs, immersion, effort)
            r.update(scenario=name, arm=arm_label, user_msg=user_msg, n_msgs=len(msgs))
            results.append(r)
            ttft = f"{r['ttft_content']:.1f}s" if r["ttft_content"] else "n/a"
            print(f"[{name} | {arm_label}] 首字 {ttft} | 总 {r['total']:.1f}s | "
                  f"回复 {len(r['content'])}字/{count_sentences(r['content'])}句 | "
                  f"思考 {len(r['reasoning'])}字 | usage {json.dumps(r['usage'], ensure_ascii=False)}")

    # 报告
    lines = ["# 角色沉浸 A/B 实测报告", "", f"endpoint: `{url}` model: `{model}`", ""]
    lines.append("| 场景 | 臂 | 首字(s) | 总耗时(s) | 回复句数 | 思考字数 | completion tokens |")
    lines.append("|---|---|---|---|---|---|---|")
    for r in results:
        ttft = f"{r['ttft_content']:.1f}" if r["ttft_content"] else "n/a"
        comp = r["usage"].get("completion_tokens", "?")
        lines.append(f"| {r['scenario']} | {r['arm']} | {ttft} | {r['total']:.1f} | {count_sentences(r['content'])} | {len(r['reasoning'])} | {comp} |")
    for r in results:
        lines += [f"\n---\n\n## {r['scenario']} · {r['arm']}", f"\n**用户**: {r['user_msg']}", ""]
        if r["reasoning"]:
            lines += ["**思考过程（reasoning_content）**:", "", "```", r["reasoning"], "```", ""]
        hit = [c for c in CLICHES if c in r["content"]]
        leak = [c for c in LEAK_PATTERNS if c in r["content"]]
        notes = []
        if hit:
            notes.append(f"口癖命中: {hit}")
        if leak:
            notes.append(f"思考泄漏到正文: {leak}")
        lines += ["**回复**:", "", "```", r["content"], "```", ""]
        if notes:
            lines.append("**⚠ " + "；".join(notes) + "**\n")
        else:
            lines.append("*口癖/泄漏检查：无命中*\n")
    out = ROOT / "scripts" / (
        f"immersion_{args.arm.lower()}_report.md" if args.arm else "immersion_ab_report.md")
    out.write_text("\n".join(lines), encoding="utf-8")
    print(f"\nreport -> {out}")


if __name__ == "__main__":
    main()
