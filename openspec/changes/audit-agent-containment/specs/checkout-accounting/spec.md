## ADDED Requirements

### Requirement: An agent's claim is proven by its launch key, and absent proof is no proof
A process SHALL be taken for a recorded agent only when it offers the secret
whose digest that record holds. A record holding no digest SHALL admit no
claim. A subagent's record SHALL be issued a key when it is split, which no
process is given, so no process can act as the subagent through the agent
verbs.

#### Scenario: A claim names a subagent with a guessed key
- **WHEN** a process sets `UZE_AGENT` to a subagent's identifier and `UZE_AGENT_KEY` to anything, inside that subagent's checkout, and runs `uze agent work split`
- **THEN** it is refused as not an agent UZE launched, and no checkout is made

#### Scenario: A record from before launch keys
- **WHEN** a record holding no key digest is named by a claim with any key
- **THEN** the claim is refused, and the agent's next placement issues it a key
