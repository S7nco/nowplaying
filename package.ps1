# package.ps1: builds a release and makes a ready-to-share zip.
# Run it from the project folder (the one with Cargo.toml and static\):
#     powershell -ExecutionPolicy Bypass -File .\package.ps1
# The result is dist\NowPlaying.zip. Unzip it somewhere else and run nowplaying.exe to test it.

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

$exeName = "nowplaying"          # the name of your .exe (without .exe)
$folder  = "dist\NowPlaying"     # the folder that goes inside the zip

# 1. Build
Write-Host "Building release..."
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "The build failed" }

# 2. Start with a clean dist folder
if (Test-Path "dist") { Remove-Item "dist" -Recurse -Force }
New-Item -ItemType Directory -Path $folder | Out-Null

# 3. Copy what the app needs, next to the .exe
Copy-Item "target\release\$exeName.exe" $folder
Copy-Item "static" "$folder\static" -Recurse
Copy-Item "icon.ico" $folder

# The cover of whatever you were last playing is saved here while the app runs. Don't ship it.
Remove-Item "$folder\static\cover.jpg" -ErrorAction SilentlyContinue
Remove-Item "$folder\static\cover.jpg.tmp" -ErrorAction SilentlyContinue

# Ship an empty custom folder (just the README), not my own themes
Get-ChildItem "$folder\static\themes\custom" -Exclude "README.txt" | Remove-Item -Recurse -Force

# Ship the template only, so unzipping an update never overwrites someone's config.toml
Copy-Item "config.example.toml" $folder

foreach ($file in "README.md", "THEMES.md", "LICENSE") {
    if (Test-Path $file) { Copy-Item $file $folder }
}

# 4. Zip the folder
Compress-Archive -Path $folder -DestinationPath "dist\NowPlaying.zip" -Force

Write-Host ""
Write-Host "Done: dist\NowPlaying.zip"
Write-Host "Test it: unzip it into a new folder and run $exeName.exe from there."