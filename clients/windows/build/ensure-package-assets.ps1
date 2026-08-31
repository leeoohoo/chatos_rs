[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$assetRoot = Join-Path $repoRoot "src\ChatOS.Desktop\Assets"
New-Item -ItemType Directory -Path $assetRoot -Force | Out-Null

Add-Type -AssemblyName System.Drawing

function Write-ChatOSAsset {
    param(
        [Parameter(Mandatory)] [string]$Name,
        [Parameter(Mandatory)] [int]$Width,
        [Parameter(Mandatory)] [int]$Height,
        [switch]$Transparent
    )

    $path = Join-Path $assetRoot $Name
    $bitmap = [System.Drawing.Bitmap]::new($Width, $Height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
        if ($Transparent) {
            $graphics.Clear([System.Drawing.Color]::Transparent)
        }
        else {
            $graphics.Clear([System.Drawing.Color]::FromArgb(22, 119, 255))
        }

        $side = [Math]::Min($Width, $Height)
        $bubbleWidth = [Math]::Max(12, [int]($side * 0.58))
        $bubbleHeight = [Math]::Max(10, [int]($side * 0.44))
        $bubbleX = [int](($Width - $bubbleWidth) / 2)
        $bubbleY = [int](($Height - $bubbleHeight) / 2 - $side * 0.03)
        $radius = [Math]::Max(3, [int]($side * 0.09))
        $pathShape = [System.Drawing.Drawing2D.GraphicsPath]::new()
        try {
            $diameter = $radius * 2
            $pathShape.AddArc($bubbleX, $bubbleY, $diameter, $diameter, 180, 90)
            $pathShape.AddArc($bubbleX + $bubbleWidth - $diameter, $bubbleY, $diameter, $diameter, 270, 90)
            $pathShape.AddArc($bubbleX + $bubbleWidth - $diameter, $bubbleY + $bubbleHeight - $diameter, $diameter, $diameter, 0, 90)
            $pathShape.AddLine(
                $bubbleX + [int]($bubbleWidth * 0.62),
                $bubbleY + $bubbleHeight,
                $bubbleX + [int]($bubbleWidth * 0.48),
                $bubbleY + $bubbleHeight + [int]($side * 0.12))
            $pathShape.AddLine(
                $bubbleX + [int]($bubbleWidth * 0.43),
                $bubbleY + $bubbleHeight,
                $bubbleX + $radius,
                $bubbleY + $bubbleHeight)
            $pathShape.AddArc($bubbleX, $bubbleY + $bubbleHeight - $diameter, $diameter, $diameter, 90, 90)
            $pathShape.CloseFigure()
            $bubbleColor = if ($Transparent) {
                [System.Drawing.Color]::FromArgb(22, 119, 255)
            }
            else {
                [System.Drawing.Color]::White
            }
            $brush = [System.Drawing.SolidBrush]::new($bubbleColor)
            try {
                $graphics.FillPath($brush, $pathShape)
            }
            finally {
                $brush.Dispose()
            }
        }
        finally {
            $pathShape.Dispose()
        }

        $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

Write-ChatOSAsset -Name "StoreLogo.png" -Width 50 -Height 50 -Transparent
Write-ChatOSAsset -Name "Square44x44Logo.png" -Width 44 -Height 44 -Transparent
Write-ChatOSAsset -Name "Square150x150Logo.png" -Width 150 -Height 150
Write-ChatOSAsset -Name "Wide310x150Logo.png" -Width 310 -Height 150
Write-ChatOSAsset -Name "Square310x310Logo.png" -Width 310 -Height 310
Write-ChatOSAsset -Name "SplashScreen.png" -Width 620 -Height 300

Write-Host "Package assets are ready in $assetRoot"
