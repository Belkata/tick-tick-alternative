---
tags: [project/utre, plan, design]
decision: soft-with-material-chips
---

# 09 UI Style Options

Back to [[00 Overview]].

**Mockups:**
- Live page: https://claude.ai/artifact/KTfhcsn5Go3BBzT1w8UQH8 (private, and it works on the phone too).
- Local copy: `plan/ui-styles/styles.html`. Open it in a browser.

Each style shows the same three screens:
1. **Desktop**, three panes: a sidebar with Inbox, Lists, Filters and the nested Tags tree; the task list grouped by priority; and the task detail.
2. **Android home screen** with the widget showing a filter, a tappable checkbox and the **+** quick add.
3. **Week calendar**. Google events are grey and read-only. Timed tasks are coloured by tag.

Each style has a light and a dark version.

| Style | Look | Tags look like | Best for | Trade-off |
|---|---|---|---|---|
| **A · Quiet** | Neutral and airy, close to TickTick and Things | Coloured dot + name | Familiar, low-noise | Least distinctive; tags are subtle |
| **B · Tiling** | Monospace, dense, square corners, Catppuccin colours | `#tag` in colour | Hyprland/terminal setup, max density | Busy on a phone, out of place on Mac |
| **C · Soft** | Rounded task cards, bold headings, teal accent | Filled pastel pills | Tags most prominent, touch-friendly | Fewer tasks per screen |
| **D · Material** | Material 3 / Material You, tonal surfaces | Outlined chips | Native-feeling Android and widget, least design work in Flutter | Looks like Android on the desktop |

**Mixing is fine.** Colours, density, tag treatment and checkbox shape are independent tokens. Some examples:
- "A's layout with C's tag pills"
- "B on desktop and D on Android", since Flutter can theme each platform separately.

> [!success] Decision (2026-11-24)
> **Style C · Soft, with Style D's Material tag chips.**
> - Kept from Soft:
>   - rounded task cards;
>   - bold Manrope headings;
>   - teal accent;
>   - roomy, touch-friendly rows;
>   - rounded widget.
> - Taken from Material: **outlined tag chips**. A small coloured dot sits inside a thin border tinted with the tag colour, replacing Soft's filled pastel pills.
> - The tag colour still drives the sidebar dot, the chip border, the widget dots and the calendar block tint.
> - The same style is used on Android and desktop, in light and dark.
> - It's shown as the first section ("★ Chosen") in the mockups page.
>
> Still to tune during M2 and M4:
> - The accent colour (teal is a placeholder).
> - Desktop density: card spacing could be tighter on desktop than on the phone.
