## MODIFIED Requirements

### Requirement: Every durable thing belongs to one tier, and the tier says what losing it costs
UZE SHALL keep what it persists in tiers distinguished by what deleting
them costs, and each thing SHALL belong to exactly one: **bytes** (the
package sources), **store** (what each harness reads), **record** (what UZE
was told or decided), **authored** (the operator's own text), **generated**
(what UZE produced for itself) and **remembered** (what UZE observed to go
faster). A thing SHALL NOT sit in a tier that claims a different cost than it
has; content UZE renders for a harness SHALL NOT sit beside the records.

#### Scenario: Deleting the bytes
- **WHEN** the operator deletes the package sources
- **THEN** the packages cannot be rebuilt until they are acquired again
- **AND** every harness keeps what was delivered to it

#### Scenario: Deleting a vendor store
- **WHEN** the operator deletes one harness's vendor store
- **THEN** that harness's UZE-delivered plugins stop loading until UZE delivers them again

#### Scenario: Deleting a record
- **WHEN** the operator deletes what UZE was told or decided, its intent
  and ownership
- **THEN** that is lost to the operator, because nothing else on the
  machine knows it

#### Scenario: Deleting the generated tier
- **WHEN** the operator deletes everything UZE generated for itself
- **THEN** the next run produces it again, and no record, package or
  checkout is lost
- **AND** no harness behaves differently in the meantime, except an agent
  the workspace launched with a context directory there

#### Scenario: Deleting the remembered tier
- **WHEN** the operator deletes what UZE remembered
- **THEN** the next run observes it again, and nothing the operator chose
  is lost

#### Scenario: A harness's recorded setup
- **WHEN** what UZE recorded about a harness — its version, how it was
  installed, how capabilities reach it — is deleted
- **THEN** it is re-derived by probing that harness, because nothing in it
  was the operator's to decide

## ADDED Requirements

### Requirement: The package bytes move to their own directory, fenced against older builds
A build that knows the store tier SHALL move each package's bytes from
`store/` to `sources/` under the mutation lock, without copying them, and
SHALL first write the package registry and the receipt ledger at a new
shape, so an older build refuses both by name instead of pruning, detaching
or writing packages into the new layout.

#### Scenario: An upgrade
- **WHEN** a machine whose package bytes are in `store/` runs the new build
- **THEN** the bytes are in `sources/`, unchanged, and `store/` holds only vendor stores

#### Scenario: Interrupted
- **WHEN** the move stops after some packages
- **THEN** the next run moves the rest, and no package is acquired again

#### Scenario: An older build afterwards
- **WHEN** an older build runs on a machine the new build has moved
- **THEN** it refuses the package registry and the receipt ledger as newer and names them, and no package or receipt is lost
- **AND** the next run of the new build removes any empty directory the older build left under `store/`

#### Scenario: An older UZE is still running
- **WHEN** a background upgrade installed the new build while a workspace of the older one is still open
- **THEN** nothing is fenced or moved until that workspace has restarted into the new release

#### Scenario: Packages installed before the move
- **WHEN** the move completes on a machine whose registry has no digests
- **THEN** every moved package has a digest recorded
