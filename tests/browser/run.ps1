# Browser checks for the overlay, gallery and settings page.
#
#   powershell -ExecutionPolicy Bypass -File tests\browser\run.ps1
#
# 1. Checks: things that must always be true (every theme starts with no script errors, fixed bugs stay fixed).
#    Any failure makes the script exit with 1.
# 2. Comparison: runs the last release and your current files side by side with the same song data,
#    and lists every difference. Read them: each one should be a change you meant to make.
#
# It uses Microsoft Edge in the background and a small fake server, so Now Playing doesn't need to be
# running and nothing in config.toml, favorites.json or the registry is touched.

param(
    [string]$Baseline,          # the release to compare with (default: the latest tag that isn't this commit)
    [switch]$FailOnDifferences  # also exit with 1 if anything differs from the baseline
)
$ErrorActionPreference = "Stop"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$probes = $PSScriptRoot

# ---------- the two versions to run ----------
if (-not $Baseline) {
    # When this commit is itself a release, compare with the one before it
    $from = if (git -C $repo tag --points-at HEAD) { "HEAD^" } else { "HEAD" }
    $Baseline = git -C $repo describe --tags --abbrev=0 $from
    if (-not $Baseline) { throw "No earlier release tag found. Pass one with -Baseline v0.3.1" }
}
$work = Join-Path ([IO.Path]::GetTempPath()) ("nowplaying-browser-tests-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory $work | Out-Null
$tar = Join-Path $work "baseline.tar"
git -C $repo archive -o $tar $Baseline static
New-Item -ItemType Directory "$work\baseline" | Out-Null
tar -xf $tar -C "$work\baseline"
# Your current files: everything git tracks or would add (with your uncommitted edits), but not ignored files
# like your private custom themes, which never ship
foreach ($file in git -C $repo ls-files --cached --others --exclude-standard static) {
    $from = Join-Path $repo $file
    if (-not (Test-Path -LiteralPath $from -PathType Leaf)) { continue }   # deleted but not committed yet
    $to = Join-Path "$work\current" $file
    New-Item -ItemType Directory (Split-Path $to) -Force | Out-Null
    Copy-Item -LiteralPath $from $to
}
$roots = @{ baseline = "$work\baseline\static"; current = "$work\current\static" }

$edge = @("${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe", "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $edge) { throw "Microsoft Edge wasn't found" }

# ---------- the fake server ----------
$settingsState = '{"settings":{"port":5000,"process_name":"AmpLibraryAgent.exe, spotify.exe","app_id_match":"applemusic, spotify","eq_floor_db":-82.0,"eq_ceil_db":-27.0,"eq_tilt_db":3.0,"tray_themes":["default"],"bind_address":"127.0.0.1","allowed_hosts":["desktop-abc.lan"]},"defaults":{"port":5000,"eq_floor_db":-73.0,"eq_ceil_db":-27.0,"eq_tilt_db":3.0},"restart_needed":false,"unknown":[],"error":null,"network_url":null}'
$png = [Convert]::FromBase64String("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M/AAwAFAQH/ijAFJQAAAABJRU5ErkJggg==")
$types = @{ ".css" = "text/css"; ".js" = "text/javascript"; ".ttf" = "font/ttf"; ".html" = "text/html"; ".ico" = "image/x-icon"; ".png" = "image/png"; ".jpg" = "image/jpeg" }
$probe = New-Object Net.Sockets.TcpListener([Net.IPAddress]::Loopback, 0); $probe.Start(); $port = $probe.LocalEndpoint.Port; $probe.Stop()
$listener = New-Object Net.HttpListener
$listener.Prefixes.Add("http://localhost:$port/")
$listener.Start()
$script:pending = $listener.GetContextAsync()

function Send-Bytes($c, $code, $type, [byte[]]$bytes) {
    $c.Response.StatusCode = $code
    if ($type) { $c.Response.ContentType = $type }
    $c.Response.ContentLength64 = $bytes.Length
    try { $c.Response.OutputStream.Write($bytes, 0, $bytes.Length); $c.Response.Close() } catch {}
}
function Send-Text($c, $code, $type, $text) { Send-Bytes $c $code $type ([Text.Encoding]::UTF8.GetBytes($text)) }

# The same list the app's /themes.json gives: custom first, examples last, names alphabetical ignoring capitals
function Get-ThemeList($root) {
    $rank = @{ "custom" = 0; "examples" = 2 }
    $folders = Get-ChildItem "$root\themes" -Directory | Sort-Object @{ e = { if ($rank.ContainsKey($_.Name)) { $rank[$_.Name] } else { 1 } } }, Name
    $list = foreach ($d in $folders) {
        [pscustomobject]@{ name = $d.Name; themes = @(Get-ChildItem $d.FullName -Filter *.css | ForEach-Object BaseName | Sort-Object { $_.ToLower() }) }
    }
    ConvertTo-Json @($list) -Depth 4 -Compress
}

# Opens one page in Edge and serves it until the page posts its result to /__result
function Invoke-Page($version, $path, [int]$failThemeList = 0) {
    $root = $roots[$version]
    $themeList = Get-ThemeList $root
    $result = $null
    $listCalls = 0
    $profile = Join-Path $work ("edge-" + [guid]::NewGuid().ToString("N"))
    # Edge's own log messages go to files, so they don't clutter the results
    $edgeProcess = Start-Process $edge -PassThru -WindowStyle Hidden -RedirectStandardOutput "$profile.out.log" -RedirectStandardError "$profile.err.log" `
        -ArgumentList "--headless=new", "--disable-gpu", "--no-first-run", "--log-level=3", "--window-size=1400,1000", "--user-data-dir=`"$profile`"", "http://localhost:$port$path"
    $deadline = (Get-Date).AddSeconds(180)
    while (-not $result -and (Get-Date) -lt $deadline) {
        if (-not $script:pending.Wait(250)) { continue }
        $c = $script:pending.Result
        $script:pending = $listener.GetContextAsync()
        $p = [Uri]::UnescapeDataString($c.Request.Url.AbsolutePath)
        $page = { param($file, $inject, $where) ([IO.File]::ReadAllText((Join-Path $root $file))).Replace($where, $inject) }
        switch -Regex ($p) {
            '^/__probe/(.+)$' { Send-Bytes $c 200 $types[[IO.Path]::GetExtension($Matches[1])] ([IO.File]::ReadAllBytes((Join-Path $probes $Matches[1]))); break }
            '^/overlay\.html$' { Send-Text $c 200 "text/html" (& $page "overlay.html" '<head><script src="/__probe/overlay-stub.js"></script>' '<head>'); break }
            '^/gallery\.html$' { Send-Text $c 200 "text/html" (& $page "gallery.html" '<script src="/__probe/gallery-probe.js"></script></body>' '</body>'); break }
            '^/settings\.html$' { Send-Text $c 200 "text/html" (& $page "settings.html" '<script src="/__probe/settings-probe.js"></script><script src="settings.js">' '<script src="settings.js">'); break }
            '^/themes\.json$' { $listCalls++; if ($listCalls -le $failThemeList) { Send-Text $c 500 "text/plain" "test failure" } else { Send-Text $c 200 "application/json" $themeList }; break }
            '^/nowplaying$' { Send-Text $c 200 "application/json" '{"title":null}'; break }
            '^/spectrum$' { Send-Text $c 200 "application/json" '{"levels":[0,0,0,0,0,0,0,0]}'; break }
            '^/favorites' { Send-Text $c 200 "application/json" $(if ($p -eq "/favorites" -and $c.Request.HttpMethod -eq "GET") { '{"saved":true,"list":["neon","Halloween","blurred"]}' } else { '["neon","Halloween","blurred"]' }); break }
            '^/settings$' { Send-Text $c 200 "application/json" $settingsState; break }
            '^/players/detected$' { Send-Text $c 200 "application/json" '[{"id":"AppleInc.AppleMusicWin_x!App","playing":true,"known":true}]'; break }
            '^/startup$' { Send-Text $c 200 "application/json" '{"enabled":true,"other_copy":false,"blocked":false,"can_enable":true}'; break }
            '^/cover-' { Send-Bytes $c 200 "image/png" $png; break }
            '^/__result$' { $result = (New-Object IO.StreamReader($c.Request.InputStream)).ReadToEnd(); Send-Text $c 204 $null ""; break }
            default {
                $file = [IO.Path]::GetFullPath((Join-Path $root ($p.TrimStart("/").Replace("/", "\"))))
                if ($file.StartsWith($root) -and (Test-Path -LiteralPath $file -PathType Leaf)) { Send-Bytes $c 200 $types[[IO.Path]::GetExtension($file)] ([IO.File]::ReadAllBytes($file)) }
                else { Send-Text $c 404 "text/plain" "" }
            }
        }
    }
    Stop-Process -Id $edgeProcess.Id -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 400
    Remove-Item $profile -Recurse -Force -ErrorAction SilentlyContinue
    if (-not $result) { throw "$version $path didn't finish within 3 minutes" }
    $result | ConvertFrom-Json
}

Write-Host "Comparing $Baseline with your current files..."
$hasSettings = Test-Path (Join-Path $roots.baseline "settings.html")
$runs = @{}
foreach ($v in "baseline", "current") {
    Write-Host "  $v`: overlays" -NoNewline
    $r = @{ overlay = Invoke-Page $v "/__probe/overlay-probe.html" }
    Write-Host ", gallery" -NoNewline
    $r.gallery = Invoke-Page $v "/gallery.html"
    $r.galleryHash = Invoke-Page $v "/gallery.html#HALLOWEEN"
    if ($v -eq "current" -or $hasSettings) { Write-Host ", settings" -NoNewline; $r.settings = Invoke-Page $v "/settings.html" }
    if ($v -eq "current") { Write-Host ", theme retry" -NoNewline; $r.retry = Invoke-Page $v "/__probe/retry-probe.html" 2 }
    Write-Host ""
    $runs[$v] = $r
}
$listener.Stop()
Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue

# ---------- 1. checks: must always be true ----------
$now = $runs.current
$failures = @()
function Test-Check($name, $ok, $detail) {
    if ($ok) { Write-Host "  ok    $name" } else { Write-Host "  FAIL  $name : $detail" -ForegroundColor Red; $script:failures += $name }
}
$themeRows = @($now.overlay.themes.PSObject.Properties)
$every = { param($scenario, $test) @($themeRows | Where-Object { -not (& $test $_.Value.$scenario) } | ForEach-Object Name) }
Write-Host ""
Write-Host "Checks"
$bad = & $every "s1 normal" { param($s) $s.ready }
Test-Check "every theme starts ($($themeRows.Count) themes)" ($bad.Count -eq 0) ($bad -join ", ")
$bad = @($themeRows | Where-Object { $row = $_.Value; @($row.PSObject.Properties | Where-Object { $_.Value.errors }).Count } | ForEach-Object Name)
Test-Check "no script errors in any theme or situation" ($bad.Count -eq 0) ($bad -join ", ")
$bad = & $every "s0 before any data" { param($s) -not (@($s.title, $s.artist, $s.album) -match '^(Title|Artist|Album)$') }
Test-Check "no placeholder text before the first song (0.3.0 showed Title/Artist/Album)" ($bad.Count -eq 0) ($bad -join ", ")
$bad = & $every "s4 no cover" { param($s) -not $s.cover }
Test-Check "a song without a cover clears the old one (0.3.0 kept it)" ($bad.Count -eq 0) ($bad -join ", ")
$bad = & $every "s1 normal" { param($s) $s.title -eq "Song Title" -or $s.title -eq "(hidden)" }
Test-Check "song text appears" ($bad.Count -eq 0) ($bad -join ", ")
$names = $now.overlay.themeNames
Test-Check "theme names work in any capitalization (0.3.0 needed the exact case)" ($names.'upper NEON'.theme -like "*neon.css" -and $names.'lower halloween'.theme -like "*Halloween.css") "NEON -> $($names.'upper NEON'.theme), halloween -> $($names.'lower halloween'.theme)"
Test-Check "an unknown theme still starts with the base style" ($names.unknown.started -and $names.unknown.theme -eq "(none)") ($names.unknown | ConvertTo-Json -Compress)
Test-Check "the theme loads after the theme list fails twice (0.3.0 stayed unstyled)" ($now.retry.started -and $now.retry.theme -like "*neon.css") ($now.retry | ConvertTo-Json -Compress)
Test-Check "gallery.html#HALLOWEEN opens Halloween" ($now.galleryHash.opened -eq "Halloween") $now.galleryHash.opened
Test-Check "gallery tag scan finishes" ([bool]$now.gallery.tagChips) "no tag chips"
Test-Check "favorites sort ignoring capitals (0.3.0 put Halloween first)" ($now.gallery.favorites -eq "blurred,Halloween,neon") $now.gallery.favorites
Test-Check "searching the name shown on a card finds it (0.3.1 needed the file name)" ($now.gallery.searchShownName -eq 1 -and $now.gallery.searchFileName -eq 1) "radio tuner -> $($now.gallery.searchShownName), radio-tuner -> $($now.gallery.searchFileName)"
Test-Check "a search with no results says so (0.3.1 showed a blank page)" ($now.gallery.noResultsMessage -ne "(blank page)") $now.gallery.noResultsMessage
Test-Check "Copy URL goes back to its label after two quick clicks (0.3.1 stayed on Copied!)" ($now.gallery.copyLabelAfterTwoClicks -eq "Copy URL") $now.gallery.copyLabelAfterTwoClicks
Test-Check "without the clipboard API, the enlarged view copies from a box inside it (0.3.1 couldn't)" ($now.gallery.copyFocusWithoutClipboard -eq "TEXTAREA in the view") $now.gallery.copyFocusWithoutClipboard
Test-Check "a copy that fails doesn't say Copied! (0.3.1 did)" ($now.gallery.copyLabelWhenItFails -and $now.gallery.copyLabelWhenItFails -ne "Copied!") $now.gallery.copyLabelWhenItFails
Test-Check "no script errors in the gallery" (-not $now.gallery.errors) $now.gallery.errors
Test-Check "two EQ sliders moved quickly are both saved (0.3.0 lost one)" ($now.settings.twoSlidersSaved -match 'eq_ceil_db":-20' -and $now.settings.twoSlidersSaved -match 'eq_floor_db":-90') $now.settings.twoSlidersSaved
Test-Check "'Saved' clears after a couple of seconds" (-not $now.settings.statusLater) $now.settings.statusLater
Test-Check "no script errors on the settings page" (-not $now.settings.errors) $now.settings.errors

# ---------- 2. comparison with the baseline ----------
function Compare-Results($label, $a, $b) {
    $out = @()
    $keys = @(@($a.PSObject.Properties.Name) + @($b.PSObject.Properties.Name) | Select-Object -Unique)
    foreach ($k in $keys) {
        $x = $a.$k; $y = $b.$k
        if (($x -is [pscustomobject]) -or ($y -is [pscustomobject])) { $out += Compare-Results "$label > $k" $x $y; continue }
        if ("$x" -ne "$y") { $out += [pscustomobject]@{ where = "$label > $k"; before = "$x"; after = "$y" } }
    }
    $out
}
$diffs = @()
# A theme that's only in one version is listed once, not field by field
$before = $runs.baseline.overlay.themes; $after = $now.overlay.themes
foreach ($name in @(@($before.PSObject.Properties.Name) + @($after.PSObject.Properties.Name) | Select-Object -Unique)) {
    if (-not $before.$name) { $diffs += [pscustomobject]@{ where = "overlay > new theme > $name"; before = "(not in $Baseline)"; after = "added" }; continue }
    if (-not $after.$name) { $diffs += [pscustomobject]@{ where = "overlay > removed theme > $name"; before = "in $Baseline"; after = "(gone)" }; continue }
    $diffs += Compare-Results "overlay > themes > $name" $before.$name $after.$name
}
$diffs += Compare-Results "overlay > themeNames" $runs.baseline.overlay.themeNames $now.overlay.themeNames
foreach ($part in "gallery", "galleryHash", "settings") {
    if ($runs.baseline.$part) { $diffs += Compare-Results $part $runs.baseline.$part $now.$part }
}
Write-Host ""
Write-Host "Differences from $Baseline"
if (-not $diffs) { Write-Host "  none" }
# The same change in many themes is listed once, with a few examples
$diffs | Group-Object { ($_.where -replace '^overlay > themes > [^>]+ > ', 'overlay > (every theme) > ') + "|" + $_.before + "|" + $_.after } | ForEach-Object {
    $d = $_.Group[0]
    $where = $d.where -replace '^overlay > themes > [^>]+ > ', 'overlay > (theme) > '
    Write-Host ("  {0}  ({1}x)" -f $where, $_.Count) -ForegroundColor Yellow
    Write-Host "      before: $($d.before)"
    Write-Host "      now:    $($d.after)"
    if ($_.Count -gt 1) { Write-Host ("      e.g. " + (($_.Group | ForEach-Object { ($_.where -split ' > ')[2] } | Select-Object -Unique -First 5) -join ", ")) }
}

Write-Host ""
if ($failures) { Write-Host "$($failures.Count) check(s) failed." -ForegroundColor Red; exit 1 }
if ($diffs -and $FailOnDifferences) { Write-Host "All checks passed, but there are differences from $Baseline." -ForegroundColor Yellow; exit 1 }
Write-Host "All checks passed." -ForegroundColor Green
exit 0
