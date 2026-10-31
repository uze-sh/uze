## Purpose
What the manage Overview reports about the machine's harnesses — every resource they carry, where it came from, what it costs per session and how often it is used — and the bounds that keep collecting it cheap, private and read only.

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
The Overview SHALL group resources by kind — skills, agents, mcp servers, hooks, rules — in one tree that opens folded. Each group row SHALL carry the count of its resources, their uses, their tokens per session and how many are broken or unused, so the folded tree is the summary.

#### Scenario: Opening the Overview
- **WHEN** the operator opens the Overview for the first time
- **THEN** every kind is one folded row with its totals, and no resource row is shown until a group is unfolded

### Requirement: Every resource names its origin
Each resource SHALL name who manages it: `uze` when a uze receipt owns it, the harness plugin's name when one brought it, `built-in` when the harness ships it, the account when it is an account connector, or `you` when it was placed by hand. Ownership by uze SHALL be decided from uze's receipts, never from a path's shape.

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
A resource uze delivered to several harnesses SHALL be listed once, with its uses broken down by harness. Resources uze did not deliver SHALL be merged only when their kind, name and content are the same.

#### Scenario: A uze skill on three harnesses
- **WHEN** one uze skill reaches Claude Code, Codex and OpenCode
- **THEN** the tree shows one row, and its detail lists the three harnesses with their uses

### Requirement: Tokens per session are what the harness lists
Each resource SHALL report the tokens it adds to every session, estimated locally and shown as an estimate: what the harness lists of it (a skill's or agent's name and description, an MCP server's tool names as listed), an always-on file whole, a hook's observed injected context. A figure that cannot be known offline SHALL read unknown.

#### Scenario: An MCP server whose tools load on demand
- **WHEN** a harness lists an MCP server's tool names and loads their schemas only when used
- **THEN** its tokens per session are the estimate of the listed names

#### Scenario: Offline, no figure is fetched
- **WHEN** the machine is offline or a server is not running
- **THEN** every figure is shown or reads unknown, and no server is started to measure it

### Requirement: Tokens when used are shown when known
A skill, agent or instruction file that loads more when it is used SHALL report that part's tokens. A resource whose loaded part cannot be read from disk SHALL read unknown, and one that adds nothing more SHALL say so.

#### Scenario: A skill's body
- **WHEN** a skill is selected
- **THEN** its detail shows the tokens of its description per session and the tokens of the rest of its file when used

### Requirement: Sessions are counted per harness
The Overview SHALL count each harness's sessions with activity in the chosen period, from the harness's own records. A subagent's record SHALL NOT count as a session. A harness whose sessions cannot be counted machine-wide SHALL show its count as unknown.

#### Scenario: Narrowing to one harness
- **WHEN** the operator chooses Codex in the harness field
- **THEN** the header shows Codex's sessions in the period

#### Scenario: A session with subagents
- **WHEN** a session started three subagents, each with its own record
- **THEN** it counts as one session, and the subagents' uses count toward the resources they used

### Requirement: Uses are counted per resource and harness
For each resource the Overview SHALL count, per harness, how often it was used in the period, from the harness's own records. A use SHALL count only when it resolves to a listed resource. Where a harness keeps no readable record the count SHALL be unknown, never zero.

#### Scenario: A harness whose records are unreadable
- **WHEN** a resource is read only by a harness whose session records this build cannot read
- **THEN** its uses are unknown and its status is `unknown`, not `unused`

#### Scenario: A name that is no resource
- **WHEN** a session invokes a skill name that no harness carries
- **THEN** nothing is counted for it

#### Scenario: A rule always loaded
- **WHEN** an instruction file loads in every session
- **THEN** its uses are that harness's sessions in the period and its status is `in use`

### Requirement: A partial signal is shown as a floor
Where a harness records only some uses of a kind, such as explicit invocations alone, that harness's count SHALL be shown as a floor (`≥`), and a zero from it SHALL NOT make a resource `unused`.

#### Scenario: Skills read without being invoked
- **WHEN** a harness records a skill only when it is invoked explicitly
- **THEN** its count reads `≥ 3`, and a skill with no record there reads `unknown` for that harness

### Requirement: Status orders each kind
Each resource SHALL carry one status: `broken` when the harness cannot use it, `unused` when every count is complete and zero for the period, `unknown` when no count is readable, otherwise `in use`. Within a kind, broken comes first, then unused, then the rest by tokens per session, heaviest first.

#### Scenario: An MCP server that needs signing in
- **WHEN** an MCP server is configured but the harness reports it needs authentication or failed to start
- **THEN** its status is `broken`, its reason is shown in one line, and it is first in its group

### Requirement: Nothing reads unused before its period is read
A resource SHALL NOT read `unused` until a pass has read every record of its period. Until then its uses SHALL read as being read.

#### Scenario: Leaving mid-read
- **WHEN** a pass is cancelled before reaching every record of the period
- **THEN** no resource reads `unused` from it, and the next pass resumes where it stopped

### Requirement: Harness and period narrow the screen
The Overview SHALL offer a harness field (every harness, or one) and a period field (7, 30 or 90 days) whose values narrow every row, group and total. Neither field SHALL grow with the number of harnesses.

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

### Requirement: The first frame never waits on collection
Opening the Overview SHALL draw at once from what was last collected. Inventory and tokens SHALL appear before uses; uses SHALL fill in from a background read marked in flight in the header, and no frame SHALL wait on reading a harness's records.

#### Scenario: Opening with a cold cache
- **WHEN** the Overview is opened with nothing collected yet
- **THEN** the screen draws immediately, lists the inventory as it arrives, and shows uses as being read

### Requirement: Collection runs only while the Overview is in front
Reading harness records SHALL run only while the Overview is the screen in front, or on its explicit refresh, and SHALL stop when another screen is chosen or the modal closes. It SHALL never run at uze's launch, a harness's launch, in the runtime shim or in any CLI command.

#### Scenario: Opening the modal on another screen
- **WHEN** the operator opens the manage modal straight to the Marketplace
- **THEN** no harness record is read

#### Scenario: Launching an agent
- **WHEN** an agent is launched in the workspace
- **THEN** no harness record is read for the Overview

### Requirement: Reading is incremental and bounded
Uses SHALL be counted from what each record gained since it was last read, kept per record in a rebuildable index in the `cache/` tier, so a record re-read from its start never counts twice. A pass SHALL hold a bounded amount of memory whatever a record's size, and only one pass SHALL run at a time.

#### Scenario: A second opening
- **WHEN** the Overview is opened again and only one session record grew
- **THEN** only that record's new part is read

#### Scenario: A record rewritten
- **WHEN** a record became shorter than its stored position
- **THEN** its earlier counts are dropped and it is read again from its start

### Requirement: Only names and counts are kept
The index SHALL keep only resources listed in the inventory, their kinds, harnesses, counts, timestamps and read positions, and never the content of a session: no prompt, output, argument, command or name a session typed.

#### Scenario: A sentinel in a session
- **WHEN** a session contains an invented skill name and a hook's output
- **THEN** neither appears in the index

### Requirement: A harness that cannot answer degrades alone
A harness whose integration reports no inventory, no session count or no usage, or whose records this build cannot parse, SHALL show that part as unknown while every other harness is reported in full. A new harness SHALL need no change to the Overview to appear in it.

#### Scenario: A record format changed by a harness release
- **WHEN** a harness release changes its session record format
- **THEN** that harness's uses become unknown, the others are unaffected, and nothing fails

### Requirement: The Overview is offered once complete
The Overview route SHALL be offered in every build once this capability is complete, and SHALL no longer depend on the `overview` feature flag.

#### Scenario: A release build
- **WHEN** the manage modal opens in a release build
- **THEN** Overview is its first tab
