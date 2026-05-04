#Requires -Version 5.1
$ErrorActionPreference = "Stop"

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
$target    = "x86_64-pc-windows-msvc"
$targetDir = "C:\cargo-target"
$nsisPath  = "C:\nsis-3.10\makensis.exe"
$staging   = "C:\maolan-staging\daw"
$vcpkgRoot = "C:\vcpkg"

# ---------------------------------------------------------------------------
# Elevation check
# ---------------------------------------------------------------------------
$currentPrincipal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
$isAdmin = $currentPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Warning "This script is NOT running as Administrator."
    Write-Warning "Most installations (VS Build Tools, LLVM, NSIS to C:\, vcpkg) require elevation."
    Write-Warning "If installs fail, run PowerShell as Administrator or execute from an RDP/VNC session."
    Write-Host ""
}

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
function Test-Command {
    param([string]$Name)
    return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

function Ensure-Git {
    $gitPath = "$env:ProgramFiles\Git\cmd\git.exe"
    if (Test-Path $gitPath) {
        Write-Host "Git already installed at $gitPath"
        $env:PATH = "$env:ProgramFiles\Git\cmd;$env:PATH"
        return
    }
    if (Test-Command "git") {
        Write-Host "Git already installed."
        return
    }
    Write-Host "Installing Git..."
    $installer = "$env:TEMP\Git-installer.exe"
    if (-not (Test-Path $installer)) {
        Invoke-WebRequest -Uri "https://github.com/git-for-windows/git/releases/download/v2.49.0.windows.1/Git-2.49.0-64-bit.exe" -OutFile $installer
    }
    Start-Process -FilePath $installer -ArgumentList "/VERYSILENT","/NORESTART" -Wait
    $env:PATH = "$env:ProgramFiles\Git\cmd;$env:PATH"
}

function Ensure-CMake {
    $cmakePath = "C:\cmake\bin\cmake.exe"
    if (Test-Path $cmakePath) {
        Write-Host "CMake already installed."
        $env:PATH = "C:\cmake\bin;$env:PATH"
        return
    }
    if (Test-Command "cmake") {
        Write-Host "CMake already installed."
        return
    }
    Write-Host "Installing CMake..."
    $zip = "$env:TEMP\cmake.zip"
    Invoke-WebRequest -Uri "https://github.com/Kitware/CMake/releases/download/v3.31.5/cmake-3.31.5-windows-x86_64.zip" -OutFile $zip
    Expand-Archive -Path $zip -DestinationPath "C:\" -Force
    # The zip extracts to a nested folder; move it up
    $nested = "C:\cmake-3.31.5-windows-x86_64"
    if (Test-Path $nested) {
        Rename-Item -Path $nested -NewName "cmake" -Force
    }
    $env:PATH = "C:\cmake\bin;$env:PATH"
    if (-not (Test-Path $cmakePath)) {
        Write-Error "CMake installation failed. Expected cmake.exe at $cmakePath"
    }
}

function Ensure-VSBuildTools {
    $vsPath = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools"
    if (Test-Path "$vsPath\VC\Tools\MSVC") {
        Write-Host "VS Build Tools already installed."
        return
    }
    Write-Host "Installing Visual Studio 2022 Build Tools (this may take several minutes)..."
    $installer = "$env:TEMP\vs_BuildTools.exe"
    Invoke-WebRequest -Uri "https://aka.ms/vs/17/release/vs_BuildTools.exe" -OutFile $installer
    $proc = Start-Process -FilePath $installer -ArgumentList "--wait","--quiet","--add","Microsoft.VisualStudio.Workload.VCTools","--includeRecommended" -Wait -PassThru
    $exit = $proc.ExitCode
    Write-Host "VS Build Tools installer exited with code: $exit"
    if ($exit -eq 3010) {
        Write-Warning "A reboot is recommended after VS Build Tools installation."
    } elseif ($exit -ne 0) {
        Write-Error "VS Build Tools installation failed with exit code $exit"
    }
    if (-not (Test-Path "$vsPath\VC\Tools\MSVC")) {
        Write-Error "VS Build Tools directory not found after install. Expected: $vsPath\VC\Tools\MSVC"
    }
}

function Patch-FFmpegNext {
    $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { "$env:USERPROFILE\.cargo" }
    $registrySrc = "$cargoHome\registry\src"

    if (-not (Test-Path $registrySrc)) {
        Write-Host "Cargo registry not found at $registrySrc. Running cargo fetch first..."
        Push-Location $PSScriptRoot
        cargo fetch
        Pop-Location
        if (-not (Test-Path $registrySrc)) {
            Write-Host "Cargo registry still not found after fetch, skipping patch."
            return
        }
    }

    $ffmpegNextDir = (Get-ChildItem $registrySrc -Recurse -Filter "ffmpeg-next-8.1.0" -ErrorAction SilentlyContinue | Select-Object -First 1).FullName
    if (-not $ffmpegNextDir) {
        Write-Host "ffmpeg-next-8.1.0 not found in cargo registry, skipping patch."
        return
    }
    $frameFile = "$ffmpegNextDir\src\util\frame\side_data.rs"
    $packetFile = "$ffmpegNextDir\src\codec\packet\side_data.rs"
    $patched = $false

    if (Test-Path $frameFile) {
        $content = Get-Content $frameFile -Raw
        if (-not $content.Contains("DYNAMIC_HDR_SMPTE_2094_APP5")) {
            # Add enum variant
            $content = $content -replace '(#\[cfg\(feature = "ffmpeg_8_1"\)\]\s+EXIF,)', "`$1`n`n            #[cfg(feature = `"ffmpeg_8_1`")]`n            DYNAMIC_HDR_SMPTE_2094_APP5,"
            # Add From<AVFrameSideDataType> match arm
            $content = $content -replace '(#\[cfg\(feature = "ffmpeg_8_1"\)\]\s+AV_FRAME_DATA_EXIF => Type::EXIF,)', "`$1`n`n            #[cfg(feature = `"ffmpeg_8_1`")]`n            AV_FRAME_DATA_DYNAMIC_HDR_SMPTE_2094_APP5 => Type::DYNAMIC_HDR_SMPTE_2094_APP5,"
            # Add From<Type> match arm
            $content = $content -replace '(#\[cfg\(feature = "ffmpeg_8_1"\)\]\s+Type::EXIF => AV_FRAME_DATA_EXIF,)', "`$1`n`n            #[cfg(feature = `"ffmpeg_8_1`")]`n            Type::DYNAMIC_HDR_SMPTE_2094_APP5 => AV_FRAME_DATA_DYNAMIC_HDR_SMPTE_2094_APP5,"
            Set-Content $frameFile $content -NoNewline
            Write-Host "Patched ffmpeg-next frame side_data.rs"
            $patched = $true
        }
    }

    if (Test-Path $packetFile) {
        $content = Get-Content $packetFile -Raw
        if (-not $content.Contains("DYNAMIC_HDR_SMPTE_2094_APP5")) {
            # Add enum variant
            $content = $content -replace '(#\[cfg\(feature = "ffmpeg_8_1"\)\]\s+EXIF,)', "`$1`n`n            #[cfg(feature = `"ffmpeg_8_1`")]`n            DYNAMIC_HDR_SMPTE_2094_APP5,"
            # Add From<AVPacketSideDataType> match arm
            $content = $content -replace '(#\[cfg\(feature = "ffmpeg_8_1"\)\]\s+AV_PKT_DATA_EXIF => Type::EXIF,)', "`$1`n`n            #[cfg(feature = `"ffmpeg_8_1`")]`n            AV_PKT_DATA_DYNAMIC_HDR_SMPTE_2094_APP5 => Type::DYNAMIC_HDR_SMPTE_2094_APP5,"
            # Add From<Type> match arm
            $content = $content -replace '(#\[cfg\(feature = "ffmpeg_8_1"\)\]\s+Type::EXIF => AV_PKT_DATA_EXIF,)', "`$1`n`n            #[cfg(feature = `"ffmpeg_8_1`")]`n            Type::DYNAMIC_HDR_SMPTE_2094_APP5 => AV_PKT_DATA_DYNAMIC_HDR_SMPTE_2094_APP5,"
            Set-Content $packetFile $content -NoNewline
            Write-Host "Patched ffmpeg-next packet side_data.rs"
            $patched = $true
        }
    }

    if (-not $patched) {
        Write-Host "ffmpeg-next already patched or not found."
    }
}

function Import-VSEnv {
    $vsPath = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools"
    $vcvars = "$vsPath\VC\Auxiliary\Build\vcvarsall.bat"
    if (-not (Test-Path $vcvars)) {
        Write-Error "vcvarsall.bat not found. Ensure VS Build Tools are installed."
        return
    }
    Write-Host "Loading VS Build Tools environment..."
    $cmd = "`"$vcvars`" x64 && set"
    $envVars = cmd /c $cmd
    foreach ($line in $envVars) {
        if ($line -match '^(.*?)=(.*)$') {
            $name = $matches[1]
            $value = $matches[2]
            [Environment]::SetEnvironmentVariable($name, $value, "Process")
        }
    }
}

function Ensure-Rust {
    $cargoPath = "$env:USERPROFILE\.cargo\bin\cargo.exe"
    if (Test-Path $cargoPath) {
        Write-Host "Rust already installed at $cargoPath"
        $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
        return
    }
    if (Test-Command "cargo") {
        Write-Host "Rust already installed."
        return
    }
    Write-Host "Installing Rust..."
    $installer = "$env:TEMP\rustup-init.exe"
    if (-not (Test-Path $installer)) {
        Invoke-WebRequest -Uri "https://win.rustup.rs/x86_64" -OutFile $installer
    }
    & $installer -y --default-toolchain stable --target $target
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
}

function Ensure-NSIS {
    if (Test-Path $nsisPath) {
        Write-Host "NSIS already installed."
        return
    }
    Write-Host "Installing NSIS..."
    $zip = "$env:TEMP\nsis-3.10.zip"
    $curl = "$env:SystemRoot\System32\curl.exe"
    if (Test-Path $curl) {
        & $curl -L -o $zip "https://prdownloads.sourceforge.net/nsis/nsis-3.10.zip"
    } else {
        Invoke-WebRequest -Uri "https://prdownloads.sourceforge.net/nsis/nsis-3.10.zip" -OutFile $zip -MaximumRedirection 5
    }
    Expand-Archive -Path $zip -DestinationPath "C:\" -Force
    if (-not (Test-Path $nsisPath)) {
        $nested = "C:\nsis-3.10\nsis-3.10"
        if (Test-Path "$nested\makensis.exe") {
            Move-Item -Path $nested -Destination "C:\nsis-3.10-temp" -Force
            Remove-Item -Recurse -Force "C:\nsis-3.10" -ErrorAction SilentlyContinue
            Rename-Item -Path "C:\nsis-3.10-temp" -NewName "nsis-3.10"
        }
    }
    if (-not (Test-Path $nsisPath)) {
        Write-Error "NSIS installation failed. Expected makensis.exe at $nsisPath"
    }
}

function Ensure-Vcpkg {
    $vcpkgExe = "$vcpkgRoot\vcpkg.exe"
    if (Test-Path $vcpkgExe) {
        Write-Host "vcpkg already bootstrapped."
        return $vcpkgRoot
    }
    Write-Host "Bootstrapping vcpkg..."
    if (-not (Test-Path $vcpkgRoot)) {
        & "$env:ProgramFiles\Git\cmd\git.exe" clone https://github.com/microsoft/vcpkg $vcpkgRoot
    }
    & "$vcpkgRoot\bootstrap-vcpkg.bat" | Out-Null
    if (-not (Test-Path $vcpkgExe)) {
        Write-Error "Failed to bootstrap vcpkg."
    }
    return $vcpkgRoot
}

function Ensure-FFmpegNuGet {
    $ffmpegDir = "C:\ffmpeg-nuget\build\native"
    if (Test-Path "$ffmpegDir\include\libavcodec\avcodec.h") {
        Write-Host "FFmpeg NuGet package already installed."
        return $ffmpegDir
    }
    Write-Host "Downloading FFmpeg.LGPL NuGet package..."
    $nupkg = "$env:TEMP\FFmpeg.LGPL.nupkg"
    $curl = "$env:SystemRoot\System32\curl.exe"
    & $curl -L -o $nupkg "https://www.nuget.org/api/v2/package/FFmpeg.LGPL/20260504.1.0"
    if (-not (Test-Path $nupkg) -or (Get-Item $nupkg).Length -lt 1000000) {
        Write-Error "FFmpeg NuGet package download failed or too small."
    }
    Write-Host "Extracting FFmpeg NuGet package..."
    New-Item -ItemType Directory -Force "C:\ffmpeg-nuget" | Out-Null
    # .nupkg is a zip but Expand-Archive only accepts .zip extension
    $zip = "$env:TEMP\FFmpeg.LGPL.zip"
    Copy-Item $nupkg $zip -Force
    Expand-Archive -Path $zip -DestinationPath "C:\ffmpeg-nuget" -Force
    if (-not (Test-Path "$ffmpegDir\include\libavcodec\avcodec.h")) {
        Write-Error "FFmpeg NuGet package extraction failed."
    }
    Write-Host "FFmpeg NuGet package ready at $ffmpegDir"
    return $ffmpegDir
}

function Install-VcpkgPackage {
    param([string]$Root, [string]$Package)
    $name = $Package.Split(":")[0]
    $list = & { $ErrorActionPreference = "Continue"; & "$Root\vcpkg.exe" list $name } 2>$null
    if ($list -match "^$name\b") {
        Write-Host "$Package is already installed."
        return
    }
    Write-Host "Installing $Package via vcpkg (first run may take a long time)..."
    & "$Root\vcpkg.exe" install $Package
    if ($LASTEXITCODE -ne 0) {
        Write-Error "vcpkg install $Package failed"
    }
}

function Ensure-LLVM {
    $llvmPaths = @(
        "C:\LLVM\bin"
        "$env:ProgramFiles\LLVM\bin"
        "$env:ProgramFiles(x86)\LLVM\bin"
    )
    foreach ($path in $llvmPaths) {
        if (Test-Path "$path\libclang.dll") {
            $env:LIBCLANG_PATH = $path
            Write-Host "Found libclang at $path"
            return
        }
    }
    Write-Host "Installing LLVM..."
    $installer = "$env:TEMP\LLVM-installer.exe"
    if (-not (Test-Path $installer)) {
        Invoke-WebRequest -Uri "https://github.com/llvm/llvm-project/releases/download/llvmorg-19.1.0/LLVM-19.1.0-win64.exe" -OutFile $installer
    }
    Start-Process -FilePath $installer -ArgumentList "/S" -Wait
    # Some installers need a moment to finish writing files even after the process exits
    for ($i = 0; $i -lt 10; $i++) {
        foreach ($path in $llvmPaths) {
            if (Test-Path "$path\libclang.dll") {
                $env:LIBCLANG_PATH = $path
                Write-Host "Found libclang at $path after installation"
                return
            }
        }
        Start-Sleep -Seconds 2
    }
    Write-Error "LLVM installation completed but libclang.dll was not found."
}

# ---------------------------------------------------------------------------
# Main flow
# ---------------------------------------------------------------------------
Ensure-VSBuildTools
Import-VSEnv
Ensure-CMake
Ensure-Rust
Ensure-NSIS
Ensure-Git
$vcpkgRoot = Ensure-Vcpkg
$ffmpegDir = Ensure-FFmpegNuGet
Install-VcpkgPackage $vcpkgRoot "sentencepiece:x64-windows"
Ensure-LLVM

# ---------------------------------------------------------------------------
# Environment
# ---------------------------------------------------------------------------
$env:FFMPEG_DIR = $ffmpegDir
$env:VCPKG_ROOT = $vcpkgRoot
# Append vcpkg paths to existing VS environment paths (don't overwrite)
$env:LIB     = "$vcpkgRoot\installed\x64-windows\lib;$env:LIB"
$env:INCLUDE = "$vcpkgRoot\installed\x64-windows\include;$env:INCLUDE"

# ---------------------------------------------------------------------------
# VC++ Redistributable
# ---------------------------------------------------------------------------
$vcRedist = Join-Path (Split-Path $PSScriptRoot -Parent) "vc_redist.x64.exe"
if (-not (Test-Path $vcRedist)) {
    Write-Host "Downloading VC++ Redistributable..."
    Invoke-WebRequest -Uri "https://aka.ms/vs/17/release/vc_redist.x64.exe" -OutFile $vcRedist
}

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------
Patch-FFmpegNext

Write-Host "Building maolan (release)..."
Push-Location $PSScriptRoot
cargo build --release --target $target --target-dir $targetDir
Pop-Location

# ---------------------------------------------------------------------------
# Stage
# ---------------------------------------------------------------------------
Write-Host "Staging files to $staging..."
New-Item -ItemType Directory -Force $staging | Out-Null
Copy-Item "$targetDir\$target\release\maolan.exe"     $staging -Force
Copy-Item "$targetDir\$target\release\maolan-cli.exe" $staging -Force
Copy-Item "$ffmpegDir\bin\av*.dll" $staging -Force
Copy-Item "$ffmpegDir\bin\sw*.dll" $staging -Force
Copy-Item $vcRedist $staging -Force

# ---------------------------------------------------------------------------
# Installer
# ---------------------------------------------------------------------------
Write-Host "Building installer..."
# NSIS can't handle UNC paths, so copy script to local temp
$nsiTemp = "$env:TEMP\maolan-installer"
New-Item -ItemType Directory -Force $nsiTemp | Out-Null
Copy-Item "$PSScriptRoot\installer.nsi" "$nsiTemp\installer.nsi" -Force
Copy-Item "$PSScriptRoot\LICENSE" "$nsiTemp\LICENSE" -Force -ErrorAction SilentlyContinue
Push-Location $nsiTemp
& $nsisPath "$nsiTemp\installer.nsi"
Pop-Location
Copy-Item "$nsiTemp\maolan-setup.exe" "$PSScriptRoot\maolan-setup.exe" -Force -ErrorAction SilentlyContinue
if (Test-Path "$PSScriptRoot\maolan-setup.exe") {
    Write-Host "Done: $(Resolve-Path "$PSScriptRoot\maolan-setup.exe")"
} else {
    Write-Error "Installer build failed. maolan-setup.exe was not created."
}
