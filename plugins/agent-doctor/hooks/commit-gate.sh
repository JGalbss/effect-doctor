#!/bin/sh
# PreToolUse(Bash) hook: when the agent runs a commit / push / submit, gate the
# diff with `agent-doctor verify`. Blocks (exit 2) ONLY on a real verify failure
# (exit 1 = policy/lease violation or failing impacted test). Anything else
# (binary not installed, no base ref) is allowed through — the hook never gets in
# the way except when there's a genuine, deterministic reason to.

input=$(cat)
cmd=$(printf '%s' "$input" | python3 -c 'import sys,json
try: print(json.load(sys.stdin).get("tool_input",{}).get("command",""))
except Exception: print("")' 2>/dev/null)

case "$cmd" in
  *"git commit"*|*"git push"*|*"gt submit"*|*"gt s "*) ;;
  *) exit 0 ;;   # not a commit/submit — allow
esac

# Prefer an installed binary; fall back to npx (zero-setup) if present.
if command -v agent-doctor >/dev/null 2>&1; then
  RUN="agent-doctor"
elif command -v npx >/dev/null 2>&1; then
  RUN="npx -y @jgalbsss/agent-doctor"
else
  exit 0   # toolkit unavailable — don't block
fi

out=$($RUN verify 2>&1)
code=$?
if [ "$code" = "1" ]; then
  printf 'agent-doctor verify blocked this commit/submit:\n%s\n' "$out" >&2
  exit 2   # block the tool call
fi
exit 0
