## Purpose

Which hosts an extension may reach, and how a request is made.

## ADDED Requirements

### Requirement: A request reaches only a host the extension was granted
`fetch` SHALL refuse any URL that is not `https` or whose host is not named
by a `net:<host>` grant, SHALL not follow a redirect to another host, and
SHALL run as a program under the run's time and output bounds.

#### Scenario: Another host is refused
- **WHEN** an extension granted `net:api.github.com` fetches `https://example.com/`
- **THEN** nothing is requested and the answer names the host it may not reach
