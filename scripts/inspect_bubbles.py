import sqlite3, sys

sys.stdout.reconfigure(encoding="utf-8")
db_path = r"C:\Users\SunJialei\AppData\Roaming\DesktopPet\desktop_pet.db"
db = sqlite3.connect(db_path)
cur = db.cursor()
cur.execute("SELECT name FROM sqlite_master WHERE type='table'")
print("tables:", [r[0] for r in cur.fetchall()])
try:
    cur.execute("PRAGMA table_info(bubble_log)")
    print("bubble_log cols:", [r[1] for r in cur.fetchall()])
    cur.execute("SELECT * FROM bubble_log ORDER BY id DESC LIMIT 40")
    for row in cur.fetchall():
        print(repr(row))
except Exception as e:
    print("ERR", e)
db.close()