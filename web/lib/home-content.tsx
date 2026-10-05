import type { ReactNode } from 'react';

// The landing page's words, apart from its layout: the home page and the
// design explorations under /lab say the same thing, so a variant is judged
// on how it looks rather than on copy that drifted.

// Said in the reader's words, not the architecture's: what each half does for
// them, and the one guarantee under it. The mechanism is a link away.
export const pillars = [
  {
    title: 'Install once, every agent gets it',
    body: 'A plugin is skills, MCP servers, hooks and subagents in the open Agent Plugin format. uze keeps one copy and puts it into each agent the way that agent expects: a real plugin where it has one, its own config where it does not, and a report wherever something had to be translated.',
    href: '/docs/plugins/delivery',
    link: 'How delivery works',
  },
  {
    title: 'A project carries its own setup',
    body: 'agents.yaml says which plugins a project uses, agents.lock pins them to a commit, and one AGENTS.md holds the instructions every agent reads. A teammate clones the repo, runs uze install, and has the same setup in every agent they use.',
    href: '/docs/plugins/context',
    link: 'Project files and context',
  },
  {
    title: 'Agents side by side, never on top of each other',
    body: 'Each agent runs in its own git worktree, on a branch of its own, so two of them never edit one checkout and your uncommitted work stays out of their way. Whether one is done is read from its branch, not from what it says.',
    href: '/docs/workspace',
    link: 'Inside the workspace',
  },
  {
    title: 'Your files stay yours',
    body: 'uze writes only inside blocks it marks and keeps a receipt for everything it places, so it can update or remove a plugin without touching what you or another tool put next to it. When it finds something it does not own, it reports it and leaves it alone.',
    href: '/docs/concepts',
    link: 'Why uze works this way',
  },
];

// The questions a developer asks in the first minute, answered before they
// have to open the docs. "Not an agent, no API key" is said here and in the
// hero's badge and nowhere else: repeated in every section it reads as
// protesting too much.
export const faq: { q: string; a: ReactNode }[] = [
  {
    q: 'Is uze another coding agent?',
    a: 'No. uze has no model and needs no API key: your agents keep the logins and subscriptions they already have. uze installs plugins into them and runs them, and you talk to each one exactly as you do today.',
  },
  {
    q: 'Does it send my code anywhere?',
    a: 'Your code goes only where your agents already send it. uze itself fetches the marketplaces you registered, and checks GitHub for a new release. Release builds contain no telemetry.',
  },
  {
    q: 'Why not symlink one skills folder into every agent?',
    a: 'Because they do not read the same formats. Codex has no Markdown agent format, so a subagent becomes TOML there; a skill only you may run is written differently in each agent; hooks run through a different mechanism in each. uze also keeps a receipt for everything it places, which is what lets it remove a plugin cleanly later.',
  },
  {
    q: 'Why not just use Claude Code’s own plugins?',
    a: 'On Claude Code, uze delivers through Claude Code’s own plugin mechanism, so nothing is lost. What you add is the same plugin reaching Codex, OpenCode and Antigravity, and a project that records it, so every teammate gets it too.',
  },
  {
    q: 'Do I need the workspace to use the plugins?',
    a: 'No. They are two tools in one binary and each works without the other. The package manager serves agents you start yourself, in a terminal, an editor or CI.',
  },
  {
    q: 'What does the workspace add to tmux and git worktree?',
    a: 'Each agent gets a tab, a branch and a worktree of its own without you creating any of them. You see its diff a keystroke away, and when the work is ready one key rebases it, runs your checks, and leaves the branch for you, merges it or opens a pull request. Closing the terminal stops nothing.',
  },
  {
    q: 'Can a plugin run code on my machine?',
    a: 'Yes: MCP servers and hooks are commands. Before installing, uze lists every command a plugin can run and asks, and asks again when an update adds one. Plugins are fetched without your Git credentials and never run a repository’s own hooks.',
  },
  {
    q: 'Does it work on Windows?',
    a: (
      <>
        Yes, natively, on Windows 10 22H2 and 11, x64 or Arm: install with{' '}
        <code className="font-mono text-ink">irm https://uze.sh/i | iex</code>. It needs Git for
        Windows, and WSL works with the Linux command too.
      </>
    ),
  },
  {
    q: 'What does it cost?',
    a: 'Nothing. uze is open source under the Apache License 2.0, and it is in beta: commands and file formats may still change before v1.',
  },
];
