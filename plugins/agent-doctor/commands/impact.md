---
description: "Show which tests are impacted by the current diff (agent-doctor impact). Optional argument is the base ref, default main."
---

Determine the tests reaching the current change. Use Bash:

```sh
agent-doctor impact --base "${ARGUMENTS:-main}"
```

Run only those tests rather than the whole suite. If a dynamic-import caveat is reported,
also run the project's smoke/always-run tests.
