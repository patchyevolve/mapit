# mapit installer for Windows
# Run: powershell -c "irm https://raw.githubusercontent.com/patchyevolve/mapit/main/install.ps1 | iex"

$App = "mapit"
$Repo = "patchyevolve/mapit"

function Die($Msg) {
    Write-Error $Msg
    exit 1
}

function Get-Arch {
    $arch = (Get-CimInstance Win32_Processor).Architecture
    switch ($arch) {
        0  { Die "x86 (32-bit) is not supported" }
        9  { "x86_64" }
        12 { "aarch64" }
        default { Die "Unsupported architecture: $arch" }
    }
}

function Get-LatestVersion {
    $api = "https://api.github.com/repos/$Repo/releases/latest"
    $json = Invoke-RestMethod -Uri $api
    return $json.tag_name -replace '^v', ''
}

function Main {
    $version = ""
    if ($args.Count -gt 0) { $version = $args[0] }

    if (-not $version) {
        Write-Host "Fetching latest version..." -ForegroundColor Cyan
        $version = Get-LatestVersion
    }

    $arch = Get-Arch
    $url = "https://github.com/$Repo/releases/download/v${version}/${App}-${arch}-pc-windows-msvc.zip"
    $installDir = "$env:USERPROFILE\.$App\bin"

    Write-Host "Downloading $App v$version..." -ForegroundColor Cyan
    $zip = "$env:TEMP\$App.zip"
    Invoke-WebRequest -Uri $url -OutFile $zip

    # Extract to a temp folder first, then move just the exe
    $tmp = "$env:TEMP\$App-extract"
    if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    Expand-Archive -Path $zip -DestinationPath $tmp -Force

    # Find mapit.exe anywhere in the extracted tree
    $exe = Get-ChildItem -Recurse -Filter "$App.exe" -Path $tmp | Select-Object -First 1
    if (-not $exe) { Die "Could not find $App.exe in the downloaded archive" }

    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
    Copy-Item -Path $exe.FullName -Destination "$installDir\$App.exe" -Force

    # Clean up temp
    Remove-Item -Recurse -Force $zip, $tmp -ErrorAction SilentlyContinue

    # Add to user PATH if not already there
    $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($currentPath -notlike "*$installDir*") {
        $newPath = "$installDir;$currentPath"
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        # Update current session too
        $env:Path = "$installDir;$env:Path"
        Write-Host "Added $installDir to your PATH (user-level)" -ForegroundColor Green
    }

    Write-Host "Installed $App v$version to $installDir\$App.exe" -ForegroundColor Green
    & "$installDir\$App.exe" --version
}

Main @args
