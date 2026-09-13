---
name: Customer cabinet and storefront
description: Owner-branded guided access across the customer site and installed Mini App.
colors:
  ground: "#f7fafe"
  surface: "#fff"
  ink: "#080b18"
  muted: "#6b6f83"
  accent-start: "var(--a1)"
  accent-end: "var(--a2)"
  on-accent: "#fff"
  active: "#48a568"
  error: "#bd3830"
  dark-ground: "#101321"
  dark-surface: "#1b2033"
  dark-ink: "#f2f3fb"
  dark-muted: "#b1b8ce"
typography:
  body:
    fontFamily: "CustomerRoboto, Arial, sans-serif"
    fontSize: "15px"
    lineHeight: 1.55
  mobile-headline:
    fontFamily: "CustomerRoboto, Arial, sans-serif"
    fontSize: "28.3px"
    fontWeight: 800
    lineHeight: 1.09
    letterSpacing: "-.035em"
  desktop-headline:
    fontFamily: "CustomerRoboto, Arial, sans-serif"
    fontSize: "40px"
    lineHeight: 1.12
    letterSpacing: "-.035em"
  storefront-headline:
    fontFamily: "CustomerRoboto, Arial, sans-serif"
    fontSize: "48px"
    lineHeight: 1.06
    letterSpacing: "-.04em"
  button:
    fontFamily: "CustomerRoboto, Arial, sans-serif"
    fontSize: "15px"
    fontWeight: 650
    lineHeight: 1.45
  code:
    fontFamily: "ui-monospace, monospace"
    fontSize: "17px"
    letterSpacing: ".025em"
rounded:
  card: "24px"
  compact-card: "18px"
  secondary-card: "20px"
  control: "16px"
  code-input: "14px"
  mobile-action: "17px"
  pill-action: "22px"
  mobile-navigation: "45px"
spacing:
  mobile-page: "14px"
  mobile-grid: "13px"
  resource-gap: "12px"
  desktop-grid: "20px"
  card-padding: "24px"
  dialog-padding: "22px"
components:
  button-primary:
    textColor: "{colors.on-accent}"
    rounded: "{rounded.control}"
    padding: "12px 18px"
  button-line:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.accent-start}"
    rounded: "{rounded.control}"
    padding: "12px 18px"
  code-input:
    backgroundColor: "{colors.ground}"
    textColor: "{colors.ink}"
    rounded: "{rounded.code-input}"
    padding: "14px 16px"
  dialog:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.card}"
  plan-card:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.card}"
    padding: "26px"
---

# Design System: Customer cabinet and storefront

## Overview

**Creative North Star: "Понятный следующий шаг"**

The user selected the guided concept and comp build path. This customer world uses clear next actions, rounded white surfaces, quiet cool ground, dark sans headings, soft depth and owner-colored gradient controls. The mobile cabinet, desktop cabinet, storefront and installed Telegram Mini App share visual materials and component behavior. Real account/catalog state determines content and available actions.

**Key Characteristics:**

- One dominant next action with state-aware instructions and supporting account controls.
- Near-white cool ground, white curved surfaces, owner-colored gradient actions and soft depth.
- Bundled variable Roboto, strong headings and darkened supporting text.
- Semantic outlined SVG, connected steps, paired resources and icon-and-label navigation.
- A distinct desktop composition and storefront adaptation sharing customer controls.

## Colors

The frontmatter records current shared customer values. Runtime accent endpoints remain CSS variables supplied by validated owner configuration; no fixed sample pair is prescribed to every owner. The chosen comp uses violet/cyan character. Primary backgrounds mix the first endpoint with a lightened second endpoint before reaching the latter; exact gradient syntax belongs to the sidecar. Low-strength accent mixes tint the ground, rail, circles, insets and secondary controls.

### Primary

Owner accent start/end mark task actions, active navigation, progress and selected controls. White is the primary action foreground. Secondary controls use accent text on tinted or bordered surfaces; dark mode supplies pale violet ink for those variants.

### Secondary

Active green belongs to a labeled access dot and related status fill; it does not report a tested VPN session or payment. The inherited error color accompanies validation text and recovery actions.

### Neutral

Ground/surface distinguish the near-white field from working cards. Ink/muted distinguish primary and supporting information. The current muted token replaces the lighter source value flagged by the reviewer; the token change is not a substitute for final rendered contrast verification. Dark mode uses the documented dark ground, surface and two text levels.

Theme offers Auto, Light and Dark. Auto follows Telegram when applicable or the system; explicit preference is stored locally. Owner light/dark logos switch with the theme. QR/code or image content does not supply a new page palette.

**The Configured Identity Rule.** Render the owner's brand, valid logos, accent endpoints and configured content; reference-comp text and prices do not become customer facts.

## Typography

**UI font:** bundled variable Roboto, registered as `CustomerRoboto` from `/fonts/roboto.ttf`, with weights (100–900) and `font-display: swap`. It overrides the shared base's platform stack. **Access-code font:** system monospace, separated from ordinary UI text.

The type ramp distinguishes heavy mobile task headings, wide cabinet headings and the storefront's largest introduction. Resource totals use tabular figures and heavier weights. The authenticated mobile body tightens to (1.45) line-height; task descriptions use (14px) on mobile and (21px) at wide desktop. Intermediate desktop reduces the task heading to (32px). The storefront introduction becomes (38px) at its intermediate breakpoint and (39px) on narrow phones. Labels, action text and support descriptions retain the same family. Exact comp wraps remain subject to the finish verdict; one-off corrective measurements are not a new global type scale.

## Layout

The authenticated phone shell caps at (640px), with safe-area-aware bottom clearance for its fixed floating navigation. Task and side regions use the mobile grid gap; devices and traffic remain paired. Explicit resource grid rows align their actions while allowing actual quantities, unlimited allowances and addon eligibility to differ. Access and help follow. Content remains scrollable instead of being clipped to manufacture a comp-sized result.

At (980px), the standalone cabinet introduces a fixed rail, task/resource column, subscription/access column and a full-width support row. The desktop shell caps at (1600px); its rail is (232px), growing to (248px) in the widest rule. Between (980px) and (1250px), it narrows the rail and contextual column and stacks task actions. Desktop shows a contextual heading while the brand moves to the rail. Mini App shares the mobile presentation.

The public storefront caps at (1280px), with paired opening content, staggered steps, auto-fit offers, a paired FAQ heading/content and wrapping configured footer. At (640px), those regions stack. Wide step connectors disappear on narrower screens without removing the step text. Empty catalogs and missing optional configuration do not generate fabricated content.

Dialogs cap at (600px) wide and `calc(100dvh - 40px)` high, retaining (24px) total horizontal clearance. Shared base behavior keeps a stable heading/close area and an independently scrolling body. Code saving, login, checkout and account controls use that same dialog family.

## Elevation & Depth

Soft directional shadows are explicitly pinned materials. Cards, active navigation and primary buttons use distinct levels of depth, while dialogs add overlay elevation. The sidecar retains the exact source values. No hard offset shadow or decorative raster layer is part of this system.

The dashboard uses a short (250ms) blur-to-clear arrival; its reduced-motion rule suppresses that animation. Base control background/border transitions and FAQ-chevron rotation are short. This does not claim a universal reduced-motion override beyond the rules in source.

## Shapes

Large cards/dialogs use soft corners; mobile task/resource/access cards use the compact radius. Primary, bordered and tinted controls are rounded rectangles; resource actions are pill-like. Dots, step discs, icon fields and the selected mobile navigation glyph are circular. The floating phone bar has its own large radius. These pinned materials do not change the panel's geometry.

**The Semantic Drawing Rule.** Keep icons, checks, arrows, progress, steps and controls as semantic SVG/CSS and actual interface elements. Reference images guide their geometry; they are not replacement interface layers.

## Components

### Buttons and fields

Primary actions use the owner gradient, white text and soft lift. Outlined/tinted alternatives retain the same family. SVG arrows/checks keep their approved drawing role alongside real labels and handlers. Focus uses a (2px) accent outline with (4px) offset; disabled buttons reduce opacity. Code fields are labeled full-width monospace inputs. Errors use text and status regions.

### Guided task and resources

Real account state determines the headline, explanation and destination. Active access offers connection; other states expose tariff, support or traffic recovery. Three steps distinguish completed and current stages; **Доступ** avoids claiming a manual subscription was paid. The resource pair maintains action alignment without inventing missing values or eligible addons.

**The State Before Action Rule.** Use real account/catalog state to choose the action and its explanation; active access, payment completion and established VPN connectivity are different facts.

### Access, support and navigation

The masked access row opens management rather than revealing a stored secret. Registration/issuance provides a one-time display, copy/download and saved-code acknowledgement; rotation explains replacement/session effects. Telegram linking requests explicit bot confirmation. Desktop separates access management and support instructions into distinct controls with their existing handlers.

Mobile navigation pairs icons and labels in a floating bar; the active glyph receives a gradient circle. Shop/payment and referral visibility follow configuration. Desktop adds a distinct home SVG, labeled rail, account and logout. Status includes words and a dot. Theme remains accessible through account settings where its separate mobile header control is hidden.

### Storefront offers and FAQ

Configured headline, description and offers lead to selection or code login. Steps combine discs, text, completion glyphs and wide-screen dashed connectors. Plans begin with a semantic device/family glyph and show real catalog details. FAQ uses native disclosure semantics and a right-hand SVG chevron. Footer links appear only when configured.

## Do's and Don'ts

- **Do** preserve the approved materials and exact-comp fidelity requirement while using real account/catalog data.
- **Do** keep owner branding and accent endpoints configurable across the site and installed Mini App.
- **Do** use the bundled UI font and explicit hierarchy across devices.
- **Do** retain semantic controls, aligned resource actions and reachable detail states.
- **Don't** turn illustrative dates, prices, paid states or support promises into runtime facts.
- **Don't** import this customer palette and rounded-gradient world into the administration panel.
- **Don't** canonize known fidelity, typography, contrast or first-viewport defects as reusable rules.
- **Don't** describe this source record as final review approval, deployment or a new security audit.

Not canonized: the finish review's previous low-contrast muted text, approximate glyphs and misaligned regions are defects to score through the ordered correction pass. The old seed has no supplied roll evidence and remains unverified provenance; no generated record substitutes for it. The parent reports a replacement full-size hero capture, but this documenter does not score that checkpoint.

## Обратная связь и ширина · 13.09.2026
Копирование кода подтверждается галочкой, текстом кнопки и inline status в том же окне. Повторный просмотр и скрытие доступны в управлении кодом. Desktop cabinet занимает полную ширину viewport с боковой навигацией у левого края; публичная витрина сохраняет свою меру текста. Административная sticky-панель сохранения использует непрозрачный `--surface`.
