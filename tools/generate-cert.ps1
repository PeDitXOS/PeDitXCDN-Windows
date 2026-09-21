# PeDitXCDN - Generate + Auto-install Certificate
# Run this ONCE on your build machine

$certName = "PeDitXCDN Code Signing"
$toolsDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$pfxPath = Join-Path $toolsDir "certificate.pfx"
$cerPath = Join-Path $toolsDir "certificate.cer"

# Generate self-signed certificate
$cert = New-SelfSignedCertificate `
    -Type CodeSigningCert `
    -Subject "CN=$certName" `
    -CertStoreLocation "Cert:\CurrentUser\My" `
    -KeyUsage DigitalSignature `
    -KeyAlgorithm RSA `
    -KeyLength 2048 `
    -KeyExportPolicy Exportable `
    -NotAfter (Get-Date).AddYears(3)

# Export PFX (for signing in CI)
Export-PfxCertificate -Cert $cert -FilePath $pfxPath -Password (New-Object Security.SecureString)

# Export CER (for embedding in installer)
Export-Certificate -Cert $cert -FilePath $cerPath -Type CERT

Write-Host "Done! Files created:" -ForegroundColor Green
Write-Host "  $pfxPath (for CI signing)"
Write-Host "  $cerPath (for installer)"
