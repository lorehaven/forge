# forge-tui
Untitled UI design tokens and widgets for forge command-line tools.

## What it is
`forge-tui` gives terminal tools the look of the forge web and mobile apps: the same palette (dark and light, six brand palettes), soft status badges, buttons, cards with rounded borders, alerts and keyboard-hint chips. Colour is truecolor when the terminal says it can, 256 colours otherwise, and plain text with `NO_COLOR`.

## Use cases
- Style a full-screen monitor like the forge web apps.
- Show status as badges and alerts instead of bare coloured words.

## Example
```rust
use forge_tui::{ColorLevel, Theme, Tone, widgets};

let theme = Theme::from_env();
let line = widgets::badge(&theme, Tone::Success, "Up to date");
println!("{}", line.render(ColorLevel::detect()));
```

## Documentation
Full documentation: [libs/forge-tui](../../docs/libs/forge-tui/index.md)
