## ADDED Requirements

### Requirement: The workspace's Git operations run nothing the repository's configuration names
Every Git process UZE starts SHALL run no hook and no file-system monitor,
refuse the `ext` transport and not recurse into submodules. Where Git could
run a program named by configuration (filter, merge driver, credential
helper, SSH, askpass or proxy command, upload or receive pack, signing
program), only the operator's system, global or environment configuration
SHALL take effect; the repository's own SHALL be overridden.

#### Scenario: A hook planted in the common Git directory does not run on placement
- **WHEN** an agent writes an executable `post-checkout` into the
  repository's common hooks directory and a checkout is then placed
- **THEN** the checkout is placed and the hook does not run

#### Scenario: A transport named by the repository does not run on the sync
- **WHEN** the repository's configuration names an `ext::` remote, an SSH
  command, a credential helper or an upload pack
- **THEN** the workspace's fetch, push and `ls-remote` run none of them

#### Scenario: A filter or merge driver the repository names does not run
- **WHEN** the repository's configuration defines a clean, smudge or
  process filter, or a merge driver, that its attributes select
- **THEN** UZE's placement, status, rebase and merge run none of them

#### Scenario: The operator's own configuration still applies
- **WHEN** the operator's global configuration names a credential helper
  or SSH command
- **THEN** UZE's Git operations use it

### Requirement: Nothing read back from Git reaches Git as an option
A value UZE reads from Git's output or configuration and passes to Git
again SHALL follow an end-of-options marker, so a value beginning with `-`
is an operand.

#### Scenario: A push URL shaped like an option is an operand
- **WHEN** the remote's push URL is `--upload-pack=<command>` and the sync
  asks which published branches the remote still has
- **THEN** Git is handed the URL after `--`, and the command does not run
