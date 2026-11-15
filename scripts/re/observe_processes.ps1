$ErrorActionPreference = "Stop"
$tempProfile = Join-Path $env:TEMP ("chrome_re_" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tempProfile -Force | Out-Null

$chromePath = "C:\Program Files\Google\Chrome\Application\chrome.exe"
$proc = Start-Process -FilePath $chromePath -ArgumentList @(
    "--user-data-dir=$tempProfile",
    "--no-first-run",
    "--no-default-browser-check",
    "--headless=new",
    "--remote-debugging-port=0",
    "about:blank"
) -PassThru

Start-Sleep -Seconds 3

$procs = Get-CimInstance Win32_Process | Where-Object { $_.Name -eq "chrome.exe" -and ($_.CommandLine -like "*$tempProfile*" -or $_.ProcessId -eq $proc.Id -or $_.ParentProcessId -eq $proc.Id) }
$output = $procs | Select-Object ProcessId, ParentProcessId, CommandLine | Sort-Object ProcessId
$output | ConvertTo-Json -Depth 3

# Cleanly terminate only the temporary Chrome process tree
$procs | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Start-Sleep -Milliseconds 500
Remove-Item -Path $tempProfile -Recurse -Force -ErrorAction SilentlyContinue
