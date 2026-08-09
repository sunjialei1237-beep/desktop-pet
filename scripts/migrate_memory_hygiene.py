# -*- coding: utf-8 -*-
"""One-time memory hygiene governance (2026-08-09 续⁹).
Mirrors mind/memory_gate.rs on EXISTING data + resets test-inflated strengths.

  1. Expire noise facts (value/key deny + out-of-whitelist category, keep current_reading).
  2. Reset non-landmark episode memory_strength -> importance (undo saturation).

NOT blanket-replaying the gate: current_reading (genuine but transient) is
preserved. Backs up the DB first. Read-only dry-run by default; pass --apply.
"""
import os, sys, shutil, sqlite3, datetime

sys.stdout.reconfigure(encoding="utf-8")

DB = os.path.join(os.environ["APPDATA"], "DesktopPet", "desktop_pet.db")
BAK = DB + ".bak-hygiene"
VALID_CAT = {"preference", "relationship", "goal", "profile", "school", "work", "health"}
NOISE_VAL = ("asked about", "asking about", "user asked", "user is asking",
             "is asking about my", "does not know", "doesn't know", "curious about",
             "user is busy", "busy with work")
APPLY = "--apply" in sys.argv


def is_noise(category, key, value):
    vl = value.lower()
    if any(p in vl for p in NOISE_VAL):
        return True
    kl = key.lower()
    if kl.endswith(("_question", "_gap", "_knowledge")) or kl.startswith("belief_in_"):
        return True
    # invented category, but keep current_reading (genuine, transient)
    if category not in VALID_CAT and category != "current_reading":
        return True
    return False


def snap(conn, label):
    print(f"\n===== {label} =====")
    a = conn.execute("SELECT COUNT(*) FROM facts WHERE valid_to IS NULL").fetchone()[0]
    t = conn.execute("SELECT COUNT(*) FROM facts").fetchone()[0]
    print(f"facts: {a} active / {t} total")
    print("active facts by category:")
    for cat, n in conn.execute(
        "SELECT category, COUNT(*) FROM facts WHERE valid_to IS NULL GROUP BY category ORDER BY 2 DESC"
    ):
        mark = "" if cat in VALID_CAT else "  ← out-of-whitelist"
        print(f"   {cat:18} {n}{mark}")
    print("active facts (key: value):")
    for cat, key, value in conn.execute(
        "SELECT category, key, value FROM facts WHERE valid_to IS NULL ORDER BY category, key"
    ):
        print(f"   [{cat}] {key} = {value}")
    eps = conn.execute("SELECT COUNT(*) FROM episodes").fetchone()[0]
    sat = conn.execute(
        "SELECT COUNT(*) FROM episodes WHERE is_landmark=0 AND memory_strength>=0.999"
    ).fetchone()[0]
    print(f"episodes: {eps} total, {sat} non-landmark saturated (strength>=0.999)")
    print("top episodes by strength:")
    for s, i, rc, summ in conn.execute(
        "SELECT memory_strength, importance, recall_count, summary "
        "FROM episodes WHERE is_landmark=0 ORDER BY memory_strength DESC LIMIT 6"
    ):
        print(f"   str={s:.3f} imp={i:.2f} rc={rc:<4} {summ[:34]}")


def main():
    if not os.path.exists(DB):
        sys.exit(f"DB not found: {DB}")
    conn = sqlite3.connect(DB)

    snap(conn, "BEFORE")

    # 1. noise facts to expire
    rows = conn.execute(
        "SELECT id, category, key, value FROM facts WHERE valid_to IS NULL"
    ).fetchall()
    noise = [(r[0], r[1], r[2], r[3]) for r in rows if is_noise(r[1], r[2], r[3])]
    print(f"\n[noise facts to expire] {len(noise)}:")
    for _id, cat, key, value in noise:
        print(f"   EXPIRE [{cat}] {key} = {value}")

    # 2. episode strength reset count
    rst = conn.execute(
        "SELECT COUNT(*) FROM episodes WHERE is_landmark=0 "
        "AND ABS(memory_strength - importance) > 1e-9"
    ).fetchone()[0]
    print(f"[strength reset] {rst} non-landmark episodes will snap to importance")

    if not APPLY:
        print("\nDRY RUN — no changes. Re-run with --apply to commit.")
        conn.close()
        return

    shutil.copy2(DB, BAK)
    print(f"\nbackup -> {BAK}")
    now = datetime.datetime.now(datetime.timezone.utc).isoformat()
    if noise:
        conn.executemany(
            "UPDATE facts SET valid_to=?, updated_at=? WHERE id=?",
            [(now, now, _id) for _id, *_ in noise],
        )
    conn.execute(
        "UPDATE episodes SET memory_strength = importance WHERE is_landmark=0"
    )
    conn.commit()
    print(f"APPLIED: expired {len(noise)} fact(s), reset {rst} episode strength(s)")

    snap(conn, "AFTER")
    conn.close()


if __name__ == "__main__":
    main()
