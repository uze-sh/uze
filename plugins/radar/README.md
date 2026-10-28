# radar

What the agents are doing: a section listing every agent and how long its
turn has run, and the last events over everything, opened from the section. A turn that
ends plays a sound, and an agent that asks for approval notifies you, in
the sidebar's notice where a new release shows.

<p align="center">
  <img src="../../web/public/examples/radar.gif" alt="An agent works while the sidebar counts its turn; and its events list the tools it used and the files they touched" width="860" />
</p>

| Where | What |
|---|---|
| sidebar, notice | an agent asking for approval, until you put it away |
| sidebar section | every agent: a spinner and its turn's time, or how it stopped |
| overlay, from the section's `events` | the last 40 events, newest first: each tool and the file it touched |

**It may:** observe the agents (`agent:observe`) and notify you
(`notify`). The sound and the approval notice are preferences, in the
Extensions screen.

## How it is built

- [`extension.kdl`](extensions/radar/extension.kdl): three surfaces, the
  workspace's own `agents` and `events` sources, and an action the
  section's button runs to open the overlay. It suggests no key: bind one
  on the Keys screen if you want it.
- [`notice.uze`](extensions/radar/notice.uze): a script asked once per
  event (`deliver "each"`), answering the sound and the notification as
  the preferences say. Its status row draws nothing: it stands so the
  source is asked.

Try it with the other examples: `/uze:try` in any agent, or `make try` in
a uze checkout.
