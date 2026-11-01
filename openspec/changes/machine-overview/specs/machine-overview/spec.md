## Purpose
What the manage Overview reports about the machine's harnesses — every resource they carry, where it came from, what it costs in context, how often it is used and whether it can work — limited to what every harness can answer, and the bounds that keep collecting it cheap, private and read only.

## ADDED Requirements

### Requirement: Every machine-level resource of every detected harness is listed
The Overview SHALL list the skills, agents, MCP servers, hooks and instruction files that each detected harness reads at machine level, whether or not uze installed them. A resource a harness reads only inside one project SHALL NOT be listed: the manage modal is global.

#### Scenario: A skill placed by hand
- **WHEN** a skill directory was copied by hand into a harness's own skills directory
- **THEN** it is listed under skills with origin `you`

#### Scenario: A project's skill is left out
- **WHEN** a project declares a skill under its own `.agents/skills/`
- **THEN** the Overview does not list it, whatever directory uze was started in

### Requirement: A menu of kinds beside a table of each
The Overview SHALL offer a menu of sections — uze's own findings while there are any, then skills, agents, mcp servers, hooks and rules, each always present — with each kind's count and how many of it are broken or duplicated, beside a table of the section chosen.

#### Scenario: Opening the Overview
- **WHEN** the operator opens the Overview
- **THEN** the menu lists every kind with its count, and the table shows the first section's resources

#### Scenario: A kind with a broken resource
- **WHEN** one hook is broken
- **THEN** the hooks section carries a mark with that count before reading its table

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
The Overview SHALL count every session each harness still keeps a record of, from the records' metadata or the harness's published statistics, and SHALL say how far back those records reach where the harness lets it be known. A subagent's record SHALL NOT count as a session. A harness whose sessions cannot be counted machine-wide SHALL show its count as unknown.

#### Scenario: A session with subagents
- **WHEN** a session started three subagents, each with its own record
- **THEN** it counts as one session

#### Scenario: Narrowing to one harness
- **WHEN** the operator chooses Codex in the harness field
- **THEN** the header shows Codex's sessions and how many days of records they span

#### Scenario: A harness that deletes old records
- **WHEN** a harness keeps only its last 30 days of sessions
- **THEN** the header says the records reach back 30 days, and no figure claims a longer span

### Requirement: Broken is a fact on disk
A resource SHALL read `broken` only when a fact on disk proves it cannot work: a hook's command file is missing, an MCP server's local command is not found, a skill or agent file cannot be read or its frontmatter is invalid, a plugin's directory is missing. Its reason SHALL be shown in one line.

#### Scenario: A hook whose script was deleted
- **WHEN** a hook's command points at a file that no longer exists
- **THEN** it reads `broken` with the reason `script missing` and the missing path

#### Scenario: A remote MCP server
- **WHEN** an MCP server is reached by URL
- **THEN** it is not judged broken, since nothing on disk can prove it

### Requirement: Status orders each kind
Each resource SHALL carry one status — `broken`, `duplicate`, `unused` or `ok` — and within a kind broken resources SHALL come first, then duplicates, then unused, then the rest by tokens per session, heaviest first. `ok` SHALL be shown as a check only where a fact on disk was checked; a resource nothing on disk can judge SHALL show no check.

#### Scenario: Ordering skills
- **WHEN** the skills group holds one broken skill, one duplicate and three others
- **THEN** the broken one is first, the duplicate second, and the three others follow heaviest first

#### Scenario: A skill nobody used
- **WHEN** no record of any harness that carries a skill holds a use of it, and every one of them could be read
- **THEN** it reads `unused` and comes after the duplicates

#### Scenario: A server reached by URL
- **WHEN** an MCP server is reached by URL and nothing is found against it
- **THEN** its status shows no check

### Requirement: The harness field narrows the screen
The Overview SHALL offer a harness field (every harness, or one) that narrows every row, group, total and use count to that harness. It SHALL NOT grow with the number of harnesses. There SHALL be no period field: each harness keeps its records for a span of its own, and every figure covers all of them.

#### Scenario: Ten harnesses
- **WHEN** ten harnesses are detected
- **THEN** the screen's width and columns are those of four

### Requirement: uze's own findings head the tree
When the health report has findings an operator can act on, the Overview SHALL show them as a first group, open, above the kinds. With none, the group SHALL not be drawn.

#### Scenario: A plugin name already taken
- **WHEN** a harness already holds a plugin with the name uze would deliver
- **THEN** the first group shows that finding and the command that frees the name

### Requirement: The Overview changes nothing outside uze's cache
Collecting and showing the Overview SHALL write nothing outside `$UZE_HOME/cache`, start no MCP server, and run no harness command that changes state. To remove what uze installed it SHALL lead to the Marketplace; for what it did not, it SHALL type the harness's own command into a new shell, unrun, or say that deleting the file removes it.

#### Scenario: Removing a harness plugin
- **WHEN** the operator asks to remove a plugin the harness installed itself
- **THEN** the Overview types the harness's own uninstall command into a new shell and runs nothing

### Requirement: Uses are counted from the harnesses' own records
Each skill, agent and MCP server SHALL show its uses, and the day of the last one, read from every record each harness keeps of its sessions: one the person typed (a slash command, a `$` or `@` mention) and one the model chose (a tool call, the read of a skill's file) alike, subagents' records included. One use SHALL count once however many signs of it a record holds. A failed read of a skill's file SHALL NOT count. No hook SHALL be added to any harness to count.

#### Scenario: A skill typed by the person
- **WHEN** the person typed `/review` in a session and the harness expanded it with no tool call
- **THEN** that skill's uses grow by one

#### Scenario: A mention followed by the call it asked for
- **WHEN** the person mentioned an agent and the model then dispatched it in the same turn
- **THEN** the agent's uses grow by one

#### Scenario: Uses before uze was installed
- **WHEN** the Overview is opened for the first time on a machine with a month of sessions
- **THEN** the uses cover that month

### Requirement: A use inferred rather than recorded is an estimate
Where a harness's record does not name a use but lets it be inferred from what the model wrote (an MCP tool spelled inside a script), the count SHALL be shown as an estimate, with `~`. A harness whose records this build cannot read SHALL show its uses as unknown, and a resource it carries SHALL NOT read `unused`.

#### Scenario: MCP tools called from a script
- **WHEN** a harness records an MCP call only inside the script that made it
- **THEN** that server's uses read with `~`

### Requirement: Uses outlive the harness's own records
When a harness deletes a record uze has read, its uses SHALL be kept, in a record of uze's own that declares its shape, and SHALL go on counting. A record the harness lists again SHALL be read live and never counted twice. Losing the cache SHALL NOT lose them; the header SHALL say since when uses are counted.

#### Scenario: A harness deletes a month-old transcript
- **WHEN** a harness deletes a transcript in which a skill was used twice, after uze read it
- **THEN** the skill's uses still count those two, and the header's date reaches back to them

### Requirement: A record already read is not read again
A record unchanged since the last collection SHALL NOT be opened again, and one only appended to SHALL be read from where the last reading stopped. Losing what was kept between readings SHALL cost only a fresh reading.

#### Scenario: A second opening
- **WHEN** the Overview is opened again after one session grew by ten lines
- **THEN** only those ten lines are read

### Requirement: Only what was used leaves a session's record
Counting sessions SHALL read at most a record's opening metadata and file time. Counting uses SHALL take from a record only which skill, agent or MCP server was used, by whom and when; nothing collected, kept or archived SHALL hold any other text from a session.

#### Scenario: A prompt's words
- **WHEN** a session's prompt carries a phrase alongside a skill's name
- **THEN** the phrase is in neither the snapshot nor anything kept between readings

### Requirement: The first frame never waits on collection
Opening the Overview SHALL draw at once from what was last collected, and a fresh collection SHALL run in the background, marked in flight in the header. No frame SHALL wait on reading a harness's files or running a harness's command.

#### Scenario: Opening with nothing collected
- **WHEN** the Overview is opened with nothing collected yet
- **THEN** the screen draws immediately and fills in as each harness answers

### Requirement: The workspace collects in the background, and nothing else does
While the workspace is open it SHALL collect in the background, on a thread of its own, shortly after it opens and every 15 minutes from then on, one collection at a time across every process. The Overview SHALL draw what was last collected, collect itself only when that is older than the interval, and offer a refresh, a key and a click alike. Nothing SHALL be collected by a CLI command, the runtime shim, a harness's launch or an agent's turn.

#### Scenario: Opening the Overview after a background collection
- **WHEN** the operator opens the Overview five minutes after the workspace collected
- **THEN** it draws that collection, says when it was made, and reads no harness

#### Scenario: Two workspaces open
- **WHEN** two workspaces are open on one machine
- **THEN** only one of them reads the harnesses at a time

#### Scenario: Launching an agent
- **WHEN** an agent is launched in the workspace
- **THEN** its launch waits on nothing collected for the Overview

### Requirement: A harness that cannot answer degrades alone
A harness whose integration reports no inventory, no session count or no uses, or whose files this build cannot parse, SHALL show that part as unknown while every other harness is reported in full. A new harness SHALL need no change to the Overview to appear in it.

#### Scenario: A config format changed by a harness release
- **WHEN** a harness release changes a config file's format
- **THEN** that harness's part reads unknown, the others are unaffected, and nothing fails

### Requirement: The Overview is offered once complete
The Overview route SHALL be offered in every build once this capability is complete, and SHALL no longer depend on the `overview` feature flag.

#### Scenario: A release build
- **WHEN** the manage modal opens in a release build
- **THEN** Overview is its first tab
