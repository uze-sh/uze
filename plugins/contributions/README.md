# contributions

A minimal contribution graph of the repository, as GitHub and GitLab draw
one, read from git: today's commits and the last two
weeks in the bar, and a map of the last twenty weeks where every day is a
click away from its commits.

<p align="center">
  <img src="../../web/public/examples/contributions.gif" alt="The contribution graph opens from the bar; a click on a day lists its commits" width="100%" />
</p>

| Where | What |
|---|---|
| bar | today's commits and the last 14 days in the map's tones |
| drawer | floating along the bottom, over the agent: the commits, active days and streaks, the map with its months and weekdays, the chosen day's commits and who made them |

Each day's tone is one of the theme's five heat steps (`heat-0` to
`heat-4`), by the day's place among the days that had a commit, so the map
reads the same for a commit a week and for a hundred a day.

**It may:** run `git`, confined (`exec:git`), and remember the chosen day
(`store`).

## How it is built

- [`extension.kdl`](extensions/contributions/extension.kdl): a bar item that opens
  a floating drawer, a source refreshed on `checkout.changed`, and an action a click
  on a day calls.
- [`heatmap.uze`](extensions/contributions/heatmap.uze): the drawer's view, built on
  `Cells` with a click region per day, and the script that reads one
  `git log`.
- [`today.uze`](extensions/contributions/today.uze): the bar's line.

Try it with the other examples: `/uze:try` in any agent, or `make try` in
a uze checkout.
