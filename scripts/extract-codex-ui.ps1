# Extract Codex Desktop UI source for design research
# Downloads the macOS app, unpacks app.asar so you can study layout patterns.
param(
    [string]$Version = "26.602.71036",
    [string]$OutDir = "artifacts/codex-research"
)

$ErrorActionPreference = "Stop"
$zipUrl = "https://persistent.oaistatic.com/codex-app-prod/Codex-darwin-arm64-$Version.zip"
$zipFile = "$OutDir/Codex-$Version.zip"
$extractDir = "$OutDir/extracted"
$asarOutput = "$OutDir/app-source"

Write-Host "=== ReadOS Codex UI Research Extractor ===" -ForegroundColor Cyan

# 1. Create output directories
New-Item -ItemType Directory -Force -Path $OutDir, $extractDir, $asarOutput | Out-Null

# 2. Download
if (-not (Test-Path $zipFile)) {
    Write-Host "[1/4] Downloading Codex $Version ..." -ForegroundColor Yellow
    Invoke-WebRequest -Uri $zipUrl -OutFile $zipFile
    Write-Host "       Downloaded to $zipFile" -ForegroundColor Green
} else {
    Write-Host "[1/4] Already downloaded: $zipFile" -ForegroundColor Gray
}

# 3. Extract ZIP (contains .app bundle)
Write-Host "[2/4] Extracting archive ..." -ForegroundColor Yellow
& 7z x $zipFile -o"$extractDir" -y | Select-Object -Last 3
Write-Host "       Extracted to $extractDir" -ForegroundColor Green

# 4. Find app.asar inside .app bundle
$appBundle = Get-ChildItem -Path $extractDir -Recurse -Directory -Filter "*.app" `
    | Where-Object { $_.FullName -notlike "*__MACOSX*" } `
    | Select-Object -First 1

if (-not $appBundle) {
    Write-Host "ERROR: Could not find .app bundle" -ForegroundColor Red
    exit 1
}

$asarPath = Join-Path $appBundle.FullName "Contents/Resources/app.asar"
if (-not (Test-Path $asarPath)) {
    Write-Host "ERROR: app.asar not found at $asarPath" -ForegroundColor Red
    exit 1
}

Write-Host "       Found app.asar: $asarPath" -ForegroundColor Green

# 5. Extract ASAR
Write-Host "[3/4] Extracting app.asar ..." -ForegroundColor Yellow
npx @electron/asar extract $asarPath $asarOutput 2>&1 | Select-Object -Last 3
Write-Host "       Extracted to $asarOutput" -ForegroundColor Green

# 6. Report structure
Write-Host "[4/4] Analyzing extracted source ..." -ForegroundColor Yellow

$topDirs = Get-ChildItem -Path $asarOutput -Directory -Exclude "node_modules" `
    | Select-Object -First 20

Write-Host ""
Write-Host "=== Top-level directories ===" -ForegroundColor Cyan
foreach ($d in $topDirs) {
    $count = (Get-ChildItem -Path $d.FullName -Recurse -File -ErrorAction SilentlyContinue | Measure-Object).Count
    Write-Host "  $($d.Name)/  ($count files)" -ForegroundColor White
}

# Look for key UI-related files
Write-Host ""
Write-Host "=== Key UI files to study ===" -ForegroundColor Cyan

$patterns = @(
    @{Label="React components"; Pattern="*.tsx"; Desc="UI component source"},
    @{Label="CSS/Styles"; Pattern="*.css"; Desc="Styling and layout"},
    @{Label="HTML templates"; Pattern="*.html"; Desc="Webview HTML structure"},
    @{Label="Layout config"; Pattern="*layout*"; Desc="Layout-related files"},
    @{Label="Theme tokens"; Pattern="*theme*"; Desc="Design tokens and theming"},
    @{Label="Package info"; Pattern="package.json"; Desc="Dependency manifest"}
)

foreach ($p in $patterns) {
    $found = Get-ChildItem -Path $asarOutput -Recurse -Name $p.Pattern `
        -Exclude "node_modules" -ErrorAction SilentlyContinue `
        | Where-Object { $_ -notlike "node_modules*" } `
        | Select-Object -First 5
    if ($found) {
        Write-Host "  [$($p.Label)] $($p.Desc):" -ForegroundColor Yellow
        foreach ($f in $found) {
            Write-Host "    - $f" -ForegroundColor Gray
        }
    }
}

# Check for webview content
$webviewDir = Join-Path $asarOutput "webview"
if (Test-Path $webviewDir) {
    $webviewFiles = Get-ChildItem -Path $webviewDir -Recurse -File `
        | Where-Object { $_.Extension -match '\.(html|css|js|tsx|jsx)$' } `
        | Select-Object -First 10
    Write-Host "  [Webview content]:" -ForegroundColor Yellow
    foreach ($f in $webviewFiles) {
        Write-Host "    - $($f.FullName.Replace($asarOutput, ''))" -ForegroundColor Gray
    }
}

Write-Host ""
Write-Host "=== Done ===" -ForegroundColor Green
Write-Host "Source extracted to: $asarOutput" -ForegroundColor White
Write-Host ""
Write-Host "Next steps:" -ForegroundColor Cyan
Write-Host "  1. Browse $asarOutput/.vite/build/ for compiled JS bundles" -ForegroundColor White
Write-Host "  2. Open $asarOutput/package.json to see dependencies" -ForegroundColor White
Write-Host "  3. Search for 'grid' or 'flex' in CSS files for layout patterns" -ForegroundColor White
Write-Host "  4. Cross-reference with docs/UI_UX_DESIGN.md for implementation ideas" -ForegroundColor White
