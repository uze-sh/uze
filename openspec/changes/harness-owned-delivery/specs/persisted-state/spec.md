## MODIFIED Requirements

### Requirement: Every durable thing belongs to one tier, and the tier says what losing it costs
UZE SHALL keep what it persists in tiers distinguished by what deleting
them costs, and each thing SHALL belong to exactly one: **bytes** (the
installed packages), **record** (what UZE was told or decided), **generated**
(what UZE produced and can produce again) and **remembered** (what
UZE observed and kept to go faster). A thing SHALL NOT sit in a tier that
claims a different cost than it has. What a harness reads SHALL NOT be in
any tier under `$UZE_HOME`: it is a delivery in the harness's territory.

#### Scenario: Deleting the bytes
- **WHEN** the operator deletes the installed packages
- **THEN** the packages are gone until they are acquired again

#### Scenario: Deleting a record
- **WHEN** the operator deletes what UZE was told or decided, its intent
  and ownership
- **THEN** that is lost to the operator, because nothing else on the
  machine knows it

#### Scenario: Deleting the generated tier
- **WHEN** the operator deletes everything UZE generated
- **THEN** the next run produces it again, and no record, package or
  checkout is lost
- **AND** no harness behaves differently in the meantime

#### Scenario: Deleting the remembered tier
- **WHEN** the operator deletes what UZE remembered
- **THEN** the next run observes it again, and nothing the operator chose
  is lost

#### Scenario: A harness's recorded setup
- **WHEN** what UZE recorded about a harness — its version, how it was
  installed, how capabilities reach it — is deleted
- **THEN** it is re-derived by probing that harness, because nothing in it
  was the operator's to decide
