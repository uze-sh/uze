# The handler the stub group in hooks.json runs on Windows, in Windows
# PowerShell. The contract (ADR-033) is the one scripts/guard keeps:
#
#   HOOK_* environment in, exit code out.
#     0 - allow
#     3 - deny; write the reason on stderr first:
#         [Console]::Error.WriteLine('why'); exit 3
#     anything else, or the timeout above - fails the way the group's
#         `effect` says
#
# The context is in $env:HOOK_TOOL, $env:HOOK_COMMAND, $env:HOOK_PATH and
# the rest of the HOOK_* variables. Match the event with the same `matcher`
# words hooks.json uses: a vendor-native name, or a tool alias like `shell`
# that every harness has an equivalent for.
#
# Replace this with the real check, and keep it fast: the timeout is
# seconds, and a handler that sits on it fails like an error does.
exit 0
