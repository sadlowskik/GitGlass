# Windows Authenticode signing hook (STUB).
#
# Tauri calls this per-artifact when `bundle.windows.signCommand` is configured.
# Until a real cert is supplied it runs in STUB mode: it logs what it *would*
# sign and exits 0 so unsigned dev/CI builds succeed. When the cert arrives,
# provide WINDOWS_CERT_BASE64 + WINDOWS_CERT_PASSWORD (and a signtool on PATH)
# and this script signs for real. See docs/SIGNING.md.
#
# Usage (configured in tauri.conf.json later): sign-windows.ps1 -Path <file>

param(
    [Parameter(Mandatory = $true)]
    [string]$Path
)

$ErrorActionPreference = "Stop"

$certB64 = $env:WINDOWS_CERT_BASE64
$certPwd = $env:WINDOWS_CERT_PASSWORD

if ([string]::IsNullOrWhiteSpace($certB64)) {
    Write-Host "[sign-windows] STUB MODE — no WINDOWS_CERT_BASE64 set."
    Write-Host "[sign-windows] Would sign: $Path"
    Write-Host "[sign-windows] Skipping (build remains UNSIGNED). This is expected in dev/CI."
    exit 0
}

Write-Host "[sign-windows] Real signing: $Path"

$tmpCert = Join-Path $env:RUNNER_TEMP "gitglass-cert.pfx"
try {
    [IO.File]::WriteAllBytes($tmpCert, [Convert]::FromBase64String($certB64))

    $signtool = Get-Command signtool.exe -ErrorAction SilentlyContinue
    if (-not $signtool) {
        throw "signtool.exe not found on PATH. Install the Windows SDK."
    }

    & $signtool.Source sign `
        /fd SHA256 `
        /tr http://timestamp.digicert.com `
        /td SHA256 `
        /f $tmpCert `
        /p $certPwd `
        $Path

    if ($LASTEXITCODE -ne 0) { throw "signtool failed with exit code $LASTEXITCODE" }
    Write-Host "[sign-windows] Signed OK."
}
finally {
    if (Test-Path $tmpCert) { Remove-Item $tmpCert -Force }
}
