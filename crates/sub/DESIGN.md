---
name: Customer connection page
description: Owner-branded guidance for installing an app and importing a VPN subscription.
colors:
  bg: "#f7fafe"
  surface: "#fff"
  ink: "#080b18"
  muted: "#6b6f83"
  line: "#dfe2ef"
  inset: "#f3f4fa"
  accent: "var(--a1,var(--ink))"
  accent-end: "var(--a2,var(--accent))"
  on-primary: "var(--surface)"
  good: "#28754f"
  good-bg: "#edf5ee"
  warning: "#92570e"
  warning-bg: "#faf1e3"
  dark-bg: "#101321"
  dark-surface: "#1b2033"
  dark-ink: "#f2f3fb"
  dark-muted: "#b1b8ce"
  dark-line: "#353b51"
  dark-inset: "#24293f"
  dark-good: "#a1dbb7"
  dark-good-bg: "#253b35"
  dark-warning: "#f4c878"
  dark-warning-bg: "#413521"
typography:
  body:
    fontFamily: "CustomerRoboto, Arial, sans-serif"
    fontSize: "15px"
    lineHeight: 1.5
  headline:
    fontFamily: "CustomerRoboto, Arial, sans-serif"
    fontSize: "44px"
    fontWeight: 800
    lineHeight: 1.08
    letterSpacing: "-.035em"
  mobile-headline:
    fontFamily: "CustomerRoboto, Arial, sans-serif"
    fontSize: "30px"
    fontWeight: 800
    lineHeight: 1.1
    letterSpacing: "-.035em"
  button:
    fontFamily: "CustomerRoboto, Arial, sans-serif"
    fontSize: "15px"
    fontWeight: 650
  button-primary:
    fontFamily: "CustomerRoboto, Arial, sans-serif"
    fontSize: "17px"
    fontWeight: 650
  link-field:
    fontFamily: "ui-monospace, monospace"
    fontSize: "13px"
    lineHeight: 1.5
rounded:
  card: "24px"
  mobile-card: "20px"
  app-picker: "18px"
  button: "17px"
  field: "14px"
  theme-option: "16px"
  status: "20px"
  shortcuts: "30px"
spacing:
  desktop-shell: "32px"
  desktop-grid: "20px"
  card-padding: "32px"
  mobile-shell: "18px 14px"
  mobile-grid: "13px"
  mobile-card-padding: "20px"
  dialog-body: "22px"
components:
  button-primary:
    textColor: "{colors.on-primary}"
    rounded: "{rounded.button}"
    padding: "13px 16px"
  button-secondary:
    textColor: "{colors.accent}"
    rounded: "{rounded.button}"
    padding: "13px 16px"
  card:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.card}"
    padding: "{spacing.card-padding}"
  link-field:
    backgroundColor: "{colors.inset}"
    textColor: "{colors.ink}"
    rounded: "{rounded.field}"
    padding: "12px"
  dialog:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.card}"
---

# Design System: Customer connection page

## Overview

**Creative North Star: "Понятный следующий шаг"**

This app-root record extends the approved [customer cabinet system](../../web/cabinet/DESIGN.md) to `sn-sub`. It records the corrected implementation in [page.rs](src/page.rs), [connect.css](src/connect.css) and [connect.js](src/connect.js). The customer system supplies the common identity: bundled Roboto, softly raised rounded cards, light/dark surfaces, configured logos and gradient actions. The connection page has its own compact task composition. The [administration design](../../DESIGN.md) remains separate.

**Key Characteristics:**

- One owner identity across the customer site, installed Mini App and connection page.
- Cool light ground or layered dark surfaces, rounded cards and soft directional depth.
- Bundled variable Roboto, strong task headings and outlined SVG controls.
- Primary gradients with contrast-adjusted endpoints; configured accent hues remain available to other details.
- Immediate theme selection and clear copy feedback, with normal document flow for longer content.

## Colors

The frontmatter extracts the connection page's current source values. `connect.css` remains the runtime authority. Both account and connection cards use the surface token; the account summary no longer has an independent navy palette.

### Primary

The owner supplies six-digit hexadecimal accent endpoints. `--a1` and `--a2` preserve those validated values. `--accent-soft` mixes the first accent at 7% into the surface. Secondary buttons, selection, focus, shortcuts and traffic use the owner accent family. Dark secondary control ink mixes 38% accent with white.

Primary controls use the separate `--primary-grad`, a 108-degree gradient between `--primary-start` and `--primary-end`. The renderer scales each configured RGB endpoint toward black only as needed to reach a 4.6:1 contrast target against its white label, then emits `--on-primary:#fff`. One valid endpoint supplies both ends when the other is absent. Without either valid endpoint, both primary stops fall back to `--ink` and the label to `--surface`, producing a legible neutral pair in either theme. The decorative `--grad` remains distinct and retains the configured hue progression; its exact syntax is in the sidecar.

**The Readable Primary Rule.** Preserve owner hues while deriving readable primary endpoints; never assume syntactically valid accent colors are sufficient for white button text.

### Secondary

Green supports labeled active-access and successful-copy states. Amber supports labeled attention states. These colors do not assert payment completion or an established VPN session. QR content keeps its white backing in either theme.

### Neutral

Ground, surface and inset separate page, working cards and nested controls. Ink and muted establish two text levels; line marks dividers. Auto follows the device color scheme. Light and Dark override it immediately; `sn.app.theme` is stored in localStorage, so the preference persists per origin. The page uses the same preference key as the other customer surfaces, but separate domains do not share localStorage. Initial head code applies the preference before the page styles render. Configured dark logos replace light logos when a pair exists; absent primary logo falls back to the brand name.

## Typography

The UI uses variable Roboto registered as `CustomerRoboto`, weights 100–900, with `font-display:swap`. The page preloads `/fonts/roboto.ttf`; `sn-sub` serves the bundled customer font from its own origin. It does not require a remote font provider. The personal subscription link field uses system monospace, and subscription quantities use tabular figures.

The task headline is 44px on the regular desktop layout, 30px on mobile, 27px in the short-phone rule and 25px below 361px width. Short desktop uses 34px. Body remains 15px; supporting instructions, labels and facts step down with available space. Primary action text is 17px desktop, 16px mobile and 15px on short phones. App names use 19px, then 17px or 16px in compact layouts. Brand text and selected-app names keep bounded measures; full customer identity and long facts remain available in details.

## Layout

The centered shell caps at 1280px, has `min-height:100svh`, uses 32px desktop padding and retains normal document flow. A two-column grid gives equal widths to the summary and connection regions; the connection card spans the summary and shortcuts rows. Header and footer sit outside the grid. The connection task keeps installation, import/copy and the final instruction together.

At 760px and below, the shell caps at 640px, the grid stacks summary, connection and shortcuts, with 14px horizontal page padding and 20px card padding. At heights up to 740px, it hides the introductory sentence, places the app picker and device select in one row and tightens spacing while retaining the final instruction. At widths up to 360px, horizontal page padding becomes 10px. At heights up to 620px, it compacts further. Short desktop has its own spacing/type rule at widths from 761px and heights up to 700px. Safe-area-aware top/bottom padding is present in the short-phone rule.

Dialogs remain centered rounded surfaces on phones and desktop, up to 600px wide with 24px total horizontal clearance, and `max-height:calc(100dvh - 40px)`. The header/close control stays outside the independently scrolling body. These are no longer full-width bottom sheets. Long lists, names and guidance wrap or scroll inside the dialog; they do not force horizontal overflow.

The final compact fixture at 320×568 has a 571px document: the main action remains visible, with three extra document pixels. Other supplied ordinary first views match their viewport heights. This is a bounded observation, not a promise that every screen, long configuration or enlarged-text state fits without scrolling.

## Elevation & Depth

Cards and shortcuts use `--shadow`: soft directional depth, lighter in Light and stronger in Dark. Primary controls add an accent-colored shadow; copied controls remove it. Dialogs use a separate overlay shadow and dark scrim. These materials extend the customer world without changing the panel's flat card rules. Exact shadows belong to the sidecar.

The connection grid has a visible blur-to-clear arrival lasting 250ms. Buttons transition background and filter over 180ms; hover uses a subtle brightness change. Reduced motion disables animations, transitions and smooth scrolling. Application changes and dialogs have no separate entrance animation in the corrected CSS.

## Shapes

Desktop cards and dialogs use 24px corners; mobile cards use 20px. App pickers use 18px, action controls 17px and fields 14px. Shortcut containers are broadly rounded at 30px desktop and 28px mobile; their items use 22px. Status badges and numbered steps use pill/circle forms. Theme, help and close controls retain clear icon shapes and outlined SVG. These are the approved customer materials, scoped to this app.

## Components

### Shared identity

The public identity projection contains only the brand name, light logo, dark logo, favicon and two accent endpoints. The API-backed and database-backed subscription modes both derive it from cabinet configuration through `CustomerBrand`. Identity image URLs accept HTTPS with a host and no embedded credentials; accent values accept six-digit hex. Cabinet private settings and service keys are not part of this projection. Logo/favicon URLs remain owner-configured resources; no new raster art ships with this change. Built-in UI/app glyphs and server-generated QR use SVG.

The subscription administration shortcut opens “Кабинет и Mini App” branding settings and explains the shared source. Subscription applications, guides and the VPN app's profile title remain separate settings. This shortcut retains administration styling.

### Actions, selection and feedback

Primary, tinted secondary and text actions form one rounded family. Focus-visible uses a 2px accent outline with 4px offset. The device picker is a labeled native select. Application choices use `aria-pressed`; the selected app updates the picker, actual installation/import actions and Help content. User-Agent detection provides an initial platform guess, and the device selector permits correction. App choices are remembered per platform only during the current page session.

Successful copying changes the initiating button to a checked confirmation with “Ссылка скопирована”. The open QR dialog adds an inline `role=status`; copying from the main view adds a polite 3.5-second message. The fallback opens the link dialog and selects the text, reporting copy success only if the operation succeeded; otherwise it explains manual copying. The checked button state remains for the current page session. Theme uses three labeled native radio options with immediate selection feedback.

### Connection guidance and detail dialogs

Help follows the actual selected application's capabilities. A configured store URL produces the named installation action; no store URL gives official-source/support guidance. A deeplink produces “Добавить подписку” with a manual-link fallback. An app without a deeplink and the no-app state both explain the visible “Скопировать ссылку” action, importing from clipboard in the VPN app and enabling VPN. Inactive or not-ready Help uses the actual access explanation instead of unavailable steps.

Native named dialogs contain full subscription facts, the personal link/QR, configured locations, app choices, theme, Help and optional service notice. Their close buttons are labeled and autofocus; app selection returns focus to the picker. Link privacy copy explains that the link/QR grants subscription access. Location details present real addresses/protocols and direct latency measurement to the VPN app. The unlimited-device summary uses an accessible infinity symbol and keeps the full wording in subscription details.

**The Available Action Rule.** Guidance must name controls that exist in the current state, and a button click must never become a claim that the external application imported a profile or established VPN connectivity.

## Do's and Don'ts

- **Do** extend the customer cabinet's configured identity, bundled font and rounded light/dark materials.
- **Do** derive readable primary colors while retaining the original owner accents for their other roles.
- **Do** keep Help synchronized with the selected application's installation and import capabilities.
- **Do** preserve visible copy feedback, explicit theme selection and normal scrolling for long content.
- **Do** describe subscription state, imported configuration and established VPN connectivity as separate facts.
- **Don't** restore the obsolete system-font navy/teal connection palette or the automatic-only theme behavior.
- **Don't** expose private cabinet configuration through the public identity projection.
- **Don't** add purchases or customer-cabinet navigation to this connection task.
- **Don't** import customer card/gradient rules into the administration design.
- **Don't** describe the resolved visual correction list as production deployment, real VPN-app verification or a whole-product audit.
