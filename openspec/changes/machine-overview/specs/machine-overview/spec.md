## Purpose
What the manage Overview reports about the machine's harnesses — every resource they carry, where it came from, what it costs in context and whether it can work — limited to what every harness can answer, and the bounds that keep collecting it cheap, private and read only.

## ADDED Requirements

### Requirement: Every machine-level resource of every detected harness is listed
The Overview SHALL list the skills, agents, MCP servers, hooks and instruction files that each detected harness reads at machine level, whether or not uze installed them. A resource a harness reads only inside one project SHALL NOT be listed: the manage modal is global.

#### Scenario: A skill placed by hand
- **WHEN** a skill directory was copied by hand into a harness's own skills directory
- **THEN** it is listed under skills with origin `you`

#### Scenario: A project's skill is left out
- **WHEN** a project declares a skill under its own `.agents/skills/`
- **THEN** the Overview does not list it, whatever directory uze was started in

### Requirement: Resources are grouped by kind in one tree
The Overview SHALL group resources by kind — skills, agents, mcp servers, hooks, rules — in one tree that opens folded. Each group row SHALL carry the count of its resources, their tokens per session and how many are broken or duplicated, so the folded tree is the summary.

#### Scenario: Opening the Overview
- **WHEN** the operator opens the Overview for the first time
- **THEN** every kind is one folded row with its totals, and no resource row is shown until a group is unfolded

### Requirement: Every resource names its origin
Each resource SHALL name who manages it: `uze` when a uze receipt owns it, the harness plugin's name when one brought it, `built-in` when the harness ships it, or `you` when it was placed by hand. Ownership by uze SHALL be decided from uze's receipts, never from a path's shape.

#### Scenario: A uze skill delivered to a harness
- **WHEN** a skill reaches Claude Code through a receipt uze recorded
- **THEN** its origin is `uze`

#### Scenario: A plugin of the harness's own marketplace
- **WHEN** a skill was brought by a plugin the harness installed itself
- **THEN** its origin is that plugin's name

#### Scenario: A skill the harness ships
- **WHEN** a harness ships skills of its own
- **THEN** they are listed with origin `built-in`

### Requirement: One resource reaching several harnesses is one row
A resource uze delivered to several harnesses SHALL be listed once, naming every harness it reaches. Resources uze did not deliver SHALL be merged only when their kind, name and content are the same.

#### Scenario: A uze skill on three harnesses
- **WHEN** one uze skill reaches Claude Code, Codex and OpenCode
- **THEN** the tree shows one row, and its detail names the three harnesses

### Requirement: Same name with different content is a duplicate
Resources of the same kind and name whose content differs SHALL each be marked `duplicate`, and each SHALL name where the others are.

#### Scenario: Two copies of a skill
- **WHEN** a skill named `review` exists by hand in Codex and, with other content, in a Claude Code plugin
- **THEN** both rows read `duplicate` and each detail names the other's harness and path

### Requirement: Tokens per session are measured on the artifact's files
Each skill, agent and instruction file SHALL report the tokens it adds to every session: a skill's or agent's name and description as written in its file, an always-on instruction file whole. They SHALL be estimated locally, without network, and shown as an estimate. MCP servers and hooks SHALL show no per-session figure in this version.

#### Scenario: A skill's description
- **WHEN** a skill's frontmatter holds a 300-character description
- **THEN** its tokens per session are the estimate of its name and description, shown with `~`

#### Scenario: An MCP server
- **WHEN** an MCP server is selected
- **THEN** its detail shows no per-session token figure and the group's total does not count it

### Requirement: Tokens when used are measured on the artifact's files
A skill, agent or instruction file that loads more when it is used SHALL report that part's tokens, estimated from its files. A resource that adds nothing more SHALL say so.

#### Scenario: A skill's body
- **WHEN** a skill is selected
- **THEN** its detail shows the tokens of its description per session and the tokens of the rest of its file when used

### Requirement: Sessions are counted per harness
The Overview SHALL count each harness's sessions with activity in the chosen period (7, 30 or 90 days), from the records' metadata or the harness's published statistics. A subagent's record SHALL NOT count as a session. A harness whose sessions cannot be counted machine-wide SHALL show its count as unknown.

#### Scenario: A session with subagents
- **WHEN** a session started three subagents, each with its own record
- **THEN** it counts as one session

#### Scenario: Narrowing to one harness
- **WHEN** the operator chooses Codex in the harness field
- **THEN** the header shows Codex's sessions in the period

### Requirement: Broken is a fact on disk
A resource SHALL read `broken` only when a fact on disk proves it cannot work: a hook's command file is missing, an MCP server's local command is not found, a skill or agent file cannot be read or its frontmatter is invalid, a plugin's directory is missing. Its reason SHALL be shown in one line.

#### Scenario: A hook whose script was deleted
- **WHEN** a hook's command points at a file that no longer exists
- **THEN** it reads `broken` with the reason `script missing` and the missing path

#### Scenario: A remote MCP server
- **WHEN** an MCP server is reached by URL
- **THEN** it is not judged broken, since nothing on disk can prove it

### Requirement: Status orders each kind
Each resource SHALL carry one status — `broken`, `duplicate` or `ok` — and within a kind broken resources SHALL come first, then duplicates, then the rest by tokens per session, heaviest first.

#### Scenario: Ordering skills
- **WHEN** the skills group holds one broken skill, one duplicate and three others
- **THEN** the broken one is first, the duplicate second, and the three others follow heaviest first

### Requirement: Harness and period narrow the screen
The Overview SHALL offer a harness field (every harness, or one) that narrows every row, group and total, and a period field that sets the period sessions are counted over. Neither field SHALL grow with the number of harnesses.

#### Scenario: Ten harnesses
- **WHEN** ten harnesses are detected
- **THEN** the screen's width and columns are those of four

### Requirement: uze's own findings head the tree
When the health report has findings an operator can act on, the Overview SHALL show them as a first group, open, above the kinds. With none, the group SHALL not be drawn.

#### Scenario: A plugin name already taken
- **WHEN** a harness already holds a plugin with the name uze would deliver
- **THEN** the first group shows that finding and the command that frees the name

### Requirement: The Overview changes nothing outside uze's cache
Collecting and showing the Overview SHALL write nothing outside `$UZE_HOME/cache`, start no MCP server, and run no harness command that changes state. To remove what uze installed it SHALL lead to the Marketplace; for what it did not, it SHALL show the harness's own command, or that deleting the file removes it.

#### Scenario: Removing a harness plugin
- **WHEN** the operator asks to remove a plugin the harness installed itself
- **THEN** the Overview offers to copy the harness's own uninstall command and runs nothing

### Requirement: No session's content is read
Counting sessions SHALL read at most a record's opening metadata, never what a session said, and nothing collected SHALL keep text from any session.

#### Scenario: Counting Codex sessions
- **WHEN** Codex's sessions are counted
- **THEN** only each record's opening metadata and file time are read

### Requirement: The first frame never waits on collection
Opening the Overview SHALL draw at once from what was last collected, and a fresh collection SHALL run in the background, marked in flight in the header. No frame SHALL wait on reading a harness's files or running a harness's command.

#### Scenario: Opening with nothing collected
- **WHEN** the Overview is opened with nothing collected yet
- **THEN** the screen draws immediately and fills in as each harness answers

### Requirement: Collection runs only while the Overview is in front
Collection SHALL run only while the Overview is the screen in front, or on its explicit refresh, and SHALL stop when another screen is chosen or the modal closes. It SHALL never run at uze's launch, a harness's launch, in the runtime shim or in any CLI command.

#### Scenario: Opening the modal on another screen
- **WHEN** the operator opens the manage modal straight to the Marketplace
- **THEN** no harness file is read for the Overview

#### Scenario: Launching an agent
- **WHEN** an agent is launched in the workspace
- **THEN** nothing is collected for the Overview

### Requirement: A harness that cannot answer degrades alone
A harness whose integration reports no inventory or no session count, or whose files this build cannot parse, SHALL show that part as unknown while every other harness is reported in full. A new harness SHALL need no change to the Overview to appear in it.

#### Scenario: A config format changed by a harness release
- **WHEN** a harness release changes a config file's format
- **THEN** that harness's part reads unknown, the others are unaffected, and nothing fails

### Requirement: The Overview is offered once complete
The Overview route SHALL be offered in every build once this capability is complete, and SHALL no longer depend on the `overview` feature flag.

#### Scenario: A release build
- **WHEN** the manage modal opens in a release build
- **THEN** Overview is its first tab
