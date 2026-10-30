## ADDED Requirements

### Requirement: Delivery stops in a repository that uses Git LFS

When the checkout being delivered uses Git LFS, delivery SHALL stop before writing anything to the remote and SHALL name the next step: an approved gate step that uploads the objects, or a push by hand. A delivery SHALL NOT leave a remote holding pointers to objects it does not have.

#### Scenario: An LFS checkout is not pushed silently
- **WHEN** an agent's checkout has a path with the `filter=lfs` attribute and its delivery runs
- **THEN** nothing is pushed, and the task says the repository uses Git LFS and how to deliver it
