param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Release",
    [string]$Version = "0.2.7.0",
    [string]$ArtifactVersion = "0.2.7",
    [string]$RuntimeIdentifier = "win-x64",
    [switch]$SkipRust,
    [switch]$SkipPackage
)

$ErrorActionPreference = "Stop"

$RootDir = Resolve-Path (Join-Path $PSScriptRoot "..")
$AppDir = Join-Path $RootDir "apps/windows/SecureChatWindows"
$Project = Join-Path $AppDir "SecureChatWindows.csproj"
$NativeDir = Join-Path $AppDir "Native"
$DistDir = Join-Path $RootDir "dist"
$Target = "x86_64-pc-windows-msvc"
$CargoProfile = if ($Configuration -eq "Release") { "release" } else { "debug" }
$CargoArgs = @("build", "--locked", "-p", "secure-chat-ffi", "--target", $Target)
if ($Configuration -eq "Release") {
    $CargoArgs += "--release"
}

function Find-CommandOrFail($Name) {
    $Command = Get-Command $Name -ErrorAction SilentlyContinue
    if (-not $Command) {
        throw "$Name was not found on PATH."
    }
    return $Command.Source
}

New-Item -ItemType Directory -Force -Path $NativeDir, $DistDir | Out-Null

if (-not $SkipRust) {
    Find-CommandOrFail "rustup" | Out-Null
    Find-CommandOrFail "cargo" | Out-Null
    rustup target add $Target | Out-Null
    cargo @CargoArgs
    $Dll = Join-Path $RootDir "target/$Target/$CargoProfile/secure_chat_ffi.dll"
    if (-not (Test-Path $Dll)) {
        throw "Rust FFI DLL was not produced at $Dll"
    }
    Copy-Item $Dll (Join-Path $NativeDir "secure_chat_ffi.dll") -Force
}

Find-CommandOrFail "dotnet" | Out-Null

$Publisher = "CN=SecureChat Local Release, O=SecureChat"
$PfxPath = Join-Path $AppDir "SecureChatWindows_TemporaryKey.pfx"
$CertPath = Join-Path $DistDir "SecureChatWindows-TemporaryKey.cer"
$PlainPassword = if ($env:SECURE_CHAT_WINDOWS_CERT_PASSWORD) { $env:SECURE_CHAT_WINDOWS_CERT_PASSWORD } else { "securechat-local" }
$SecurePassword = ConvertTo-SecureString $PlainPassword -AsPlainText -Force

if (-not (Test-Path $PfxPath)) {
    $Cert = New-SelfSignedCertificate `
        -Type Custom `
        -Subject $Publisher `
        -FriendlyName "SecureChat Windows Test Signing" `
        -KeyUsage DigitalSignature `
        -CertStoreLocation "Cert:\CurrentUser\My" `
        -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3")
    Export-PfxCertificate -Cert $Cert -FilePath $PfxPath -Password $SecurePassword | Out-Null
    Export-Certificate -Cert $Cert -FilePath $CertPath | Out-Null
}

dotnet restore $Project

if (-not $SkipPackage) {
    dotnet publish $Project `
        -c $Configuration `
        -r $RuntimeIdentifier `
        /p:PackageVersion=$Version `
        /p:AppxBundle=Never `
        /p:GenerateAppxPackageOnBuild=true `
        /p:PackageCertificateKeyFile=$PfxPath `
        /p:PackageCertificatePassword=$PlainPassword

    $Msix = Get-ChildItem -Path (Join-Path $AppDir "bin/$Configuration") -Recurse -Filter "*.msix" |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $Msix) {
        throw "MSIX package was not produced."
    }

    $OutMsix = Join-Path $DistDir "SecureChatWindows-$ArtifactVersion.msix"
    Copy-Item $Msix.FullName $OutMsix -Force
    $Hash = Get-FileHash -Algorithm SHA256 $OutMsix
    "$($Hash.Hash.ToLowerInvariant())  $(Split-Path $OutMsix -Leaf)" |
        Set-Content -Encoding ascii (Join-Path $DistDir "SecureChatWindows-$ArtifactVersion.msix.sha256")

    Write-Host "msix=$OutMsix"
    Write-Host "sha256=$($Hash.Hash.ToLowerInvariant())"
    if (Test-Path $CertPath) {
        Write-Host "test_certificate=$CertPath"
    }
} else {
    dotnet build $Project -c $Configuration -r $RuntimeIdentifier /p:PackageVersion=$Version
}
