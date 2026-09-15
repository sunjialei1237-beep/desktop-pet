"""Split a unified diff into per-hunk patches; keep only hunks that belong to
the startup-bubble fix (welcomeTimerRef / WELCOME_FALLBACKS / app-status guards),
leaving the parallel F2 edit-proposal session's hunks unstaged."""
import sys

path = sys.argv[1]
out_path = sys.argv[2] if len(sys.argv) > 2 else None
mine_markers = [
    "welcomeTimerRef",
    "WELCOME_FALLBACKS",
    "与其它问候监听器同一套守卫",
    "重启问候统一走后端协调",
]
their_markers = [
    "EditProposalInfo",
    "EditApplyOutcome",
    "handleApplyEdit",
    "handleUndoEdit",
    "editProposal",
    "editOutcome",
    "edit-confirm",
    "edit_proposal",
    "setEditProposal",
    "setEditOutcome",
]

raw = open(path, "rb").read()
if raw.startswith(b"\xff\xfe") or raw.startswith(b"\xfe\xff"):
    text = raw.decode("utf-16")
else:
    text = raw.decode("utf-8")
hunks = []
cur = []
for line in text.splitlines(keepends=True):
    if line.startswith("@@"):
        if cur:
            hunks.append(cur)
        cur = [line]
    else:
        cur.append(line)
if cur:
    hunks.append(cur)

kept = []
for i, h in enumerate(hunks):
    body = "".join(h)
    if not h[0].startswith("@@"):
        # diff --git / index / --- / +++ header lines — always required.
        kept.append(body)
        continue
    is_mine = any(m in body for m in mine_markers)
    is_theirs = any(m in body for m in their_markers)
    print(f"hunk {i}: mine={is_mine} theirs={is_theirs} :: {body.splitlines()[0][:80]}", file=sys.stderr)
    if is_mine == is_theirs:
        # must be unambiguous
        raise SystemExit(f"ambiguous hunk {i}:\n{body[:400]}")
    if is_mine:
        kept.append(body)

out = "".join(kept)
print("kept", len(kept), "of", len(hunks), "hunks", file=sys.stderr)
if out_path:
    open(out_path, "wb").write(out.encode("utf-8"))
else:
    sys.stdout.buffer.write(out.encode("utf-8"))