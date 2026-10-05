# What the installer is held to. Everything at Warning and above, except
# the one rule that is about a different kind of script: the installer
# talks to the person at the console, under `iex` as often as not, and its
# output is that conversation, not a pipeline's data.
@{
    Severity     = @('Warning', 'Error')
    ExcludeRules = @('PSAvoidUsingWriteHost')
}
