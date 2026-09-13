---
name: STEALTHNET SOFTWARE
description: Readable operating surfaces for VPN service administration.
colors:
  bg: "#F5F7FA"
  surface: "#FFFFFF"
  surface-3: "#F1F5F9"
  surface-4: "#E2E8F0"
  border: "#E4E9F0"
  border-hi: "#CBD5E1"
  text: "#0F172A"
  text-2: "#44536B"
  text-3: "#5C6980"
  primary: "#12233D"
  primary-hi: "#1D3A63"
  on-primary: "#FFFFFF"
  accent: "#0E7490"
  accent-hi: "#0891B2"
  accent-dim: "#E1F3F8"
  ok: "#0C7A53"
  ok-ink: "#07603F"
  warn: "#9A5308"
  warn-ink: "#7C4206"
  err: "#C42B22"
  err-ink: "#A6231B"
  dark-bg: "#0B1220"
  dark-surface: "#121C2E"
  dark-surface-2: "#16233A"
  dark-surface-3: "#1D2C45"
  dark-surface-4: "#274058"
  dark-text: "#E9EFF8"
  dark-text-2: "#AFC0D6"
  dark-text-3: "#93A5BE"
  dark-primary: "#22D3EE"
  dark-primary-hi: "#67E8F9"
  dark-on-primary: "#062733"
  dark-ok: "#34D399"
  dark-ok-ink: "#6EE7B7"
  dark-warn: "#FBBF24"
  dark-warn-ink: "#FCD34D"
  dark-err: "#F87171"
  dark-err-ink: "#FCA5A5"
typography:
  headline:
    fontFamily: "Inter, -apple-system, SF Pro Text, system-ui, sans-serif"
    fontSize: "26px"
    fontWeight: 700
    lineHeight: 1.2
    letterSpacing: "-.025em"
  console-headline:
    fontFamily: "Inter, -apple-system, SF Pro Text, system-ui, sans-serif"
    fontSize: "32px"
    fontWeight: 700
    lineHeight: 1.15
    letterSpacing: "-.035em"
  title:
    fontFamily: "Inter, -apple-system, SF Pro Text, system-ui, sans-serif"
    fontSize: "16px"
    fontWeight: 600
    lineHeight: 1.45
  body:
    fontFamily: "Inter, -apple-system, SF Pro Text, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: "Inter, -apple-system, SF Pro Text, system-ui, sans-serif"
    fontSize: "11px"
    fontWeight: 400
    lineHeight: 1.5
  caps:
    fontFamily: "Inter, -apple-system, SF Pro Text, system-ui, sans-serif"
    fontSize: "10.5px"
    fontWeight: 600
    lineHeight: 1.2
    letterSpacing: ".11em"
  field-label:
    fontFamily: "Inter, -apple-system, SF Pro Text, system-ui, sans-serif"
    fontSize: "12px"
    lineHeight: 1.5
  form-heading:
    fontFamily: "Inter, -apple-system, SF Pro Text, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 600
    lineHeight: 1.45
    letterSpacing: "0"
  numeric:
    fontFamily: "JetBrains Mono, ui-monospace, SF Mono, Menlo, monospace"
    fontSize: "13px"
    fontWeight: 600
    lineHeight: 1.5
    letterSpacing: "-.01em"
rounded:
  r-sm: "8px"
  r-md: "12px"
  r-lg: "16px"
  control: "9px"
  control-sm: "7px"
  badge: "20px"
spacing:
  s-1: "4px"
  s-2: "8px"
  s-3: "12px"
  s-4: "16px"
  s-5: "20px"
  s-6: "24px"
  s-8: "32px"
  server-gap: "14px"
  panel-gap: "14px"
  panel-mobile: "18px"
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
    rounded: "{rounded.control}"
    padding: "8px 14px"
  button-primary-hover:
    backgroundColor: "{colors.primary-hi}"
    textColor: "{colors.on-primary}"
  button-secondary:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text-2}"
    rounded: "{rounded.control}"
    padding: "8px 14px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.text-2}"
    rounded: "{rounded.control}"
    padding: "8px 14px"
  button-danger:
    textColor: "{colors.err-ink}"
    rounded: "{rounded.control}"
    padding: "8px 14px"
  input-search:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.control}"
    padding: "8.5px 12px 8.5px 36px"
    height: "40px"
  navigation-active:
    backgroundColor: "{colors.accent-dim}"
    textColor: "{colors.accent}"
    rounded: "{rounded.r-sm}"
    padding: "6.5px 10px"
  status-badge:
    rounded: "{rounded.badge}"
    padding: "2.5px 10px"
  server-card:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.r-md}"
    padding: "22px 18px"
  panel-card:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.r-md}"
    padding: "20px"
  panel-card-mobile:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.r-md}"
    padding: "18px"
  dialog:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.r-lg}"
  detail-tab:
    backgroundColor: "transparent"
    textColor: "{colors.text-2}"
    padding: "18px 0"
  traffic-daily:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.r-md}"
---

# Design System: STEALTHNET SOFTWARE

## Overview

**Creative North Star: "Evidence before action"**

This is a code-derived organizing phrase, not a user-approved brand metaphor. The implemented interface uses navy, teal, clear type, compact controls, and visible boundaries to make operational information readable. Status language and measured values carry the meaning; decoration remains secondary.

**Key Characteristics:**

- Light workspace by default, with an explicit persistent dark-theme choice.
- Navy primary regions, teal selection and measurements, distinct status colors.
- Readable server identity, restrained boundaries, and task-specific detail.
- Flat, separated panel cards, readable form sections, and bounded dialogs.
- Measurements, timestamps, missing data, and uncertainty remain distinguishable.

## Colors

The light palette pairs cool neutral surfaces with navy actions and teal operational emphasis. The frontmatter is a compact extraction of actual values; `web/app.css` remains the full runtime source, including theme overrides and color mixes. Unprefixed colors describe the light theme; `dark-*` records selected values assigned to the same CSS properties in dark mode.

### Primary

- **Operating Navy — primary:** the network summary field and solid primary buttons. Its hover companion is `primary-hi`.
- **Measured Teal — accent:** active navigation, selected filters, meters, chart lines, and focus-related treatments. `accent-dim` supplies a pale selected surface.
- Primary buttons use a solid theme primary fill and its hover companion. In dark mode the primary and accent properties share the cyan `dark-primary` value, with dark text on the brighter button. The existing palette remains unchanged.

### Secondary

- **Healthy Green — ok / ok-ink:** positive confirmed status and readable badge text.
- **Attention Amber — warn / warn-ink:** stale reports, unknown engine state, or resource attention.
- **Fault Red — err / err-ink:** unavailable agents, failed engines, and destructive actions. Status fills and borders use translucent mixes; text uses the separate ink values.

### Neutral

- **Cool Workspace — bg:** the application canvas. **White Surface — surface:** light-theme cards and containers; the light input/modal property `--surface-2` has the same value.
- **Inset Surface — surface-3 / surface-4:** table headings, segmented controls, meter tracks, and status strips.
- **Boundary — border / border-hi:** quiet container rules and stronger control edges.
- **Reading Ink — text / text-2 / text-3:** primary identity, supporting content, and tertiary labels. Muted text is still information, not disabled content.
- Dark mode substitutes deep blue surfaces and lighter text. Its borders remain the actual translucent CSS colors rather than new opaque palette entries.

**The Meaning Rule.** Pair status color with words or an icon; color alone must not carry server state.

The sidecar's tonal strips are synthesized OKLCH preview aids derived from the extracted colors. They are not additional shipping palette tokens.

## Typography

**UI Font:** Inter with the platform system fallbacks in the frontmatter. **Numeric / Code Font:** JetBrains Mono with system monospace fallbacks. This is an operational UI pairing already present in the product; there is no separate editorial display face.

### Hierarchy

- **Headline:** shared page heading; the console uses the larger `console-headline` role and reduces it to (28px) on narrow screens.
- **Title:** server identity. Overview summary and server-detail titles use (23px), while supporting section titles generally use (14–17px).
- **Body:** the base document role; compact fields and controls commonly use (12–13px). Console descriptions keep a maximum of (68ch) with a line-height of (1.6). Shared field hints use (12px) with a line-height of (1.6).
- **Label:** supporting measurement and timestamp labels. Field and card labels use the `field-label` size. Form headings use `form-heading` in sentence case without tracking; shared section headings use (15px). The inherited caps role remains in compact table and navigation labels, not form-section titles.
- **Numeric:** monospace measurements with tabular figures. Larger totals keep the UI family with tabular figures, rather than turning every number into code.

## Layout

The inherited desktop shell has a fixed sidebar (236px), top bar (56px), and status bar (32px), with an independently scrolling main area. Main padding is (22px 24px 40px); the console is centered with a maximum width of (1680px). Shared spacing follows the extracted four-pixel scale, with component-specific observed intervals retained where necessary.

Shared cards, tiles, and profile lists use `panel-gap`. Cards take their content height, with `s-5` internal padding on desktop and `panel-mobile` padding on narrow screens. Two-column forms collapse according to their containing card or dialog at (580px), so a narrow dialog remains readable even on a wide display. Toolbars and action groups wrap; card grids admit one full-width column when space is limited.

Infrastructure layout moves from summary to inventory and then to server detail. Its summary uses a (1.5fr / 1fr) split with a minimum secondary width of (280px), stacking at (1150px). Cards use an auto-fit grid with a minimum width of (280px), constrained to the available width. The final user-requested separation is a gap of (14px), a border of (1px), and the `r-md` radius. Inventory card padding is (0 20px 20px).

At (760px), navigation becomes a toggled drawer, main padding becomes (22px 14px 32px), the summary and detail columns stack, and search occupies a full row. The paired receive/send value stays together when its fact row wraps. Inventory tables remain horizontally scrollable with a minimum width of (1000px); daily traffic tables use (760px). Dense data is preserved rather than compressed into unreadable columns. Traffic analysis becomes one column at (1100px).

Dialogs grow with their content within the viewport. Standard drawers cap width at (760px), large drawers at (1040px), and extra-large drawers at (1240px), retaining (48px) total viewport clearance on desktop. Tabbed client workspaces grow with their active content instead of reserving empty viewport height. Viewing a client focuses the heading; editable dialogs focus their first field. Shared modal and drawer bodies scroll independently beneath their headers. On mobile, dialogs become full-width bottom sheets bounded by (94dvh), with wrapping footer actions and safe-area bottom padding. Full editor drawers retain their dedicated viewport-height rule.

## Elevation & Depth

Depth comes from tonal surfaces and thin rules throughout shared panel cards and the infrastructure inventory. Cards and profile rows have no shadow or hover lift; primary buttons use solid fills without shadows. Clickable cards signal hover through the accent boundary and inset surface. Authentication cards retain their ambient shadow, and dialogs retain overlay elevation.

**The Separated Surface Rule.** Give each panel card its own thin boundary and visible sibling gap; keep its position stable on hover.

## Shapes

Use the existing `r-sm`, `r-md`, and `r-lg` family for compact surfaces, shared cards, and dialogs. Controls retain their observed `control` radius; status badges retain their pill shape. Shared cards and server cards have a single (1px) boundary with `r-md` corners. Server cards retain internal rules around resource readings. Mobile dialogs round only their upper corners; the desktop server detail retains its dedicated (1080px) cap.

## Components

### Buttons

Compact, labeled actions with visible keyboard focus. Primary buttons use a solid theme primary fill and foreground; hover uses the primary hover fill with the inherited brightness treatment. Secondary buttons use the field surface and stronger border; ghost buttons use transparent edges until hover; danger buttons use the error tint and ink. Small and icon-only sizing are modifiers, not a new visual family. Enabled buttons depress by (1px) when active. Shared focus uses a (2px) primary outline with (3px) offset; disabled buttons reduce opacity to (.45). Shared mobile buttons have a minimum height of (40px), while dialog footer buttons use (38px).

### Inputs / Fields

Surface-backed, outlined fields use the control radius and explicit labels or accessible names. Focus strengthens the accent border and adds a (3px) accent-dim ring. The server search has an inline SVG icon, inset text, and a (40px) height. Search, country, sorting, and status filters act on the same inventory.

Shared fields have a minimum height of (40px), hints below labels, and sentence-case section headings. An unassociated field label is linked to its first input, select, or textarea by the shared enhancement. Controls with suffixes preserve an editable area; price rows and bulk settings reflow within their container. Disabled fields use the inset surface and tertiary ink. Title-only icon buttons receive accessible names when no visible button text exists.

### Navigation and Chips

The shared sidebar uses text and compact SVG icons; an active item receives an accent-tinted surface, border, and small edge marker. A narrow-screen menu button reveals the sidebar. Status badges combine text and a dot with semantic ink, fill, and border. Inventory filter chips use pressed states; the cards/table switch uses a recessed segmented surface and a raised selected segment.

Shared tab strips scroll horizontally and keep each label on one line. Selected buttons use the accent tint. The enhancement exposes these controls as named groups with pressed states: Left/Right arrows wrap through enabled tabs; Home/End select the first/last; the selected control is scrolled into view. This shared behavior is distinct from the server-detail tablist's panel relationships. Table scroll wrappers accept keyboard focus.

### Panel Cards / Dialogs

Shared cards use the `panel-card` tokens, with the mobile variant at the shell breakpoint. Clickable surfaces use border and background changes rather than movement. Dialogs keep title, contextual subtitle, close control, scrollable content, and footer actions in clear regions. Forms use separated sections instead of ornamental accent strips. The shared dialog opens with an accessible heading relationship and labeled close control; the existing layer mechanism handles stacked dialogs.

### Server Cards / Inventory

A server card keeps identity and actions at the top, state and report age together, paired CPU/memory readings next, then traffic, network, profile, and host facts. The footer opens that server's detail. Long names and addresses wrap. Resource bars accompany numeric readings; missing readings show “Нет замера” or an em dash. The table presents the same server comparison in columns. Card examples in the sidecar use labeled demonstration content and missing measurements, not simulated live readings.

### Server Detail

The detail modal preserves identity, status explanation, and report timestamp above sticky tabs: overview, charts, hosts/access, and diagnostics. The tab bar exposes selection and panel relationships, with left/right arrow navigation. The history tab loads data on demand. Each diagnostic explains its evidence and offers relevant actions or copyable read-only commands. Related host configuration is not proof of a successful client connection.

### Traffic and Freshness

Traffic views pair a timeline with ranking and exact daily tables. An explicit “Точные значения” action opens and focuses the daily table disclosure, including on mobile. Missing days have a separate neutral marker and em dashes; measured zero remains zero. History draws gaps instead of filling absent readings. Report freshness is visible in inventory and detail; the current implementation treats reports older than three minutes as stale. A recent report plus confirmed running engine is required before a server is labeled healthy.

## Do's and Don'ts

### Do:

- **Do** reuse the runtime theme properties so the same component works in both themes.
- **Do** preserve the separated-card gap, single border, and rounded boundary across panel surfaces.
- **Do** keep a network receive/send pair together when the surrounding row wraps.
- **Do** pair charts with available exact values and explicit missing-data states.
- **Do** keep status wording, report age, keyboard focus, and reduced-motion behavior visible in the design.
- **Do** let forms reflow with their container and keep overflowing tables and tabs keyboard reachable.
- **Do** use sentence-case form headings and field labels at the recorded readable sizes.

### Don't:

- **Don't** replace missing measurements with zero or call unconfirmed engine state healthy.
- **Don't** treat configured hosts or an agent report as proof of end-to-end VPN access.
- **Don't** add hover lift, card shadows, or ornamental accent strips to shared panel cards.
- **Don't** present shared visual coverage or fixture review as evidence that every backend workflow is complete.
