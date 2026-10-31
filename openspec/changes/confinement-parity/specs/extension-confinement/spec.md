## ADDED Requirements

### Requirement: Windows confines a program in a container of its own
On Windows a program SHALL run in an AppContainer made for that run and
removed after it, inside a Job Object. It SHALL read, execute and write
only beneath the paths its confinement grants and what Windows gives
every app container (the system and installed programs). It SHALL NOT
read the operator's profile, another process, or a named pipe.

#### Scenario: A read outside the grant is denied
- **WHEN** on Windows a confined program reads `%USERPROFILE%\.ssh\id_ed25519` or a file beside the checkout
- **THEN** the read fails with access denied

#### Scenario: A write outside the writable paths is denied
- **WHEN** on Windows a confined program writes into the checkout it may only read
- **THEN** the write fails and the file is unchanged

#### Scenario: A read grant executes nothing
- **WHEN** a program it was granted to read is started from a read-only path
- **THEN** starting it fails

#### Scenario: Another process is out of reach
- **WHEN** a confined program opens the process that started it to read its memory or end it
- **THEN** the open fails

### Requirement: Windows reaches the network only when granted
Without network, a confined program on Windows SHALL reach no host and
resolve no name, loopback included. With network it SHALL reach Internet
hosts and still no loopback service, named pipe or Unix socket. No
connection from outside SHALL reach a port it listens on.

#### Scenario: No network reaches nothing
- **WHEN** a program confined without network connects to `1.1.1.1:443` or to a service listening on `127.0.0.1`
- **THEN** both connections fail

#### Scenario: Network reaches the Internet and no local service
- **WHEN** a program confined with network connects to an Internet host and to a service on `127.0.0.1`
- **THEN** the first succeeds and the second fails

### Requirement: A Windows run is bounded and ends whole
A confined run on Windows SHALL be refused memory beyond its address-space
limit and processor time beyond its CPU limit. Ending the run SHALL end
every process it started.

#### Scenario: Memory past the bound is refused
- **WHEN** a program bounded to 2 GiB commits 3 GiB
- **THEN** the allocation fails or the program ends, and the machine is unaffected

### Requirement: What a machine cannot enforce is named as an outcome
`uze doctor`, `uze status` and the Extensions screen SHALL name each right a
machine does not enforce by what the program could then do, never by a
mechanism another platform has. Every platform SHALL answer every right.

#### Scenario: Windows names its gaps by outcome
- **WHEN** the operator runs `uze doctor` on Windows
- **THEN** the report says programs run confined by AppContainer, without a file size or open files limit

#### Scenario: A Linux kernel before ABI 6 names what it lacks
- **WHEN** the kernel answers Landlock ABI 3
- **THEN** the summary says a program can still reach ungranted devices and signal other processes, rather than naming the Landlock ABI that would refuse them
