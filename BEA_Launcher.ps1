# ==============================================================================
# BEA_Launcher.ps1 - BlueEngineAntigravity (BEA) Dedicated Map Launcher GUI
# Blue Engine v2 Environments: Suburban House, School Wing, Corporate Office,
# Convenience Store, and Studio Room.
# Features: Scientist (Wrench & Black Pistol) and Feta (Lab Rat).
# ==============================================================================

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$BEA_DIR = "C:\Users\TheNa\.gemini\antigravity\scratch\BlueEngineAntigravity"

$form = New-Object System.Windows.Forms.Form
$form.Text = "BEA - Blue Engine Map Launcher"
$form.Size = New-Object System.Drawing.Size(860, 620)
$form.StartPosition = "CenterScreen"
$form.FormBorderStyle = "FixedDialog"
$form.MaximizeBox = $false
$form.BackColor = [System.Drawing.Color]::FromArgb(14, 18, 26) # Dark Navy #0E121A
$form.ForeColor = [System.Drawing.Color]::FromArgb(230, 237, 243)

# Fonts
$fontHeader = New-Object System.Drawing.Font("Segoe UI", 16, [System.Drawing.FontStyle]::Bold)
$fontSub    = New-Object System.Drawing.Font("Segoe UI", 9, [System.Drawing.FontStyle]::Regular)
$fontGroup  = New-Object System.Drawing.Font("Segoe UI", 11, [System.Drawing.FontStyle]::Bold)
$fontBtn    = New-Object System.Drawing.Font("Segoe UI", 10, [System.Drawing.FontStyle]::Bold)
$fontTitle  = New-Object System.Drawing.Font("Segoe UI", 11, [System.Drawing.FontStyle]::Bold)
$fontBadge  = New-Object System.Drawing.Font("Segoe UI", 8, [System.Drawing.FontStyle]::Bold)
$fontDesc   = New-Object System.Drawing.Font("Segoe UI", 9, [System.Drawing.FontStyle]::Regular)

# Header Title
$lblTitle = New-Object System.Windows.Forms.Label
$lblTitle.Text = "BLUE ENGINE ANTIGRAVITY (BEA)"
$lblTitle.Font = $fontHeader
$lblTitle.ForeColor = [System.Drawing.Color]::FromArgb(88, 166, 255)
$lblTitle.Location = New-Object System.Drawing.Point(24, 16)
$lblTitle.Size = New-Object System.Drawing.Size(600, 30)
$form.Controls.Add($lblTitle)

$lblSub = New-Object System.Windows.Forms.Label
$lblSub.Text = "Blue Engine v2 Map Launcher  |  The Scientist (Wrench & Black Pistol) & Feta"
$lblSub.Font = $fontSub
$lblSub.ForeColor = [System.Drawing.Color]::FromArgb(139, 148, 158)
$lblSub.Location = New-Object System.Drawing.Point(26, 46)
$lblSub.Size = New-Object System.Drawing.Size(600, 20)
$form.Controls.Add($lblSub)

# Character Selection Panel
$pnlMode = New-Object System.Windows.Forms.Panel
$pnlMode.Location = New-Object System.Drawing.Point(24, 74)
$pnlMode.Size = New-Object System.Drawing.Size(796, 50)
$pnlMode.BackColor = [System.Drawing.Color]::FromArgb(22, 27, 34)
$pnlMode.BorderStyle = "FixedSingle"

$lblMode = New-Object System.Windows.Forms.Label
$lblMode.Text = "CHARACTER:"
$lblMode.Font = $fontBtn
$lblMode.ForeColor = [System.Drawing.Color]::FromArgb(88, 166, 255)
$lblMode.Location = New-Object System.Drawing.Point(12, 13)
$lblMode.Size = New-Object System.Drawing.Size(95, 24)
$pnlMode.Controls.Add($lblMode)

$rbScientist = New-Object System.Windows.Forms.RadioButton
$rbScientist.Text = "The Scientist (Wrench & Black Pistol - Mouse Scroll)"
$rbScientist.Font = $fontBtn
$rbScientist.ForeColor = [System.Drawing.Color]::FromArgb(230, 237, 243)
$rbScientist.Location = New-Object System.Drawing.Point(115, 12)
$rbScientist.Size = New-Object System.Drawing.Size(390, 24)
$rbScientist.Checked = $true
$pnlMode.Controls.Add($rbScientist)

$rbFeta = New-Object System.Windows.Forms.RadioButton
$rbFeta.Text = "Feta (Lab Rat - Scurry & Passages)"
$rbFeta.Font = $fontBtn
$rbFeta.ForeColor = [System.Drawing.Color]::FromArgb(230, 237, 243)
$rbFeta.Location = New-Object System.Drawing.Point(520, 12)
$rbFeta.Size = New-Object System.Drawing.Size(260, 24)
$pnlMode.Controls.Add($rbFeta)

$form.Controls.Add($pnlMode)

# Map Cards Container
$pnlMaps = New-Object System.Windows.Forms.Panel
$pnlMaps.Location = New-Object System.Drawing.Point(24, 134)
$pnlMaps.Size = New-Object System.Drawing.Size(796, 380)
$pnlMaps.AutoScroll = $true

$maps = @(
    @{
        Title = "Suburban House"
        Badge = "RESIDENCE"
        BadgeColor = [System.Drawing.Color]::FromArgb(56, 139, 253)
        Desc = "Two-story home with kitchen, living room, bedrooms, yard landscaping, and interior props."
        MapArg = "--map assets\maps\starters\house.json"
        Accent = [System.Drawing.Color]::FromArgb(56, 139, 253)
    },
    @{
        Title = "School Wing"
        Badge = "INSTITUTION"
        BadgeColor = [System.Drawing.Color]::FromArgb(63, 185, 80)
        Desc = "Modern school corridor with classrooms, study desks, reading boards, and storage bins."
        MapArg = "--map assets\maps\starters\school-wing.json"
        Accent = [System.Drawing.Color]::FromArgb(63, 185, 80)
    },
    @{
        Title = "Corporate Office"
        Badge = "COMMERCIAL"
        BadgeColor = [System.Drawing.Color]::FromArgb(163, 113, 247)
        Desc = "Spacious office layout with conference tables, ergonomic seating, desks, and file cabinets."
        MapArg = "--map assets\maps\starters\office.json"
        Accent = [System.Drawing.Color]::FromArgb(163, 113, 247)
    },
    @{
        Title = "Convenience Store"
        Badge = "RETAIL"
        BadgeColor = [System.Drawing.Color]::FromArgb(210, 153, 34)
        Desc = "Retail market with shopping aisles, snack bags, beverage bottles, register counter, and coolers."
        MapArg = "--map assets\maps\starters\convenience-store.json"
        Accent = [System.Drawing.Color]::FromArgb(210, 153, 34)
    },
    @{
        Title = "Studio Room"
        Badge = "SANDBOX"
        BadgeColor = [System.Drawing.Color]::FromArgb(139, 148, 158)
        Desc = "Native lighting and physics studio sandbox with test props and target surfaces."
        MapArg = "--studio"
        Accent = [System.Drawing.Color]::FromArgb(139, 148, 158)
    }
)

$cardY = 0
foreach ($m in $maps) {
    $card = New-Object System.Windows.Forms.Panel
    $card.Location = New-Object System.Drawing.Point(0, $cardY)
    $card.Size = New-Object System.Drawing.Size(770, 68)
    $card.BackColor = [System.Drawing.Color]::FromArgb(22, 27, 34)
    $card.BorderStyle = "FixedSingle"

    # Accent Stripe
    $stripe = New-Object System.Windows.Forms.Panel
    $stripe.Location = New-Object System.Drawing.Point(0, 0)
    $stripe.Size = New-Object System.Drawing.Size(5, 68)
    $stripe.BackColor = $m.Accent
    $card.Controls.Add($stripe)

    # Title
    $lblMTitle = New-Object System.Windows.Forms.Label
    $lblMTitle.Text = $m.Title
    $lblMTitle.Font = $fontTitle
    $lblMTitle.ForeColor = [System.Drawing.Color]::FromArgb(240, 246, 252)
    $lblMTitle.Location = New-Object System.Drawing.Point(16, 10)
    $lblMTitle.Size = New-Object System.Drawing.Size(220, 22)
    $card.Controls.Add($lblMTitle)

    # Badge
    $lblBadge = New-Object System.Windows.Forms.Label
    $lblBadge.Text = "  " + $m.Badge + "  "
    $lblBadge.Font = $fontBadge
    $lblBadge.ForeColor = [System.Drawing.Color]::White
    $lblBadge.BackColor = $m.BadgeColor
    $lblBadge.Location = New-Object System.Drawing.Point(240, 11)
    $lblBadge.Size = New-Object System.Drawing.Size(100, 18)
    $lblBadge.TextAlign = [System.Drawing.ContentAlignment]::MiddleCenter
    $card.Controls.Add($lblBadge)

    # Description
    $lblDesc = New-Object System.Windows.Forms.Label
    $lblDesc.Text = $m.Desc
    $lblDesc.Font = $fontDesc
    $lblDesc.ForeColor = [System.Drawing.Color]::FromArgb(139, 148, 158)
    $lblDesc.Location = New-Object System.Drawing.Point(16, 35)
    $lblDesc.Size = New-Object System.Drawing.Size(580, 20)
    $card.Controls.Add($lblDesc)

    # Launch Button
    $btnLaunch = New-Object System.Windows.Forms.Button
    $btnLaunch.Text = "PLAY"
    $btnLaunch.Font = $fontBtn
    $btnLaunch.ForeColor = [System.Drawing.Color]::White
    $btnLaunch.BackColor = [System.Drawing.Color]::FromArgb(35, 134, 54) # Green #238636
    $btnLaunch.FlatStyle = "Flat"
    $btnLaunch.FlatAppearance.BorderSize = 0
    $btnLaunch.Location = New-Object System.Drawing.Point(645, 14)
    $btnLaunch.Size = New-Object System.Drawing.Size(105, 38)
    $btnLaunch.Cursor = [System.Windows.Forms.Cursors]::Hand

    $mapArgVal = $m.MapArg
    $btnLaunch.Add_Click({
        $charArg = if ($rbFeta.Checked) { " --feta --third-person" } else { "" }
        $fullArgs = "$mapArgVal$charArg"
        
        $exe = Join-Path $BEA_DIR "bin\BEA.exe"
        if (-not (Test-Path $exe)) {
            $exe = Join-Path $BEA_DIR "bin\BE2.exe"
        }
        if (-not (Test-Path $exe)) {
            $exe = Join-Path $BEA_DIR "target\release\be2.exe"
        }
        
        if (Test-Path $exe) {
            Start-Process -FilePath $exe -ArgumentList $fullArgs -WorkingDirectory $BEA_DIR
        } else {
            Start-Process -FilePath "cargo" -ArgumentList "run --release --bin be2 -- $fullArgs" -WorkingDirectory $BEA_DIR
        }
    }.GetNewClosure())

    $card.Controls.Add($btnLaunch)
    $pnlMaps.Controls.Add($card)
    $cardY += 74
}

$form.Controls.Add($pnlMaps)

# Footer Controls Info
$lblFooter = New-Object System.Windows.Forms.Label
$lblFooter.Text = "CONTROLS: WASD to Move | Shift to Sprint | Space to Jump | C/Ctrl to Crouch | Mouse Wheel to Switch Weapon (Wrench / Black Pistol) | Left-Click to Attack/Fire | Q for Camera"
$lblFooter.Font = $fontSub
$lblFooter.ForeColor = [System.Drawing.Color]::FromArgb(110, 118, 129)
$lblFooter.Location = New-Object System.Drawing.Point(24, 535)
$lblFooter.Size = New-Object System.Drawing.Size(796, 32)
$lblFooter.TextAlign = [System.Drawing.ContentAlignment]::MiddleCenter
$form.Controls.Add($lblFooter)

[System.Windows.Forms.Application]::Run($form)
