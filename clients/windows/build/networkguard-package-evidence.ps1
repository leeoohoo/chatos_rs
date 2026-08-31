function Assert-NetworkGuardPackageManifest {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$PackageDirectory
    )

    if (-not (Test-Path $PackageDirectory -PathType Container)) {
        throw "NetworkGuard package directory was not found."
    }
    $package = (Resolve-Path $PackageDirectory).Path
    $manifestPath = Join-Path $package "manifest.json"
    if (-not (Test-Path $manifestPath -PathType Leaf)) {
        throw "NetworkGuard package manifest.json was not found."
    }
    try {
        $manifest = @(Get-Content $manifestPath -Raw | ConvertFrom-Json)
    }
    catch {
        throw "NetworkGuard package manifest is not valid JSON."
    }
    if ($manifest.Count -lt 4) {
        throw "NetworkGuard package manifest is incomplete."
    }

    $packagePrefix = $package.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $seenPaths = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($entry in $manifest) {
        $relativePath = [string]$entry.path
        if ([string]::IsNullOrWhiteSpace($relativePath) -or
            [IO.Path]::IsPathRooted($relativePath) -or
            $relativePath -match '^[A-Za-z]:' -or
            $relativePath.StartsWith('\\') -or
            $relativePath.Replace('\', '/').Split('/') -contains '..') {
            throw "NetworkGuard package manifest contains an invalid path."
        }
        $normalizedPath = $relativePath.Replace('\', [IO.Path]::DirectorySeparatorChar).Replace('/', [IO.Path]::DirectorySeparatorChar)
        $candidate = [IO.Path]::GetFullPath((Join-Path $package $normalizedPath))
        if (-not $candidate.StartsWith($packagePrefix, [StringComparison]::OrdinalIgnoreCase) -or
            -not (Test-Path $candidate -PathType Leaf)) {
            throw "NetworkGuard package manifest references a missing or escaped file."
        }
        $canonicalRelativePath = $candidate.Substring($packagePrefix.Length).Replace('\', '/')
        if (-not $seenPaths.Add($canonicalRelativePath)) {
            throw "NetworkGuard package manifest contains a duplicate path."
        }
        $file = Get-Item $candidate
        $hash = (Get-FileHash $candidate -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($file.Length -ne [long]$entry.length -or
            [string]::IsNullOrWhiteSpace([string]$entry.sha256) -or
            $hash -ine [string]$entry.sha256) {
            throw "NetworkGuard package integrity verification failed for $relativePath."
        }
    }

    foreach ($requiredPath in @(
        "driver/ChatOS.NetworkGuard.Driver.sys",
        "driver/ChatOS.NetworkGuard.Driver.cat",
        "driver/ChatOS.NetworkGuard.Driver.inf",
        "service/ChatOS.NetworkGuard.Service.exe")) {
        if (-not $seenPaths.Contains($requiredPath)) {
            throw "NetworkGuard package manifest is missing required artifact $requiredPath."
        }
    }
    return $manifest
}
