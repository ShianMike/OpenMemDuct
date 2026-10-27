param(
    [string]$BundleDir = 'src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis',
    [string]$Version,
    [switch]$SelfCheck
)

$ErrorActionPreference = 'Stop'

function Write-UpdaterArtifacts([string]$Directory, [string]$ReleaseVersion) {
    $parsedVersion = $null
    if (-not [System.Management.Automation.SemanticVersion]::TryParse($ReleaseVersion, [ref]$parsedVersion) -or
        $parsedVersion.ToString() -cne $ReleaseVersion) {
        throw 'Expected a canonical SemVer version without a v prefix.'
    }

    $expectedName = "OpenMemDuct_${ReleaseVersion}_x64-setup.exe"
    $installers = @(Get-ChildItem -LiteralPath $Directory -Filter *.exe -File)
    if ($installers.Count -ne 1 -or $installers[0].Name -cne $expectedName -or $installers[0].Length -eq 0) {
        throw "Expected exactly one non-empty NSIS installer: $expectedName"
    }

    $signaturePath = Join-Path $Directory "$expectedName.sig"
    if (-not (Test-Path -LiteralPath $signaturePath -PathType Leaf)) {
        throw "Missing Tauri updater signature: $expectedName.sig"
    }
    $signature = Get-Content -LiteralPath $signaturePath -Raw
    if ([string]::IsNullOrWhiteSpace($signature)) {
        throw "Empty Tauri updater signature: $expectedName.sig"
    }
    try {
        $signatureText = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($signature))
    } catch {
        throw "Expected a base64 Tauri updater signature: $expectedName.sig"
    }
    $signatureLines = $signatureText -split '\r?\n'
    if ($signatureLines.Count -lt 4 -or $signatureLines[2] -cnotlike 'trusted comment: *') {
        throw "Expected a Tauri updater signature trusted comment: $expectedName.sig"
    }
    # Packaging checks consistency; the native updater authenticates this comment with the signature.
    $signedVersion = $signatureLines[2].Substring('trusted comment: '.Length).Split("`t") |
        Where-Object { $_.StartsWith('version:') } | Select-Object -First 1
    if ($signedVersion -cne "version:$ReleaseVersion") {
        throw "Updater signature trusted comment must bind version $ReleaseVersion. Use tauri build or signer sign --app-version."
    }

    $manifestPath = Join-Path $Directory 'latest.json'
    [ordered]@{
        version = $ReleaseVersion
        pub_date = [DateTimeOffset]::UtcNow.ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", [Globalization.CultureInfo]::InvariantCulture)
        platforms = @{
            'windows-x86_64' = @{
                url = "https://github.com/ShianMike/OpenMemDuct/releases/download/v$ReleaseVersion/$expectedName"
                signature = $signature
            }
        }
    } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $manifestPath -Encoding utf8NoBOM

    @($installers[0].FullName, $signaturePath, $manifestPath) | ForEach-Object {
        $hash = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()
        "$hash  $([IO.Path]::GetFileName($_))"
    } | Set-Content -LiteralPath (Join-Path $Directory 'SHA256SUMS') -Encoding ascii
}

if ($SelfCheck) {
    function Assert-Rejected([scriptblock]$Action, [string]$ExpectedMessage) {
        try {
            & $Action
        } catch {
            if ($_.Exception.Message -notlike "*$ExpectedMessage*") { throw }
            return
        }
        throw "Self-check expected rejection: $ExpectedMessage"
    }

    $testRoot = Join-Path (Split-Path $PSScriptRoot -Parent) "validation/updater-artifacts-self-check/$([guid]::NewGuid())"
    $validDir = Join-Path $testRoot 'valid'
    $invalidDir = Join-Path $testRoot 'invalid'
    [void][IO.Directory]::CreateDirectory($validDir)
    [void][IO.Directory]::CreateDirectory($invalidDir)
    $testVersion = '1.2.3-rc.1+build.5'
    $installerName = "OpenMemDuct_${testVersion}_x64-setup.exe"
    $testSignatureText = "untrusted comment: fixture`nRkFLRQ==`ntrusted comment: timestamp:1`tfile:$installerName`tversion:$testVersion`nRkFLRQ==`n"
    $testSignature = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($testSignatureText))
    Set-Content -LiteralPath (Join-Path $validDir $installerName) -Value 'fixture installer' -Encoding ascii
    Set-Content -LiteralPath (Join-Path $validDir "$installerName.sig") -Value $testSignature -NoNewline -Encoding ascii
    Write-UpdaterArtifacts $validDir $testVersion

    $manifestJson = Get-Content -LiteralPath (Join-Path $validDir 'latest.json') -Raw
    $manifest = $manifestJson | ConvertFrom-Json -AsHashtable
    $platform = $manifest.platforms['windows-x86_64']
    if ($manifest.version -cne $testVersion -or $manifest.platforms.Count -ne 1 -or
        $platform.signature -cne $testSignature -or
        $platform.url -cne "https://github.com/ShianMike/OpenMemDuct/releases/download/v$testVersion/$installerName" -or
        $manifestJson -cnotmatch '"pub_date"\s*:\s*"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z"' -or
        [Math]::Abs(([DateTimeOffset]::UtcNow - [DateTimeOffset]$manifest.pub_date).TotalSeconds) -gt 60) {
        throw 'Self-check failed: updater manifest fields or publication date.'
    }
    $checksums = @(Get-Content -LiteralPath (Join-Path $validDir 'SHA256SUMS'))
    if ($checksums.Count -ne 3) { throw 'Self-check failed: expected three checksums.' }
    foreach ($name in @($installerName, "$installerName.sig", 'latest.json')) {
        $hash = (Get-FileHash -LiteralPath (Join-Path $validDir $name) -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($checksums -cnotcontains "$hash  $name") { throw "Self-check failed: checksum for $name." }
    }

    $invalidInstaller = Join-Path $invalidDir $installerName
    $invalidSignature = "$invalidInstaller.sig"
    Set-Content -LiteralPath $invalidInstaller -Value 'fixture installer' -Encoding ascii
    Assert-Rejected { Write-UpdaterArtifacts $invalidDir $testVersion } 'Missing Tauri updater signature'
    Set-Content -LiteralPath $invalidSignature -Value '' -NoNewline -Encoding ascii
    Assert-Rejected { Write-UpdaterArtifacts $invalidDir $testVersion } 'Empty Tauri updater signature'
    Set-Content -LiteralPath $invalidSignature -Value 'not a signature URL or path' -Encoding ascii
    Assert-Rejected { Write-UpdaterArtifacts $invalidDir $testVersion } 'Expected a base64 Tauri updater signature'
    Set-Content -LiteralPath $invalidSignature -Value 'c2VsZi1jaGVjayBzaWduYXR1cmU=' -NoNewline -Encoding ascii
    Assert-Rejected { Write-UpdaterArtifacts $invalidDir $testVersion } 'Expected a Tauri updater signature trusted comment'
    foreach ($comment in @(
        $testSignatureText.Replace("version:$testVersion", 'version:9.9.9'),
        $testSignatureText.Replace("`tversion:$testVersion", '')
    )) {
        $badSignature = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($comment))
        Set-Content -LiteralPath $invalidSignature -Value $badSignature -NoNewline -Encoding ascii
        Assert-Rejected { Write-UpdaterArtifacts $invalidDir $testVersion } 'Updater signature trusted comment must bind version'
    }
    Set-Content -LiteralPath $invalidInstaller -Value '' -NoNewline -Encoding ascii
    Assert-Rejected { Write-UpdaterArtifacts $invalidDir $testVersion } 'Expected exactly one non-empty NSIS installer'
    Set-Content -LiteralPath $invalidInstaller -Value 'fixture installer' -Encoding ascii
    Set-Content -LiteralPath (Join-Path $invalidDir 'extra.exe') -Value 'unexpected installer' -Encoding ascii
    Assert-Rejected { Write-UpdaterArtifacts $invalidDir $testVersion } 'Expected exactly one non-empty NSIS installer'
    foreach ($invalidVersion in @('', 'v1.2.3', '01.2.3', '1.2', '1.2.3-01', '../1.2.3')) {
        Assert-Rejected { Write-UpdaterArtifacts $validDir $invalidVersion } 'Expected a canonical SemVer version'
    }
    Write-Output "Updater artifact self-check passed. Fixtures retained at $testRoot"
    return
}

Write-UpdaterArtifacts $BundleDir $Version
