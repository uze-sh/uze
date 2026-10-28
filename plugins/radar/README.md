# radar

The agents at a glance, from anywhere: one item in the bar says whether one
is waiting on you, how many are working and for how long, or how long they
have all been idle. A click opens what they did, tool by tool. A turn that
ends plays a sound, and an agent that asks for approval notifies you, in
the sidebar's notice where a new release shows.

<p align="center">
  <img src="../../web/public/examples/radar.gif" alt="The bar says two agents are working and for how long; a click opens the drawer of what they did, each tool and the file it touched" width="860" />
</p>

| Where | What |
|---|---|
| bar | `! 1 awaiting approval`, or `⠙ 2 working 2m05s`, or `✓ idle 5m00s` |
| drawer | the last 40 events, newest first: each tool and the file it touched |
| sidebar, notice | an agent asking for approval, until you put it away |

It lists no agent: the sidebar already does. The sound and the approval
notice are preferences, in the Extensions screen.

**It may:** observe the agents (`agent:observe`), notify you (`notify`),
and keep which agents are working in UZE's state (`store`).

## How it is built

- [`extension.kdl`](extensions/radar/extension.kdl): a bar item that opens
  a drawer, the workspace's own `events` source, and a script source asked
  once per agent event (`deliver "each"`).
- [`now.uze`](extensions/radar/now.uze): the bar's line, and the script
  that keeps which agents are working and which ask for approval in the
  store, and answers the sound and the notification.

Try it with the other examples: `/uze:try` in any agent, or `make try` in
a uze checkout.
