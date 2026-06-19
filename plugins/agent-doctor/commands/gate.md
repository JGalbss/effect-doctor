---
description: "Gate the current diff against policy, ACL, and leases (agent-doctor gate). Optional argument is the actor id."
---

Check the current change against the repo's policy. Use Bash:

```sh
agent-doctor gate --base main ${ARGUMENTS:+--actor "$ARGUMENTS"}
```

If it reports violations, they are ground truth — fix the change to comply (don't edit the
policy to get around it unless that is the actual task).
