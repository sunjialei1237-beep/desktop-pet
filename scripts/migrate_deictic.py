# -*- coding: utf-8 -*-
"""One-time deictic-time hygiene for existing memory data (2026-08-14).
Proactive bubble governance R5: relative time words ("今天/昨天/明天/最近/上周"…)
were stored verbatim in summaries/titles, so a bubble could echo "你说今天在找实习"
days later. The pipeline now strips them at extraction and surfacing; this script
cleans what is ALREADY stored:

  1. episodes.summary: strip deictic words + collapse whitespace.
  2. facts.value / facts.key: same.
  3. pending_events.title: same.

Backs up the DB first. Read-only dry-run by default; pass --apply.
"""
import os, sys, shutil, sqlite3, datetime, re

sys.stdout.reconfigure(encoding="utf-8")

DB = os.path.join(os.environ["APPDATA"], "DesktopPet", "desktop_pet.db")
BAK = DB + ".bak-deictic"
# Longest-first so "上个星期" is removed before "上周" would interfere — simple
# substring removal is order-independent, but the list mirrors deictic.rs.
DEICTIC = [
    "前天", "昨天", "今天", "明天", "后天", "今早", "今晚", "今儿",
    "明早", "明晚", "昨儿",
    "上个星期", "这个星期", "下个星期", "上周", "这周", "下周",
    "上个月", "这个月", "下个月", "去年", "今年", "明年",
    "最近", "刚刚", "刚才", "前几天", "过几天", "前阵子", "这段时间", "这几天",
]
APPLY = "--apply" in sys.argv


def neutralize(text):
    if not text:
        return text
    for w in DEICTIC:
        text = text.replace(w, "")
    return re.sub(r"\s+", " ", text).strip()


def snap(conn, label):
    print(f"\n===== {label} =====")
    for table, cols in (("episodes", ["summary"]), ("facts", ["value", "key"]),
                        ("pending_events", ["title"])):
        for col in cols:
            n = conn.execute(
                f"SELECT COUNT(*) FROM {table} WHERE {col} LIKE '%今天%' OR {col} LIKE '%昨天%' "
                f"OR {col} LIKE '%明天%' OR {col} LIKE '%最近%' OR {col} LIKE '%上周%' "
                f"OR {col} LIKE '%今早%' OR {col} LIKE '%今晚%' OR {col} LIKE '%今年%'"
            ).fetchone()[0]
            print(f"{table}.{col}: {n} rows with deictic words")


def main():
    if not os.path.exists(DB):
        sys.exit(f"DB not found: {DB}")
    conn = sqlite3.connect(DB)

    snap(conn, "BEFORE")

    changes = []
    for table, cols in (("episodes", ["summary"]), ("facts", ["value", "key"]),
                        ("pending_events", ["title"])):
        for col in cols:
            rows = conn.execute(f"SELECT id, {col} FROM {table}").fetchall()
            for _id, val in rows:
                nv = neutralize(val)
                if nv != val:
                    changes.append((table, col, _id, val, nv))

    print(f"\n[to neutralize] {len(changes)} field(s):")
    for table, col, _id, old, new in changes[:40]:
        print(f"   {table}.{col} id={_id[:8]}  {old[:40]!r} -> {new[:40]!r}")
    if len(changes) > 40:
        print(f"   … and {len(changes) - 40} more")

    if not APPLY:
        print("\nDRY RUN — no changes. Re-run with --apply to commit.")
        conn.close()
        return

    shutil.copy2(DB, BAK)
    print(f"\nbackup -> {BAK}")
    now = datetime.datetime.now(datetime.timezone.utc).isoformat()
    for table, col, _id, _old, new in changes:
        if table == "facts":
            conn.execute(
                f"UPDATE facts SET {col}=?, updated_at=? WHERE id=?",
                (new, now, _id),
            )
        else:
            conn.execute(f"UPDATE {table} SET {col}=? WHERE id=?", (new, _id))
    conn.commit()
    print(f"APPLIED: neutralized {len(changes)} field(s)")

    snap(conn, "AFTER")
    conn.close()


if __name__ == "__main__":
    main()
