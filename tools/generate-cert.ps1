# PeDitXCDN Code Signing Certificate Generator
# Run this on Windows PowerShell (Admin)

$certName = "PeDitXCDN Code Signing"
$outputPath = "$PSScriptRoot\certificate.pfx"

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

# Export to PFX (without password for simplicity)
$pfxPassword = ConvertTo-SecureString -String "" -Force -AsPlainText
Export-PfxCertificate `
    -Cert $cert `
    -FilePath $outputPath `
    -Password $pfxPassword

# Convert to Base64 for GitHub Secrets
$certBase64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($outputPath))

Write-Host "========================================" -ForegroundColor Green
Write-Host "Certificate created successfully!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green
Write-Host ""
Write-Host "Certificate Thumbprint: $($cert.Thumbprint)" -ForegroundColor Yellow
Write-Host "PFX File: $outputPath" -ForegroundColor Yellow
Write-Host ""
Write-Host "For GitHub Actions, add this secret:" -ForegroundColor Cyan
Write-Host "  CERTIFICATE_BASE64 = (contents of certificate.pfx as base64)" -ForegroundColor White
Write-Host ""
Write-Host "To sign on your machine:" -ForegroundColor Cyan
Write-Host '  signtool sign /f certificate.pfx /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 file.exe' -ForegroundColor White
