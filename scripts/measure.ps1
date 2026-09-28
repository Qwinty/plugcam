# Measures how much memory and CPU a running app takes, counting its whole process tree (for
# Plugcam that is Plugcam.exe and its WebView2 processes). Samples every second and prints the
# averages, so the README numbers can be checked on any PC. taskManagerMB is the private working
# set, the "Memory" column of Task Manager; privateMB is all private memory, including what is
# paged out or never touched. The adb server is left out by default:
# it is shared with every other Android tool on the PC; add it with -Also adb.
#
#   .\scripts\measure.ps1                          # Plugcam, 60 s
#   .\scripts\measure.ps1 -Name Camo -Seconds 120  # any other app, by process name
param(
    [string]$Name = 'Plugcam',
    [int]$Seconds = 60,
    # Extra top-level processes to count, e.g. adb.
    [string[]]$Also = @()
)
$ErrorActionPreference = 'Stop'

function Tree {
    $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name
    $roots = $all | Where-Object { $_.Name -replace '\.exe$' -in @($Name) + $Also }
    if (-not $roots) { throw "no process named $Name is running" }
    $ids = [Collections.Generic.HashSet[int]]::new()
    $queue = [Collections.Generic.Queue[int]]::new()
    foreach ($r in $roots) { if ($ids.Add([int]$r.ProcessId)) { $queue.Enqueue([int]$r.ProcessId) } }
    while ($queue.Count) {
        $p = $queue.Dequeue()
        foreach ($c in $all | Where-Object { $_.ParentProcessId -eq $p }) {
            if ($ids.Add([int]$c.ProcessId)) { $queue.Enqueue([int]$c.ProcessId) }
        }
    }
    $ids
}

$cores = [Environment]::ProcessorCount
$samples = @()
$prevCpu = $null
$prevTime = $null
for ($i = 0; $i -le $Seconds; $i++) {
    $ids = @(Tree)
    $procs = Get-Process -Id $ids -ErrorAction SilentlyContinue
    $perf = Get-CimInstance Win32_PerfRawData_PerfProc_Process | Where-Object { [int]$_.IDProcess -in $ids }
    # CPU seconds so far; a process that exited in between makes a sample slightly low.
    $cpu = [double](($procs | ForEach-Object { $_.TotalProcessorTime.TotalSeconds } | Measure-Object -Sum).Sum)
    $now = [DateTime]::UtcNow
    $sample = [ordered]@{
        processes = $procs.Count
        taskManagerMB = [math]::Round((($perf | Measure-Object WorkingSetPrivate -Sum).Sum) / 1MB, 1)
        workingSetMB = [math]::Round((($procs | Measure-Object WorkingSet64 -Sum).Sum) / 1MB, 1)
        privateMB = [math]::Round((($procs | Measure-Object PrivateMemorySize64 -Sum).Sum) / 1MB, 1)
        cpuPercent = $null
    }
    if ($prevTime) {
        $busy = [math]::Max(0.0, $cpu - $prevCpu)
        $sample.cpuPercent = [math]::Round(100 * $busy / ($now - $prevTime).TotalSeconds / $cores, 2)
    }
    $prevCpu = $cpu
    $prevTime = $now
    $samples += [pscustomobject]$sample
    if ($i -lt $Seconds) { Start-Sleep -Seconds 1 }
}

$measured = $samples | Where-Object { $null -ne $_.cpuPercent }
[pscustomobject]@{
    app = $Name
    seconds = $Seconds
    processes = ($samples | Measure-Object processes -Maximum).Maximum
    taskManagerMB = [math]::Round(($samples | Measure-Object taskManagerMB -Average).Average, 1)
    workingSetMB = [math]::Round(($samples | Measure-Object workingSetMB -Average).Average, 1)
    privateMB = [math]::Round(($samples | Measure-Object privateMB -Average).Average, 1)
    cpuPercent = [math]::Round(($measured | Measure-Object cpuPercent -Average).Average, 2)
    cpuNote = "share of all $cores logical cores"
}
