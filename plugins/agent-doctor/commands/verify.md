---
description: "Gate the working diff and run the impacted tests (agent-doctor verify). Optional argument is the test command to run, e.g. npx vitest run."
---

Run the agent-doctor pre-submit check on the current diff. Use Bash:

```sh
agent-doctor verify ${ARGUMENTS:+--run "$ARGUMENTS"}
```

Report the result concisely. If it exits non-zero, summarize the policy/lease violations or
the failing impacted tests and propose a fix. Do not bypass the gate.
