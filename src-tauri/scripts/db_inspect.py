import sqlite3, sys, os

db = r"C:\Users\SunJialei\AppData\Roaming\DesktopPet\desktop_pet.db"
c = sqlite3.connect(db)
c.row_factory = sqlite3.Row

tables = [r[0] for r in c.execute("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")]
print("TABLES:", tables)

print("\n--- FACTS (recent 25) ---")
for r in c.execute("SELECT category,key,value,confidence,updated_at FROM facts ORDER BY updated_at DESC LIMIT 25"):
    print(dict(r))

print("\n--- EPISODES (recent 10) ---")
try:
    cnt = c.execute("SELECT COUNT(*) FROM episodes").fetchone()[0]
    print("episode count:", cnt)
    for r in c.execute("SELECT id,time,summary,importance,created_at FROM episodes ORDER BY created_at DESC LIMIT 10"):
        print(dict(r))
except Exception as e:
    print("episodes err:", e)

print("\n--- USER_PROFILE / onboarding ---")
for t in ("user_profile","onboarding_answers"):
    try:
        rows = [dict(r) for r in c.execute("SELECT * FROM " + t)]
        print(t, rows)
    except Exception as e:
        print(t, "err:", e)
import sqlite3, sys

db = r"C:\Users\SunJialei\AppData\Roaming\DesktopPet\desktop_pet.db"
c = sqlite3.connect(db)

print("=== BYTE DIAGNOSIS of corrupted Chinese fact values ===")
for key in ["favorite_movie", "knowledge_question"]:
    row = c.execute("SELECT CAST(value AS BLOB) FROM facts WHERE key=? ORDER BY updated_at DESC LIMIT 1", (key,)).fetchone()
    if not row:
        print(key, "-> none")
        continue
    raw = bytes(row[0])
    print("\nkey:", key)
    print(" raw bytes:", raw.hex(" "))
    print(" raw len:", len(raw))
    for enc in ["utf-8", "gbk", "gb18030", "cp1252", "latin-1"]:
        try:
            print(f"  decode({enc}) -> {raw.decode(enc)!r}")
        except Exception as e:
            print(f"  decode({enc}) -> ERR {e}")

print("\n=== A clean reference: what 'xing ji chuan yue' should be ===")
ref = "星际穿越"
print(" utf-8 bytes:", ref.encode("utf-8").hex(" "))
print(" gbk   bytes:", ref.encode("gbk").hex(" "))
print(" cp1252 roundtrip of gbk bytes:", ref.encode("gbk").decode("latin-1")[:20])

print("\n=== Compare: english fact (should be clean utf-8) ===")
row = c.execute("SELECT CAST(value AS BLOB) FROM facts WHERE value LIKE 'likes milk%' LIMIT 1").fetchone()
if row:
    print(" bytes:", bytes(row[0]).hex(" "))
    print(" utf-8:", bytes(row[0]).decode("utf-8", "replace"))
