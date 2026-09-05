$ErrorActionPreference = 'Stop'
$toolsDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$packageArgs = @{
    packageName   = 'rustbox'
    fileType      = 'exe'
    url           = 'https://github.com/mlm-games/rustbox/releases/download/v0.2.0/rustbox-desktop-windows-x86_64.exe'
    softwareName  = 'rustbox*'
    checksum      = '' # filled by CI on release: `Get-FileHash -Algorithm SHA256`
    checksumType  = 'sha256'
}
Install-ChocolateyPackage @packageArgs
