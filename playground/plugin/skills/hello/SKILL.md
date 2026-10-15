---
name: hello
description: "Greets the world. Only the person can invoke it, never the model on its own."
invoke:
  model: false
  user: true
---
Answer with exactly:

`hello, world (UZE_PLAYGROUND_SKILL: hello)`

If the invocation carried a name, greet that name instead of `world`.
