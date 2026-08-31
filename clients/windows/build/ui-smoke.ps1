[CmdletBinding()]
param(
    [Parameter(Mandatory, ParameterSetName = "Executable")]
    [ValidateScript({ Test-Path $_ -PathType Leaf })]
    [string]$ExecutablePath,

    [Parameter(Mandatory, ParameterSetName = "Packaged")]
    [ValidateNotNullOrEmpty()]
    [string]$AppUserModelId,

    [ValidateNotNullOrEmpty()]
    [string]$ProcessName = "ChatOS.Desktop",

    [int]$StartupTimeoutSeconds = 30,

    [switch]$RequireAuthenticated
)

$ErrorActionPreference = "Stop"
if (-not $IsWindows) {
    throw "Windows UI automation must run on Windows."
}

$testUsername = [Environment]::GetEnvironmentVariable(
    "CHATOS_UI_TEST_USERNAME",
    [EnvironmentVariableTarget]::Process)
$testPassword = [Environment]::GetEnvironmentVariable(
    "CHATOS_UI_TEST_PASSWORD",
    [EnvironmentVariableTarget]::Process)
[Environment]::SetEnvironmentVariable(
    "CHATOS_UI_TEST_USERNAME",
    $null,
    [EnvironmentVariableTarget]::Process)
[Environment]::SetEnvironmentVariable(
    "CHATOS_UI_TEST_PASSWORD",
    $null,
    [EnvironmentVariableTarget]::Process)

$hasUsername = -not [string]::IsNullOrWhiteSpace($testUsername)
$hasPassword = -not [string]::IsNullOrWhiteSpace($testPassword)
if ($hasUsername -ne $hasPassword) {
    $testUsername = $null
    $testPassword = $null
    throw "Authenticated UI smoke requires both CHATOS_UI_TEST_USERNAME and CHATOS_UI_TEST_PASSWORD."
}
$hasCredentials = $hasUsername -and $hasPassword

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

function Find-AutomationElement {
    param(
        [Parameter(Mandatory)] [System.Windows.Automation.AutomationElement]$Root,
        [Parameter(Mandatory)] [string]$AutomationId
    )

    $condition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::AutomationIdProperty,
        $AutomationId)
    return $Root.FindFirst(
        [System.Windows.Automation.TreeScope]::Descendants,
        $condition)
}

function Wait-AutomationElement {
    param(
        [Parameter(Mandatory)] [System.Windows.Automation.AutomationElement]$Root,
        [Parameter(Mandatory)] [string]$AutomationId,
        [Parameter(Mandatory)] [DateTimeOffset]$Deadline
    )

    do {
        $element = Find-AutomationElement -Root $Root -AutomationId $AutomationId
        if ($element) {
            return $element
        }
        Start-Sleep -Milliseconds 200
    } while ([DateTimeOffset]::UtcNow -lt $Deadline)

    return $null
}

function Assert-AutomationElements {
    param(
        [Parameter(Mandatory)] [System.Windows.Automation.AutomationElement]$Root,
        [Parameter(Mandatory)] [object[]]$Expected
    )

    foreach ($item in $Expected) {
        $element = Find-AutomationElement -Root $Root -AutomationId $item.Id
        if (-not $element) {
            throw "Required automation element was not found: $($item.Id)"
        }
        if ($item.Type -and $element.Current.ControlType -ne $item.Type) {
            throw "Automation element $($item.Id) has unexpected control type $($element.Current.ControlType.ProgrammaticName)."
        }
        if ($item.RequireName -and [string]::IsNullOrWhiteSpace($element.Current.Name)) {
            throw "Automation element $($item.Id) has no accessible name."
        }
    }
}

function Set-AutomationValue {
    param(
        [Parameter(Mandatory)] [System.Windows.Automation.AutomationElement]$Element,
        [AllowEmptyString()] [string]$Value,
        [Parameter(Mandatory)] [string]$AutomationId
    )

    $patternObject = $null
    if (-not $Element.TryGetCurrentPattern(
            [System.Windows.Automation.ValuePattern]::Pattern,
            [ref]$patternObject)) {
        throw "Automation element does not support ValuePattern: $AutomationId"
    }
    $pattern = [System.Windows.Automation.ValuePattern]$patternObject
    if ($pattern.Current.IsReadOnly) {
        throw "Automation element is read-only: $AutomationId"
    }
    $pattern.SetValue($Value)
}

function Invoke-AutomationElement {
    param(
        [Parameter(Mandatory)] [System.Windows.Automation.AutomationElement]$Element,
        [Parameter(Mandatory)] [string]$AutomationId
    )

    $patternObject = $null
    if (-not $Element.TryGetCurrentPattern(
            [System.Windows.Automation.InvokePattern]::Pattern,
            [ref]$patternObject)) {
        throw "Automation element does not support InvokePattern: $AutomationId"
    }
    ([System.Windows.Automation.InvokePattern]$patternObject).Invoke()
}

function Assert-VisibleFocusableControlsHaveNames {
    param(
        [Parameter(Mandatory)] [System.Windows.Automation.AutomationElement]$Root
    )

    $interactive = $Root.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.Condition]::TrueCondition) |
        Where-Object {
            $_.Current.IsKeyboardFocusable -and
            $_.Current.IsEnabled -and
            $_.Current.IsOffscreen -eq $false
        }
    $unnamed = @($interactive | Where-Object { [string]::IsNullOrWhiteSpace($_.Current.Name) })
    if ($unnamed.Count -gt 0) {
        $details = ($unnamed | ForEach-Object {
            $id = $_.Current.AutomationId
            if ([string]::IsNullOrWhiteSpace($id)) { $id = "<no AutomationId>" }
            "$($_.Current.ControlType.ProgrammaticName) [$id]"
        }) -join ", "
        throw "Visible focusable controls without accessible names: $details"
    }
}

$process = $null
try {
    if ($PSCmdlet.ParameterSetName -eq "Executable") {
        $resolvedExecutable = (Resolve-Path $ExecutablePath).Path
        $process = Start-Process -FilePath $resolvedExecutable `
            -WorkingDirectory (Split-Path -Parent $resolvedExecutable) `
            -PassThru
    }
    else {
        $existingIds = @(Get-Process -Name $ProcessName -ErrorAction SilentlyContinue |
            ForEach-Object { $_.Id })
        Start-Process -FilePath "explorer.exe" -ArgumentList "shell:AppsFolder\$AppUserModelId"
        $launchDeadline = [DateTimeOffset]::UtcNow.AddSeconds($StartupTimeoutSeconds)
        do {
            Start-Sleep -Milliseconds 250
            $process = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue |
                Where-Object { $_.Id -notin $existingIds } |
                Sort-Object StartTime -Descending |
                Select-Object -First 1
        } while (-not $process -and [DateTimeOffset]::UtcNow -lt $launchDeadline)
        if (-not $process) {
            throw "The packaged ChatOS process did not start within $StartupTimeoutSeconds seconds."
        }
    }

    $deadline = [DateTimeOffset]::UtcNow.AddSeconds($StartupTimeoutSeconds)
    do {
        Start-Sleep -Milliseconds 250
        $process.Refresh()
        if ($process.HasExited) {
            throw "ChatOS.Desktop exited during startup with code $($process.ExitCode)."
        }
    } while ($process.MainWindowHandle -eq 0 -and [DateTimeOffset]::UtcNow -lt $deadline)

    if ($process.MainWindowHandle -eq 0) {
        throw "ChatOS.Desktop did not expose a main window within $StartupTimeoutSeconds seconds."
    }

    $root = [System.Windows.Automation.AutomationElement]::FromHandle(
        [IntPtr]$process.MainWindowHandle)
    if (-not $root) {
        throw "UI Automation could not attach to the ChatOS main window."
    }

    $loginExpected = @(
        @{ Id = "ChatOS.Login.Root"; Type = $null; RequireName = $true },
        @{ Id = "ChatOS.Login.Username"; Type = [System.Windows.Automation.ControlType]::Edit; RequireName = $true },
        @{ Id = "ChatOS.Login.Password"; Type = [System.Windows.Automation.ControlType]::Edit; RequireName = $true },
        @{ Id = "ChatOS.Login.Submit"; Type = [System.Windows.Automation.ControlType]::Button; RequireName = $true }
    )
    $shellExpected = @(
        @{ Id = "ChatOS.Shell.Root"; Type = $null; RequireName = $true },
        @{ Id = "ChatOS.Shell.Notepad"; Type = [System.Windows.Automation.ControlType]::Button; RequireName = $true },
        @{ Id = "ChatOS.Shell.Artifacts"; Type = [System.Windows.Automation.ControlType]::Button; RequireName = $true },
        @{ Id = "ChatOS.Shell.AccountMenu"; Type = [System.Windows.Automation.ControlType]::Button; RequireName = $true },
        @{ Id = "ChatOS.Shell.Refresh"; Type = [System.Windows.Automation.ControlType]::Button; RequireName = $true },
        @{ Id = "ChatOS.Shell.CreateResource"; Type = [System.Windows.Automation.ControlType]::Button; RequireName = $true },
        @{ Id = "ChatOS.Shell.Contacts"; Type = [System.Windows.Automation.ControlType]::List; RequireName = $true },
        @{ Id = "ChatOS.Shell.Projects"; Type = [System.Windows.Automation.ControlType]::List; RequireName = $true },
        @{ Id = "ChatOS.Shell.LocalResources"; Type = [System.Windows.Automation.ControlType]::List; RequireName = $true },
        @{ Id = "ChatOS.Shell.RemoteResources"; Type = [System.Windows.Automation.ControlType]::List; RequireName = $true },
        @{ Id = "ChatOS.Shell.Workspace"; Type = $null; RequireName = $false }
    )

    $shell = Find-AutomationElement -Root $root -AutomationId "ChatOS.Shell.Root"
    if (-not $shell) {
        Assert-AutomationElements -Root $root -Expected $loginExpected
        if (-not $RequireAuthenticated) {
            Assert-VisibleFocusableControlsHaveNames -Root $root
            Write-Host "ChatOS Windows anonymous login UI automation and accessibility smoke test passed."
            return
        }
        if (-not $hasCredentials) {
            throw "Authenticated UI smoke was requested, but no test credentials or existing signed-in session are available."
        }

        $usernameElement = Find-AutomationElement -Root $root -AutomationId "ChatOS.Login.Username"
        $passwordElement = Find-AutomationElement -Root $root -AutomationId "ChatOS.Login.Password"
        $submitElement = Find-AutomationElement -Root $root -AutomationId "ChatOS.Login.Submit"
        Set-AutomationValue -Element $usernameElement -Value $testUsername -AutomationId "ChatOS.Login.Username"
        Set-AutomationValue -Element $passwordElement -Value $testPassword -AutomationId "ChatOS.Login.Password"
        Invoke-AutomationElement -Element $submitElement -AutomationId "ChatOS.Login.Submit"

        $shell = Wait-AutomationElement `
            -Root $root `
            -AutomationId "ChatOS.Shell.Root" `
            -Deadline ([DateTimeOffset]::UtcNow.AddSeconds($StartupTimeoutSeconds))
        if (-not $shell) {
            throw "ChatOS did not reach the authenticated shell after sign-in."
        }
    }

    Assert-AutomationElements -Root $root -Expected $shellExpected

    $accountMenu = Find-AutomationElement -Root $root -AutomationId "ChatOS.Shell.AccountMenu"
    Invoke-AutomationElement -Element $accountMenu -AutomationId "ChatOS.Shell.AccountMenu"
    $desktopRoot = [System.Windows.Automation.AutomationElement]::RootElement
    $settingsMenuItem = Wait-AutomationElement `
        -Root $desktopRoot `
        -AutomationId "ChatOS.Shell.Settings" `
        -Deadline ([DateTimeOffset]::UtcNow.AddSeconds(5))
    if (-not $settingsMenuItem) {
        throw "The account menu did not expose the settings command."
    }
    Invoke-AutomationElement -Element $settingsMenuItem -AutomationId "ChatOS.Shell.Settings"

    $settingsRoot = Wait-AutomationElement `
        -Root $root `
        -AutomationId "ChatOS.Settings.Root" `
        -Deadline ([DateTimeOffset]::UtcNow.AddSeconds(5))
    if (-not $settingsRoot) {
        throw "The settings page did not open from the account menu."
    }
    Assert-AutomationElements -Root $root -Expected @(
        @{ Id = "ChatOS.Settings.Root"; Type = $null; RequireName = $true },
        @{ Id = "ChatOS.Settings.Back"; Type = [System.Windows.Automation.ControlType]::Button; RequireName = $true },
        @{ Id = "ChatOS.Settings.Language"; Type = [System.Windows.Automation.ControlType]::ComboBox; RequireName = $true },
        @{ Id = "ChatOS.Settings.Theme"; Type = [System.Windows.Automation.ControlType]::ComboBox; RequireName = $true },
        @{ Id = "ChatOS.Settings.FontScale"; Type = [System.Windows.Automation.ControlType]::Slider; RequireName = $true },
        @{ Id = "ChatOS.Settings.PetEnabled"; Type = [System.Windows.Automation.ControlType]::Button; RequireName = $true },
        @{ Id = "ChatOS.Settings.SandboxEnabled"; Type = [System.Windows.Automation.ControlType]::Button; RequireName = $true },
        @{ Id = "ChatOS.Settings.SandboxProfile"; Type = [System.Windows.Automation.ControlType]::ComboBox; RequireName = $true },
        @{ Id = "ChatOS.Settings.SandboxNetwork"; Type = [System.Windows.Automation.ControlType]::ComboBox; RequireName = $true },
        @{ Id = "ChatOS.Settings.ApprovalMode"; Type = [System.Windows.Automation.ControlType]::ComboBox; RequireName = $true }
    )

    $chatRoot = Find-AutomationElement -Root $root -AutomationId "ChatOS.Chat.Root"
    if ($chatRoot) {
        Assert-AutomationElements -Root $root -Expected @(
            @{ Id = "ChatOS.Chat.ModelPicker"; Type = [System.Windows.Automation.ControlType]::ComboBox; RequireName = $true },
            @{ Id = "ChatOS.Chat.Composer"; Type = [System.Windows.Automation.ControlType]::Edit; RequireName = $true },
            @{ Id = "ChatOS.Chat.Send"; Type = [System.Windows.Automation.ControlType]::Button; RequireName = $true }
        )
    }

    Assert-VisibleFocusableControlsHaveNames -Root $root
    Write-Host "ChatOS Windows authenticated shell, settings navigation, UI automation, and accessibility smoke test passed."
}
finally {
    $testUsername = $null
    $testPassword = $null
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    }
}
