# Official BlueEngine branding

The white rat on the blue tile is the user-selected official engine icon and default
logo for BlueEngine-related projects, documentation, launchers and future branding,
unless the user specifies a different asset for a particular use.

- `blueengine.ico`: original multi-resolution Windows icon, copied byte-for-byte
  from the approved `scripts/test_lab.ico`.
- `blueengine.png`: matching original PNG for documentation and other logo uses.

The legacy copies in scripts/ remain for compatibility. The old make_icon.py script
is historical generation code; it does not update these approved canonical files.
Preserve the artwork and transparency. Do not automatically generate a replacement.

The shortcut script and map launcher use this ICO; README uses the PNG. This change
does not embed a Windows executable resource icon or alter an installed shortcut.
