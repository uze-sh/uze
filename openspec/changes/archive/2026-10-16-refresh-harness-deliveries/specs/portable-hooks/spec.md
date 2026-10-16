## MODIFIED Requirements

### Requirement: Command hooks use one portable ABI
The system SHALL hand a command handler the normalized hook context as
`HOOK_*` environment and SHALL read its decision from its exit code: `0`
allows, the canonical deny exit denies with the reason on stderr, and any
other status is a handler failure resolved by the group's effect. The reason
read back SHALL be bounded, and each handler SHALL be bounded by its declared
timeout. It SHALL support observation, allow, deny, ask and input
transformation where the target supports them, and SHALL report a group
whose effect the target cannot preserve instead of attaching it. Where the
target offers a native way to block a call whose hook cannot be started or
exceeds the target's own timeout, a deny, ask or transform group SHALL be
delivered with it, so the group fails closed even when the delivered
handler itself never runs; an observational group SHALL NOT be.

#### Scenario: First deny wins in deterministic order
- **WHEN** one group declares several pre-tool command handlers
- **THEN** they run sequentially in manifest order
- **AND** a deny stops the later handlers and the intercepted operation where the target supports blocking

#### Scenario: A deny group whose delivered handler is gone
- **WHEN** a deny group is delivered to a target that offers native block-on-failure and the delivered handler file is then deleted
- **THEN** the intercepted call is blocked, not allowed

#### Scenario: An observational group whose handler is gone
- **WHEN** an observational group's delivered handler cannot be started
- **THEN** the intercepted call proceeds
