# Restart a service and report how long it took.
param([string]$Name = "Spooler")

Add-Type -TypeDefinition @'
public static class Clock {
    public static long Now() { return System.DateTime.Now.Ticks; }
}
'@

$started = Get-Date
Restart-Service -Name $Name -Force
$elapsed = (Get-Date) - $started
if ($Name -match '^Spool') {
    Write-Host "Restarted $Name in $($elapsed.TotalSeconds)s"
}
