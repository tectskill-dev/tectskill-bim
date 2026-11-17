# Parametric Steel Building Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `SteelBuilding` host element (portal frames, purlins and girts of a steel shed) whose members are ordinary columns and beams, regenerated in the same transaction whenever its parameters change, with an engine command, plan drawing, a Structure tab dialog and palette rows.

**Architecture:**
- `cc_core::steel_building` holds the model (`SteelBuilding`, `MemberRole`, `FrameRoof`, bounds and defaults), a deterministic `layout`, the sync run by `EditCtx::finish` (detach members the user edited, rewrite the others in place, keep ids) and the repair.
- `cc_engine` gets `steel_building_create` (catalog default types through a shared `catalog_type` helper in `structure_material.rs`), `element_get` derived member lists and `element_set` checks.
- `cc_views` draws the host's footprint dashed in plans; IFC export leaves the host out (its members export as usual).
- `cc_app` adds a *Steel Building* button and dialog (`steel_building_ui.rs`) and the palette rows with *Reset Members*.

**Tech Stack:** Rust 2024 workspace. egui/eframe app with egui_kittest UI tests. Fluent i18n in 12 languages. cargo-nextest.

**Spec:** `docs/superpowers/specs/2026-11-12-steel-building-design.md`

## Global Constraints

- Work only in the worktree `D:\constructcad-steel` (branch `feat/steel-building`, built on `feat/structure-tab-materials`). Never touch `D:\constructcad`.
- The spec is at `docs/superpowers/specs/2026-11-12-steel-building-design.md` in the worktree (copied there before Task 1). Read it with this plan.
- Never push, never run `gh`, never merge. Commit with `git commit -s`. No Claude attribution or Co-Authored-By.
- Non-test code never panics: no `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!` or `unimplemented!` (clippy denies them). No `unsafe`.
- Every new interface or transaction text is an i18n key added to all 12 files in `assets/i18n/`: `pt-BR`, `en`, `es`, `fr`, `de`, `it`, `lo`, `pl`, `uk-UA`, `zh-TW`, `zh-CN`, `ja`. Tests fail otherwise.
- Every engine command parameter has a doc comment. Register the command in `crates/cc_engine/src/registry.rs` (`registry!`). Regenerate the reference with `cargo run -p cc_cli -- reference > docs/commands.md`; a test checks it.
- Integration tests are one binary: a new file in `crates/cc_engine/tests/` needs `mod <name>;` in `crates/cc_engine/tests/main.rs`.
- Tests run with cargo-nextest: `cargo nextest run -p <crate> <filter>`. Use `cargo check --tests -p <crate>` (or `--workspace`) for type errors.
- Formatting is rustfmt with `max_width = 120`: `cargo fmt --all`. Clippy: `cargo clippy --workspace --all-targets -- -D warnings`.
- Code, comments and identifiers are in English.
- Values (verbatim from the spec):
  - `span` 2..=100 m, default 20.
  - `eave_height` 1..=50 m, default 6.
  - `slope_percent` 0..=100, default 10.
  - `bays` 1..=100, default 5 (frames = bays + 1).
  - `bay_spacing` 1..=30 m, default 6.
  - Purlin and girt `spacing` 0.3..=10 m, default 1.5; both on by default.
  - `MAX_MEMBERS = 4000`.
  - A missing section reads as 0.2 m.
  - Girt `index` = `side * 1000 + row`.
  - Default types: columns and rafters catalog `W` row `W310x38,7`; purlins and girts catalog `NBR6355-Ue` row `Ue 150x60x20x2,00`.
  - Out-of-range values are refused by commands, not clamped; repair resets them to the defaults.
- Decisions this plan makes where the spec is open (see the end for the list): the roof shape enum is `FrameRoof` (not `RoofShape`, which already exists in `cc_core`); columns rise to the rafters' top at the column axis (`eave_height + rafter depth`) so the existing beam joins stop the rafters at the column faces; only changes to an element's kind, type or level detach a member (marks, comments, phases do not); a deleted member is always detached; edits of a type the building uses regenerate it; the sync runs right after `sync_profiles` (which touches only types) and before every sync that touches elements; `ElementKind::SteelBuilding` holds a `Box`.

## i18n keys

Each task adds the keys it uses to all 12 files, appended under a comment `# Steel building`. Translations:

| key | pt-BR | en | es | fr | de | it | lo | pl | uk-UA | zh-TW | zh-CN | ja |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| category-steel-building | Galpões metálicos | Steel buildings | Naves metálicas | Bâtiments métalliques | Stahlhallen | Capannoni in acciaio | ອາຄານໂຄງເຫຼັກ | Hale stalowe | Сталеві будівлі | 鋼構建築 | 钢结构建筑 | 鉄骨建屋 |
| tpl-type-steel-building | Galpão metálico | Steel building | Nave metálica | Bâtiment métallique | Stahlhalle | Capannone in acciaio | ອາຄານໂຄງເຫຼັກ | Hala stalowa | Сталева будівля | 鋼構建築 | 钢结构建筑 | 鉄骨建屋 |
| tx-steel-building | Criar galpão metálico | Create steel building | Crear nave metálica | Créer un bâtiment métallique | Stahlhalle erstellen | Crea capannone in acciaio | ສ້າງອາຄານໂຄງເຫຼັກ | Utwórz halę stalową | Створити сталеву будівлю | 建立鋼構建築 | 创建钢结构建筑 | 鉄骨建屋を作成 |
| action-steel-building | Galpão metálico | Steel Building | Nave metálica | Bâtiment métallique | Stahlhalle | Capannone in acciaio | ອາຄານໂຄງເຫຼັກ | Hala stalowa | Сталева будівля | 鋼構建築 | 钢结构建筑 | 鉄骨建屋 |
| dlg-steel-building | Galpão metálico | Steel Building | Nave metálica | Bâtiment métallique | Stahlhalle | Capannone in acciaio | ອາຄານໂຄງເຫຼັກ | Hala stalowa | Сталева будівля | 鋼構建築 | 钢结构建筑 | 鉄骨建屋 |
| section-steel-building | Galpão metálico | Steel Building | Nave metálica | Bâtiment métallique | Stahlhalle | Capannone in acciaio | ອາຄານໂຄງເຫຼັກ | Hala stalowa | Сталева будівля | 鋼構建築 | 钢结构建筑 | 鉄骨建屋 |
| section-secondary-members | Terças e longarinas | Purlins and Girts | Correas de cubierta y fachada | Pannes et lisses | Pfetten und Wandriegel | Arcarecci e correnti di parete | ແປຫຼັງຄາ ແລະ ແປຝາ | Płatwie i rygle ścienne | Прогони та ригелі фахверку | 檁條與牆檁 | 檩条与墙檩 | 母屋と胴縁 |
| prop-span | Vão | Span | Luz | Portée | Spannweite | Luce | ຊ່ວງກວ້າງ | Rozpiętość | Проліт | 跨距 | 跨度 | スパン |
| prop-eave-height | Altura do beiral | Eave height | Altura de alero | Hauteur à l'égout | Traufhöhe | Altezza di gronda | ຄວາມສູງຊາຍຄາ | Wysokość okapu | Висота карниза | 簷高 | 檐口高度 | 軒高 |
| prop-bays | Módulos | Bays | Vanos | Travées | Felder | Campate | ຈຳນວນຊ່ອງ | Przęsła | Кількість кроків | 開間數 | 开间数 | ベイ数 |
| prop-bay-spacing | Distância entre pórticos | Bay spacing | Separación entre pórticos | Entraxe des portiques | Binderabstand | Interasse dei portali | ໄລຍະຫ່າງລະຫວ່າງໂຄງ | Rozstaw ram | Крок рам | 開間間距 | 开间间距 | ベイ間隔 |
| label-rafter-type | Tipo de viga de cobertura | Rafter type | Tipo de dintel | Type de traverse | Riegeltyp | Tipo di traverso | ປະເພດຄານຫຼັງຄາ | Typ rygla | Тип ригеля | 屋頂大梁類型 | 屋面梁类型 | ラフタータイプ |
| prop-purlins | Terças | Purlins | Correas | Pannes | Pfetten | Arcarecci | ແປຫຼັງຄາ | Płatwie | Прогони | 檁條 | 檩条 | 母屋 |
| prop-girts | Longarinas | Girts | Correas de fachada | Lisses | Wandriegel | Correnti di parete | ແປຝາ | Rygle ścienne | Ригелі фахверку | 牆檁 | 墙檩 | 胴縁 |
| prop-members | Elementos | Members | Barras | Barres | Stäbe | Aste | ອົງປະກອບ | Pręty | Елементи | 構件 | 构件 | 部材 |
| prop-detached-members | Elementos desvinculados | Detached members | Barras desvinculadas | Barres détachées | Gelöste Stäbe | Aste scollegate | ອົງປະກອບທີ່ແຍກອອກ | Odłączone pręty | Від'єднані елементи | 已分離構件 | 已分离构件 | 切り離した部材 |
| action-reset-members | Restaurar elementos | Reset Members | Restablecer barras | Rétablir les barres | Stäbe zurücksetzen | Ripristina aste | ຄືນຄ່າອົງປະກອບ | Przywróć pręty | Відновити елементи | 重設構件 | 重置构件 | 部材をリセット |
| opt-catalog-default | Padrão do catálogo | Catalog default | Predeterminado del catálogo | Par défaut du catalogue | Katalogstandard | Predefinito del catalogo | ຄ່າເລີ່ມຕົ້ນຈາກລາຍການ | Domyślny z katalogu | Типовий з каталогу | 型錄預設 | 目录默认 | カタログ既定 |

Two longer keys:

`cmd-desc-steel-building`:
- pt-BR: Cria um galpão metálico paramétrico: pórticos de pilares e vigas de cobertura, com terças e longarinas, que acompanham seu vão, altura, inclinação e módulos.
- en: Create a parametric steel building: portal frames of columns and rafters, with purlins and girts, kept in step with its span, height, slope and bays.
- es: Crea una nave metálica paramétrica: pórticos de pilares y dinteles, con correas de cubierta y de fachada, que siguen su luz, altura, pendiente y vanos.
- fr: Crée un bâtiment métallique paramétrique : portiques de poteaux et de traverses, avec pannes et lisses, qui suivent sa portée, sa hauteur, sa pente et ses travées.
- de: Erstellt eine parametrische Stahlhalle: Rahmen aus Stützen und Riegeln mit Pfetten und Wandriegeln, die Spannweite, Höhe, Neigung und Feldern folgen.
- it: Crea un capannone in acciaio parametrico: portali di pilastri e traversi, con arcarecci e correnti di parete, che seguono luce, altezza, pendenza e campate.
- lo: ສ້າງອາຄານໂຄງເຫຼັກແບບພາຣາມິເຕີ: ໂຄງປະກອບດ້ວຍເສົາ ແລະ ຄານຫຼັງຄາ, ພ້ອມແປຫຼັງຄາ ແລະ ແປຝາ, ທີ່ປ່ຽນຕາມຊ່ວງກວ້າງ, ຄວາມສູງ, ຄວາມຊັນ ແລະ ຈຳນວນຊ່ອງ.
- pl: Tworzy parametryczną halę stalową: ramy ze słupów i rygli, z płatwiami i ryglami ściennymi, dopasowane do rozpiętości, wysokości, spadku i przęseł.
- uk-UA: Створює параметричну сталеву будівлю: рами з колон і ригелів, з прогонами та ригелями фахверку, що відповідають прольоту, висоті, ухилу та кроку рам.
- zh-TW: 建立參數化鋼構建築：由柱與屋頂大梁組成的剛架，附檁條與牆檁，隨跨距、高度、坡度與開間更新。
- zh-CN: 创建参数化钢结构建筑：由柱和屋面梁组成的刚架，附檩条和墙檩，随跨度、高度、坡度和开间更新。
- ja: パラメトリックな鉄骨建屋を作成します。柱とラフターからなる門型フレームに母屋と胴縁を加え、スパン、高さ、勾配、ベイに合わせて更新します。

`msg-steel-building-created` (placeholder `$count`, identical in every language):
- pt-BR: Galpão metálico criado com { $count } elementos.
- en: Steel building created with { $count } members.
- es: Nave metálica creada con { $count } barras.
- fr: Bâtiment métallique créé avec { $count } barres.
- de: Stahlhalle mit { $count } Stäben erstellt.
- it: Capannone in acciaio creato con { $count } aste.
- lo: ສ້າງອາຄານໂຄງເຫຼັກແລ້ວ ມີ { $count } ອົງປະກອບ.
- pl: Utworzono halę stalową, prętów: { $count }.
- uk-UA: Створено сталеву будівлю, елементів: { $count }.
- zh-TW: 已建立鋼構建築，共 { $count } 個構件。
- zh-CN: 已创建钢结构建筑，共 { $count } 个构件。
- ja: 部材 { $count } 本の鉄骨建屋を作成しました。

Existing keys reused (do not add): `prop-rotation`, `prop-roof-shape`, `roof-gable`, `roof-shed` (the mono-pitch choice), `prop-slope` (shown with a `%` suffix), `fp-origin-x`, `fp-origin-y`, `prop-type`, `section-type`, `label-column-type`, `label-spacing`, `action-ok`, `action-cancel`.

---

### Task 1: The model and its layout (cc_core)

**Files:**
- Create: `crates/cc_core/src/steel_building.rs`
- Create: `assets/icons/ui/steel-building.svg`
- Modify:
  - `crates/cc_core/src/lib.rs`: `pub mod steel_building;` in alphabetical position (after `pub mod shaft;`).
  - `crates/cc_core/src/element.rs`: variant `SteelBuilding(Box<crate::steel_building::SteelBuilding>)` after `Shaft(..)`; `Element::category` arm; `Element::translate` arm `ElementKind::SteelBuilding(b) => b.origin += delta,`.
  - `crates/cc_core/src/transform.rs`: `Element::transform` arm `ElementKind::SteelBuilding(b) => b.transform(t),`.
  - `crates/cc_core/src/types.rs`:
    - `Category::SteelBuilding` (doc comment `/// Parametric steel buildings: hosts of portal frames, purlins and girts.`) after `Shaft`;
    - `Category::ALL` becomes `[Category; 35]` with `Category::SteelBuilding` after `Category::Shaft` (not in `MODEL` nor `SCHEDULABLE`);
    - `Category::key`: `Category::SteelBuilding => "category-steel-building",`;
    - `TypeData::SteelBuilding {}` (doc comment `/// Steel buildings (no properties of their own; types name them).`) after `Shaft {}`;
    - every `TypeData` match that lists `TypeData::Shaft {}` also lists `TypeData::SteelBuilding {}` (around lines 598, 671 and 984), and the category match gets `TypeData::SteelBuilding {} => Category::SteelBuilding,`.
  - `crates/cc_core/src/repair.rs`: `element_is_valid` arm `ElementKind::SteelBuilding(b) => b.origin.is_finite() && b.rotation_deg.is_finite(),`; add `| TypeData::SteelBuilding {}` next to `| TypeData::Shaft {} => {}`.
  - `crates/cc_views/src/legend.rs` (~258) and `crates/cc_ifc/src/export.rs` (~1722): add `| TypeData::SteelBuilding {}` next to `| TypeData::Shaft {}` (no legend entry, no IFC type object). Update the comment above the IFC list: `... masses, shafts, steel buildings and pads have no IFC type object.`
  - `crates/cc_app/src/icons.rs`: `Icon::SteelBuilding` after `Icon::GridFrame` in the enum, in `ALL` (bump the length from 181 to 182) and `Icon::SteelBuilding => include_str!("../../../assets/icons/ui/steel-building.svg"),`.
  - `crates/cc_app/src/ribbon.rs` (~53): `Category::SteelBuilding => Icon::SteelBuilding,`.
  - Any other exhaustive match the compiler flags: put `SteelBuilding` with the do-nothing arm or list `Shaft` sits in (`ElementKind::SteelBuilding(_) => {}`, `Category::SteelBuilding => &[]` and so on).
  - `assets/i18n/*.ftl` (12 files): `category-steel-building`.

**Interfaces:**
- Produces (module `cc_core::steel_building`):
  - consts `MAX_MEMBERS: usize = 4000`, `MISSING_SECTION: f64 = 0.2`, ranges `SPAN`, `EAVE_HEIGHT`, `SLOPE`, `BAY_SPACING`, `SPACING: RangeInclusive<f64>`, `BAYS: RangeInclusive<u32>`, defaults `DEFAULT_SPAN`, `DEFAULT_EAVE_HEIGHT`, `DEFAULT_SLOPE`, `DEFAULT_BAYS: u32`, `DEFAULT_BAY_SPACING`, `DEFAULT_SECONDARY_SPACING`.
  - `enum FrameRoof { Gable (default), Mono }` with `ALL`, `code() -> &'static str` (`gable`/`mono`), `from_code(&str) -> Option<Self>`.
  - `enum RoleKind { Column, Rafter, Purlin, Girt }` (`Copy + Ord`).
  - `struct MemberRole { pub kind: RoleKind, pub frame: u16, pub index: u16 }` (`Copy + Ord`), `MemberRole::fits(self, &ElementKind) -> bool`.
  - `struct Secondary { pub type_id: ElementTypeId, pub spacing: f64 }` (`Copy`).
  - `struct FrameMember { pub role: MemberRole, pub id: ElementId }` (`Copy`).
  - `struct SteelBuilding { origin, rotation_deg, span, eave_height, slope_percent, roof, bays, bay_spacing, column_type, rafter_type, purlins, girts, members, detached }` with:
    - `new(column_type, rafter_type, secondary: Option<ElementTypeId>) -> Self` (defaults, origin 0);
    - `check(&self) -> Result<(), String>` (numbers), `check_types(&self, &Project) -> Result<(), String>`, `reset_out_of_range(&mut self) -> usize`;
    - `type_ids(&self) -> impl Iterator<Item = ElementTypeId>`;
    - `length()`, `to_plan(DVec2) -> DVec2`, `footprint() -> [DVec2; 4]`, `transform(&mut self, &PlanTransform)`;
    - `layout(&self, &Project) -> Vec<(MemberRole, ElementTypeId, ElementKind)>`.
  - `section_box(&Project, ElementTypeId) -> (f64, f64)` (width, depth).
  - `host_type(id: ElementTypeId, name: String) -> ElementType`.
  - `Project::steel_members_of(&self, host: ElementId) -> Vec<ElementId>`.
  - `ElementKind::SteelBuilding(Box<SteelBuilding>)`, `Category::SteelBuilding`, `TypeData::SteelBuilding {}`, `Icon::SteelBuilding`.

- [ ] **Step 1: Write the module with its failing tests** in `crates/cc_core/src/steel_building.rs`:

```rust
//! Parametric steel buildings: a host element that holds the parameters of
//! a steel shed (span, eave height, roof slope and shape, bays, the types of
//! its members, purlins and girts) and owns ordinary columns and beams made
//! from them. Every edit keeps the members in step (see
//! [`crate::EditCtx`]): a changed building rewrites its members in place,
//! keeping their ids while their roles exist; a member the user edited or
//! deleted is detached, and the building leaves its role alone from then on.
//!
//! Local coordinates: x across the span (from the first column line), y
//! along the building (frame `k` at `k * bay_spacing`), mapped to plan
//! through `origin` and `rotation_deg`.

use crate::{
    Beam, Category, Column, ElementId, ElementKind, ElementType, ElementTypeId, PlanTransform, Project, TypeData,
    WallTop,
};
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;

/// Most members one building makes; the layout stops there.
pub const MAX_MEMBERS: usize = 4000;
/// Most purlin lines on a roof plane and girt rows on a side.
const MAX_LINES: usize = 999;
/// The width and depth of a type whose section cannot be read, meters.
pub const MISSING_SECTION: f64 = 0.2;

pub const SPAN: RangeInclusive<f64> = 2.0..=100.0;
pub const EAVE_HEIGHT: RangeInclusive<f64> = 1.0..=50.0;
pub const SLOPE: RangeInclusive<f64> = 0.0..=100.0;
pub const BAYS: RangeInclusive<u32> = 1..=100;
pub const BAY_SPACING: RangeInclusive<f64> = 1.0..=30.0;
/// Purlin and girt spacing.
pub const SPACING: RangeInclusive<f64> = 0.3..=10.0;

pub const DEFAULT_SPAN: f64 = 20.0;
pub const DEFAULT_EAVE_HEIGHT: f64 = 6.0;
pub const DEFAULT_SLOPE: f64 = 10.0;
pub const DEFAULT_BAYS: u32 = 5;
pub const DEFAULT_BAY_SPACING: f64 = 6.0;
pub const DEFAULT_SECONDARY_SPACING: f64 = 1.5;

/// The roof of the frames: two rafters meeting at mid-span, or one rising
/// toward `x = span`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum FrameRoof {
    #[default]
    Gable,
    Mono,
}

impl FrameRoof {
    pub const ALL: [FrameRoof; 2] = [FrameRoof::Gable, FrameRoof::Mono];

    pub fn code(self) -> &'static str {
        match self {
            FrameRoof::Gable => "gable",
            FrameRoof::Mono => "mono",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.code().eq_ignore_ascii_case(code.trim()))
    }
}

/// What a member is in its building.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RoleKind {
    Column,
    Rafter,
    Purlin,
    Girt,
}

/// A member's place: its kind, its frame (columns, rafters) or bay
/// (purlins, girts), and its index there (girts: `side * 1000 + row`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MemberRole {
    pub kind: RoleKind,
    pub frame: u16,
    pub index: u16,
}

impl MemberRole {
    /// Whether an element of `kind` can fill this role: a column for
    /// columns, a beam for the rest.
    pub fn fits(self, kind: &ElementKind) -> bool {
        match self.kind {
            RoleKind::Column => matches!(kind, ElementKind::Column(_)),
            RoleKind::Rafter | RoleKind::Purlin | RoleKind::Girt => matches!(kind, ElementKind::Beam(_)),
        }
    }
}

/// Purlins or girts: their beam type and the distance between lines
/// (purlins: along the rafter's plan run; girts: up the wall).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Secondary {
    pub type_id: ElementTypeId,
    pub spacing: f64,
}

/// A member the building manages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameMember {
    pub role: MemberRole,
    pub id: ElementId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SteelBuilding {
    /// Plan point of the first frame's left column.
    pub origin: DVec2,
    /// Turn of the local x axis (across the span), degrees counter-clockwise.
    #[serde(default)]
    pub rotation_deg: f64,
    /// Column axis to column axis, meters.
    pub span: f64,
    /// Column top above the level (the rafters' underside at the column axis), meters.
    pub eave_height: f64,
    /// Roof slope, percent.
    pub slope_percent: f64,
    #[serde(default)]
    pub roof: FrameRoof,
    /// Bays between frames (frames = bays + 1).
    pub bays: u32,
    /// Frame to frame, meters.
    pub bay_spacing: f64,
    pub column_type: ElementTypeId,
    pub rafter_type: ElementTypeId,
    #[serde(default)]
    pub purlins: Option<Secondary>,
    #[serde(default)]
    pub girts: Option<Secondary>,
    /// The members it manages, by role.
    #[serde(default)]
    pub members: Vec<FrameMember>,
    /// Roles the user took over (edited or deleted members): not made again.
    #[serde(default)]
    pub detached: Vec<MemberRole>,
}

fn within(name: &str, v: f64, range: &RangeInclusive<f64>) -> Result<(), String> {
    if v.is_finite() && range.contains(&v) {
        Ok(())
    } else {
        Err(format!("{name} must be between {} and {}", range.start(), range.end()))
    }
}

/// Reset `v` to `default` when it is out of `range`; returns 1 when it did.
fn reset(v: &mut f64, range: &RangeInclusive<f64>, default: f64) -> usize {
    if v.is_finite() && range.contains(v) {
        0
    } else {
        *v = default;
        1
    }
}

/// The width (across) and depth (up) of a column or beam type's section
/// box, [`MISSING_SECTION`] each when it cannot be read.
pub fn section_box(project: &Project, type_id: ElementTypeId) -> (f64, f64) {
    let size = project.element_type(type_id).and_then(|t| project.type_section(&t.data)).map(|s| {
        let (lo, hi) = s.outline.bbox();
        (hi.x - lo.x, hi.y - lo.y)
    });
    match size {
        Some((w, d)) if w.is_finite() && d.is_finite() && w > 0.0 && d > 0.0 => (w, d),
        _ => (MISSING_SECTION, MISSING_SECTION),
    }
}

/// The type a project gets with its first steel building.
pub fn host_type(id: ElementTypeId, name: String) -> ElementType {
    ElementType { id, name, data: TypeData::SteelBuilding {}, info: Default::default(), uid: Default::default() }
}

/// Members collected by a layout, stopping at [`MAX_MEMBERS`].
struct Members(Vec<(MemberRole, ElementTypeId, ElementKind)>);

impl Members {
    fn full(&self) -> bool {
        self.0.len() >= MAX_MEMBERS
    }

    fn push(&mut self, kind: RoleKind, frame: u32, index: usize, type_id: ElementTypeId, element: ElementKind) {
        let (Ok(frame), Ok(index)) = (u16::try_from(frame), u16::try_from(index)) else { return };
        if !self.full() {
            self.0.push((MemberRole { kind, frame, index }, type_id, element));
        }
    }
}

impl SteelBuilding {
    /// A building with the default parameters at the origin; `secondary`
    /// gives purlins and girts of that beam type.
    pub fn new(column_type: ElementTypeId, rafter_type: ElementTypeId, secondary: Option<ElementTypeId>) -> Self {
        let line = secondary.map(|type_id| Secondary { type_id, spacing: DEFAULT_SECONDARY_SPACING });
        Self {
            origin: DVec2::ZERO,
            rotation_deg: 0.0,
            span: DEFAULT_SPAN,
            eave_height: DEFAULT_EAVE_HEIGHT,
            slope_percent: DEFAULT_SLOPE,
            roof: FrameRoof::Gable,
            bays: DEFAULT_BAYS,
            bay_spacing: DEFAULT_BAY_SPACING,
            column_type,
            rafter_type,
            purlins: line,
            girts: line,
            members: Vec::new(),
            detached: Vec::new(),
        }
    }

    /// Whether every number is in its range (the first one that is not, named).
    pub fn check(&self) -> Result<(), String> {
        if !(self.origin.is_finite() && self.rotation_deg.is_finite()) {
            return Err("origin and rotation must be finite numbers".into());
        }
        within("span", self.span, &SPAN)?;
        within("eave_height", self.eave_height, &EAVE_HEIGHT)?;
        within("slope_percent", self.slope_percent, &SLOPE)?;
        within("bay_spacing", self.bay_spacing, &BAY_SPACING)?;
        if !BAYS.contains(&self.bays) {
            return Err(format!("bays must be between {} and {}", BAYS.start(), BAYS.end()));
        }
        for (name, line) in [("purlin spacing", &self.purlins), ("girt spacing", &self.girts)] {
            if let Some(s) = line {
                within(name, s.spacing, &SPACING)?;
            }
        }
        Ok(())
    }

    /// Whether the column type is a column type and the others beam types.
    pub fn check_types(&self, project: &Project) -> Result<(), String> {
        let is = |t: ElementTypeId, c: Category| project.element_type(t).is_some_and(|x| x.category() == c);
        if !is(self.column_type, Category::Column) {
            return Err(format!("type {} is not a column type", self.column_type.0));
        }
        let beams = [Some(self.rafter_type), self.purlins.map(|s| s.type_id), self.girts.map(|s| s.type_id)];
        match beams.into_iter().flatten().find(|t| !is(*t, Category::Beam)) {
            Some(t) => Err(format!("type {} is not a beam type", t.0)),
            None => Ok(()),
        }
    }

    /// Put every out-of-range number back to its default; returns how many.
    pub fn reset_out_of_range(&mut self) -> usize {
        let mut n = reset(&mut self.span, &SPAN, DEFAULT_SPAN)
            + reset(&mut self.eave_height, &EAVE_HEIGHT, DEFAULT_EAVE_HEIGHT)
            + reset(&mut self.slope_percent, &SLOPE, DEFAULT_SLOPE)
            + reset(&mut self.bay_spacing, &BAY_SPACING, DEFAULT_BAY_SPACING);
        if !BAYS.contains(&self.bays) {
            self.bays = DEFAULT_BAYS;
            n += 1;
        }
        for s in [&mut self.purlins, &mut self.girts].into_iter().flatten() {
            n += reset(&mut s.spacing, &SPACING, DEFAULT_SECONDARY_SPACING);
        }
        n
    }

    /// The member types it uses.
    pub fn type_ids(&self) -> impl Iterator<Item = ElementTypeId> + '_ {
        [Some(self.column_type), Some(self.rafter_type), self.purlins.map(|s| s.type_id), self.girts.map(|s| s.type_id)]
            .into_iter()
            .flatten()
    }

    /// Frame line to frame line, meters.
    pub fn length(&self) -> f64 {
        f64::from(self.bays) * self.bay_spacing
    }

    fn axes(&self) -> (DVec2, DVec2) {
        let x = DVec2::from_angle(self.rotation_deg.to_radians());
        (x, x.perp())
    }

    /// A local point in plan.
    pub fn to_plan(&self, local: DVec2) -> DVec2 {
        let (x, y) = self.axes();
        self.origin + x * local.x + y * local.y
    }

    /// The column lines' rectangle in plan, counter-clockwise from the origin.
    pub fn footprint(&self) -> [DVec2; 4] {
        let (w, l) = (self.span, self.length());
        [DVec2::ZERO, DVec2::new(w, 0.0), DVec2::new(w, l), DVec2::new(0.0, l)].map(|p| self.to_plan(p))
    }

    /// Move, turn or mirror it. A mirror flips the local frame's handedness,
    /// so the far gable's corner becomes the origin (the frames run the
    /// other way and the footprint stays where the mirror puts it).
    pub fn transform(&mut self, t: &PlanTransform) {
        let corner = if t.mirror { DVec2::new(0.0, self.length()) } else { DVec2::ZERO };
        self.origin = t.apply(self.to_plan(corner));
        self.rotation_deg = t.apply_angle_deg(self.rotation_deg);
    }

    fn slope(&self) -> f64 {
        self.slope_percent / 100.0
    }

    /// The rafters' top above the level at local `x` (the location line).
    fn rafter_top(&self, rafter_depth: f64, x: f64) -> f64 {
        let run = match self.roof {
            FrameRoof::Gable => x.min(self.span - x),
            FrameRoof::Mono => x,
        };
        self.eave_height + rafter_depth + self.slope() * run.clamp(0.0, self.span)
    }

    /// The local x of the purlin lines: on each roof plane from the column
    /// face every `spacing`, the last at the ridge (gable: half a purlin
    /// short of mid-span, mirrored on the other half) or at the far column
    /// face (mono); a line closer than a purlin's width to the last is left out.
    fn purlin_lines(&self, half_column: f64, width: f64, spacing: f64) -> Vec<f64> {
        let plane = |from: f64, to: f64| {
            let mut xs = Vec::new();
            let mut x = from;
            while x < to - width - 1e-9 && xs.len() < MAX_LINES {
                xs.push(x);
                x += spacing;
            }
            xs.push(to.max(from));
            xs
        };
        match self.roof {
            FrameRoof::Gable => {
                let left = plane(half_column, self.span / 2.0 - width / 2.0);
                let right: Vec<f64> = left.iter().map(|x| self.span - x).collect();
                left.into_iter().chain(right).collect()
            }
            FrameRoof::Mono => plane(half_column, self.span - half_column),
        }
    }

    /// The girt rows' tops above the level: every `spacing` from `spacing`
    /// up, below the eave.
    fn girt_rows(&self, spacing: f64) -> Vec<f64> {
        (1..=MAX_LINES)
            .map(|r| r as f64 * spacing)
            .take_while(|z| *z < self.eave_height - 1e-9)
            .collect()
    }

    /// Its members: role, type and element, in a fixed order (columns,
    /// rafters, purlins, girts), at most [`MAX_MEMBERS`]. Empty when a
    /// number is out of range.
    pub fn layout(&self, project: &Project) -> Vec<(MemberRole, ElementTypeId, ElementKind)> {
        let mut out = Members(Vec::new());
        if self.check().is_err() {
            return out.0;
        }
        let (_, column_depth) = section_box(project, self.column_type);
        let (_, rafter_depth) = section_box(project, self.rafter_type);
        let half_column = column_depth / 2.0;
        let plan = |x: f64, y: f64| self.to_plan(DVec2::new(x, y));
        let rafters = match self.roof {
            FrameRoof::Gable => vec![(0.0, self.span / 2.0), (self.span, self.span / 2.0)],
            FrameRoof::Mono => vec![(0.0, self.span)],
        };
        for k in 0..=self.bays {
            let y = f64::from(k) * self.bay_spacing;
            for (i, x) in [0.0, self.span].into_iter().enumerate() {
                // Turned a quarter so the section's depth runs across the span,
                // up to the rafters' top so they frame into it.
                let column = Column {
                    position: plan(x, y),
                    rotation_deg: self.rotation_deg + 90.0,
                    base_offset: 0.0,
                    height: self.eave_height + rafter_depth,
                    top: WallTop::Unconnected,
                    structural: true,
                };
                out.push(RoleKind::Column, k, i, self.column_type, ElementKind::Column(column));
            }
            for (i, &(a, b)) in rafters.iter().enumerate() {
                let mut beam = Beam::new(plan(a, y), plan(b, y), self.rafter_top(rafter_depth, a));
                let end = self.rafter_top(rafter_depth, b);
                beam.end_offset = ((end - beam.top_offset).abs() > 1e-9).then_some(end);
                out.push(RoleKind::Rafter, k, i, self.rafter_type, ElementKind::Beam(beam));
            }
        }
        if let Some(s) = self.purlins {
            let (width, depth) = section_box(project, s.type_id);
            let lines = self.purlin_lines(half_column, width, s.spacing);
            for k in 0..self.bays {
                let (y0, y1) = (f64::from(k) * self.bay_spacing, f64::from(k + 1) * self.bay_spacing);
                for (i, &x) in lines.iter().enumerate() {
                    if out.full() {
                        break;
                    }
                    // It bears on the rafters: its bottom on their top.
                    let beam = Beam::new(plan(x, y0), plan(x, y1), self.rafter_top(rafter_depth, x) + depth);
                    out.push(RoleKind::Purlin, k, i, s.type_id, ElementKind::Beam(beam));
                }
            }
        }
        if let Some(s) = self.girts {
            let (width, _) = section_box(project, s.type_id);
            let off = half_column + width / 2.0;
            let rows = self.girt_rows(s.spacing);
            for k in 0..self.bays {
                let (y0, y1) = (f64::from(k) * self.bay_spacing, f64::from(k + 1) * self.bay_spacing);
                for (side, x) in [(0, -off), (1, self.span + off)] {
                    for (r, &z) in rows.iter().enumerate() {
                        if out.full() {
                            break;
                        }
                        let beam = Beam::new(plan(x, y0), plan(x, y1), z);
                        out.push(RoleKind::Girt, k, side * 1000 + r, s.type_id, ElementKind::Beam(beam));
                    }
                }
            }
        }
        out.0
    }
}

impl Project {
    /// The members a steel building manages (none for other elements).
    pub fn steel_members_of(&self, host: ElementId) -> Vec<ElementId> {
        match self.element(host).map(|e| &e.kind) {
            Some(ElementKind::SteelBuilding(b)) => b.members.iter().map(|m| m.id).collect(),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ColumnSection, Document, Element, LevelId};

    fn close(a: DVec2, b: DVec2) -> bool {
        a.distance(b) < 1e-9
    }

    fn add_type(e: &mut crate::EditCtx, data: TypeData) -> ElementTypeId {
        let id = ElementTypeId(e.alloc_id());
        e.add_type(ElementType { id, name: format!("T{}", id.0), data, info: Default::default(), uid: Default::default() })
    }

    pub(super) fn element(id: ElementId, level: LevelId, type_id: ElementTypeId, kind: ElementKind) -> Element {
        Element {
            id,
            level,
            type_id,
            mark: String::new(),
            comments: String::new(),
            uid: Default::default(),
            kind,
            phase_created: Default::default(),
            phase_demolished: None,
            design_option: None,
            paints: Vec::new(),
        }
    }

    /// A document with a default gable building on the first level: columns
    /// of a 0.2 x 0.3 type, rafters, purlins and girts of a 0.1 x 0.2 beam type.
    pub(super) fn site() -> (Document, ElementId) {
        let mut doc = Document::new(Project::from_template());
        let host = doc.edit("building", |e| {
            let level = e.project().levels[0].id;
            let material = e.project().materials[0].name.clone();
            let column = add_type(
                e,
                TypeData::Column {
                    section: ColumnSection::Rectangular { width: 0.2, depth: 0.3 },
                    material: material.clone(),
                    profile: None,
                    profile_rotation_deg: 0.0,
                    profile_mirror: false,
                },
            );
            let beam = add_type(
                e,
                TypeData::Beam {
                    width: 0.1,
                    height: 0.2,
                    material,
                    shape: Default::default(),
                    profile: None,
                    profile_rotation_deg: 0.0,
                    profile_mirror: false,
                },
            );
            let ty = add_type(e, TypeData::SteelBuilding {});
            let id = ElementId(e.alloc_id());
            let b = SteelBuilding::new(column, beam, Some(beam));
            e.add_element(element(id, level, ty, ElementKind::SteelBuilding(Box::new(b))))
        });
        (doc, host)
    }

    pub(super) fn building(doc: &Document, host: ElementId) -> SteelBuilding {
        match doc.project.element(host).map(|e| &e.kind) {
            Some(ElementKind::SteelBuilding(b)) => (**b).clone(),
            _ => panic!("not a steel building"),
        }
    }

    fn count(layout: &[(MemberRole, ElementTypeId, ElementKind)], kind: RoleKind) -> usize {
        layout.iter().filter(|(r, _, _)| r.kind == kind).count()
    }

    fn find(layout: &[(MemberRole, ElementTypeId, ElementKind)], kind: RoleKind, frame: u16, index: u16) -> ElementKind {
        let role = MemberRole { kind, frame, index };
        layout.iter().find(|(r, _, _)| *r == role).map(|(_, _, k)| k.clone()).expect("role")
    }

    #[test]
    fn layout_counts_members() {
        let (doc, host) = site();
        let mut b = building(&doc, host);
        let p = &doc.project;
        let kinds = |l: &[(MemberRole, ElementTypeId, ElementKind)]| {
            [count(l, RoleKind::Column), count(l, RoleKind::Rafter), count(l, RoleKind::Purlin), count(l, RoleKind::Girt)]
        };
        // Gable, 5 bays: 6 frames of 2 columns and 2 rafters; per bay 16 purlin
        // lines (7 every 1.5 m from the column face plus the ridge, each half)
        // and 3 girt rows (1.5, 3, 4.5 m) on each side.
        assert_eq!(kinds(&b.layout(p)), [12, 12, 80, 30]);
        // Mono: 1 rafter per frame, 15 purlin lines (14 plus the far column face).
        b.roof = FrameRoof::Mono;
        assert_eq!(kinds(&b.layout(p)), [12, 6, 75, 30]);
        (b.purlins, b.girts) = (None, None);
        assert_eq!(b.layout(p).len(), 18);
        b.roof = FrameRoof::Gable;
        assert_eq!(b.layout(p).len(), 24);
    }

    #[test]
    fn rafters_rise_from_the_columns() {
        let (doc, host) = site();
        let l = building(&doc, host).layout(&doc.project);
        // Rafter depth 0.2: the top starts 0.2 over the 6 m eave and rises
        // 10 % over half the 20 m span.
        let ElementKind::Beam(left) = find(&l, RoleKind::Rafter, 0, 0) else { panic!("a beam") };
        assert!(close(left.start, DVec2::ZERO) && close(left.end, DVec2::new(10.0, 0.0)));
        assert!((left.top_offset - 6.2).abs() < 1e-9);
        assert!((left.end_offset.unwrap_or(0.0) - 7.2).abs() < 1e-9);
        let ElementKind::Beam(right) = find(&l, RoleKind::Rafter, 1, 1) else { panic!("a beam") };
        assert!(close(right.start, DVec2::new(20.0, 6.0)) && close(right.end, DVec2::new(10.0, 6.0)));
        // Columns rise to the rafters' top, turned so their depth runs across the span.
        let ElementKind::Column(c) = find(&l, RoleKind::Column, 0, 1) else { panic!("a column") };
        assert!(close(c.position, DVec2::new(20.0, 0.0)));
        assert!((c.height - 6.2).abs() < 1e-9 && (c.rotation_deg - 90.0).abs() < 1e-9);
        // The first purlin starts at the column face (0.15 m in) and bears on the rafter.
        let ElementKind::Beam(p0) = find(&l, RoleKind::Purlin, 0, 0) else { panic!("a beam") };
        assert!(close(p0.start, DVec2::new(0.15, 0.0)) && close(p0.end, DVec2::new(0.15, 6.0)));
        assert!((p0.top_offset - (6.2 + 0.015 + 0.2)).abs() < 1e-9);
        // The ridge purlins sit half a purlin off mid-span.
        let ElementKind::Beam(ridge) = find(&l, RoleKind::Purlin, 0, 7) else { panic!("a beam") };
        assert!((ridge.start.x - 9.95).abs() < 1e-9);
        // Girts: outside the column face, rows every 1.5 m below the eave.
        let ElementKind::Beam(g) = find(&l, RoleKind::Girt, 0, 1002) else { panic!("a beam") };
        assert!((g.start.x - 20.2).abs() < 1e-9 && (g.top_offset - 4.5).abs() < 1e-9);
    }

    #[test]
    fn origin_and_rotation_place_the_frames() {
        let (doc, host) = site();
        let mut b = building(&doc, host);
        (b.origin, b.rotation_deg) = (DVec2::new(5.0, 5.0), 90.0);
        let l = b.layout(&doc.project);
        let at = |frame, index| match find(&l, RoleKind::Column, frame, index) {
            ElementKind::Column(c) => c.position,
            _ => panic!("a column"),
        };
        assert!(close(at(0, 1), DVec2::new(5.0, 25.0)), "the span runs along +y");
        assert!(close(at(1, 0), DVec2::new(-1.0, 5.0)), "the frames along -x");
        assert!(close(b.footprint()[2], DVec2::new(-25.0, 25.0)));
    }

    #[test]
    fn a_mirror_keeps_the_footprint() {
        let (doc, host) = site();
        let mut b = building(&doc, host);
        b.rotation_deg = 30.0;
        let t = PlanTransform::mirror_across(DVec2::new(3.0, 0.0), DVec2::new(0.0, 1.0));
        let want: Vec<DVec2> = b.footprint().iter().map(|p| t.apply(*p)).collect();
        b.transform(&t);
        for p in b.footprint() {
            assert!(want.iter().any(|q| close(*q, p)), "{p}");
        }
    }

    #[test]
    fn layouts_stop_at_the_limit_and_need_valid_numbers() {
        let (doc, host) = site();
        let mut b = building(&doc, host);
        (b.span, b.bays, b.eave_height) = (100.0, 100, 50.0);
        for s in [&mut b.purlins, &mut b.girts].into_iter().flatten() {
            s.spacing = 0.3;
        }
        assert_eq!(b.layout(&doc.project).len(), MAX_MEMBERS);
        b.span = 150.0;
        assert!(b.check().is_err());
        assert!(b.layout(&doc.project).is_empty());
        assert_eq!(b.reset_out_of_range(), 1);
        assert_eq!(b.span, DEFAULT_SPAN);
        b.column_type = b.rafter_type;
        assert!(b.check_types(&doc.project).is_err());
    }

    #[test]
    fn roof_codes_round_trip() {
        for r in FrameRoof::ALL {
            assert_eq!(FrameRoof::from_code(r.code()), Some(r));
        }
        assert_eq!(FrameRoof::from_code(" MONO "), Some(FrameRoof::Mono));
        assert_eq!(FrameRoof::from_code("dome"), None);
    }
}
```

Notes:
- `site()` runs a real edit. Before Task 2 there is no sync, so the host has no members yet; the tests above only read `layout`.
- `SectionOutline::bbox` (`crates/cc_core/src/structural_profile.rs:368`) returns `(lo, hi)`. `TypeSection::outline` is that outline in meters. If `type_section` returns `None` for a legacy `Rectangular` column with `profile: None`, check `section_data` in `profiles_project.rs`; the fallback is `legacy_size`.
- If `glam`'s `DVec2::from_angle` is not available, use `DVec2::new(a.cos(), a.sin())`.
- `PlanTransform` must be re-exported at the crate root (`crate::PlanTransform`, used in `pad.rs` tests). If it is not, use `crate::transform::PlanTransform`.

- [ ] **Step 2: Wire the variant, the category and the type data** as listed under *Files*. In `element.rs`:

```rust
    /// A parametric steel building: it owns the columns and beams of its frames.
    SteelBuilding(Box<crate::steel_building::SteelBuilding>),
```

`Element::category`: `ElementKind::SteelBuilding(_) => Category::SteelBuilding,`. `Element::translate`: `ElementKind::SteelBuilding(b) => b.origin += delta,`. `Element::transform` (`transform.rs`): `ElementKind::SteelBuilding(b) => b.transform(t),`.

The icon `assets/icons/ui/steel-building.svg` (24 grid, white strokes, width 1.75, soft fill 20%), a portal frame with a dashed purlin line:

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M4 21V10l8-5 8 5v11z" fill="#fff" fill-opacity=".2" stroke="none"/><path d="M4 21V10l8-5 8 5v11"/><path d="M4 14h16" stroke-width="1.25" stroke-dasharray="2 2"/><path d="M2 21h20"/></svg>
```

Add `category-steel-building` to the 12 `.ftl` files under `# Steel building`.

- [ ] **Step 3: Compile the workspace.** `cargo check --workspace --tests`. Fix every non-exhaustive match the compiler reports by adding `SteelBuilding` where `Shaft` (or `Mass`) has the neutral behavior: no geometry, no schedule fields, no IFC product, no legend entry. Expected: compiles.

- [ ] **Step 4: Run the tests.** `cargo nextest run -p cc_core steel_building`. Expected: 6 pass. Then `cargo nextest run -p cc_app i18n icons`. Expected: pass.

- [ ] **Step 5: Clippy, fmt, commit.** `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all`, then `git add -A && git commit -s -m "Add the steel building element and its member layout"`.

---

### Task 2: Members kept in step (cc_core)

**Files:**
- Modify:
  - `crates/cc_core/src/document.rs`:
    - two accessors on `EditCtx`: `element_changes` and `changed_types`;
    - `finish`: call `self.sync_steel_buildings();` right after `self.sync_profiles();`;
    - `remove_element`: `pending.extend(self.project.steel_members_of(id));` next to the `pads_of` line.
  - `crates/cc_core/src/steel_building.rs`: the sync (`impl crate::EditCtx<'_>`), the repair (`impl Project`), more tests.
  - `crates/cc_core/src/transform.rs`: `Project::copy_set` leaves out the members of copied buildings.
  - `crates/cc_core/src/repair.rs`: `repairs += self.repair_steel_buildings();` right after the element-validity `loop { .. }` (before `// Dimension ends referencing missing elements…`).

**Interfaces:**
- Consumes (Task 1): everything in `cc_core::steel_building`.
- Produces:
  - `EditCtx::element_changes(&self) -> Vec<(ElementId, Option<&Element>)>` (`pub(crate)`).
  - `EditCtx::changed_types(&self) -> BTreeSet<ElementTypeId>` (`pub(crate)`).
  - `EditCtx::sync_steel_buildings(&mut self)` (`pub(crate)`).
  - `Project::repair_steel_buildings(&mut self) -> usize` (`pub(crate)`).
  - Behavior later tasks rely on: any edit that changes or adds a building (or edits a type it uses) leaves its members matching `layout`, minus `detached` roles, in the same undo step; a member whose kind, type or level an edit changed (with its building unchanged) or that an edit deleted is detached.

- [ ] **Step 1: Write the failing tests**, appended inside `mod tests` of `steel_building.rs`:

```rust
    fn role(kind: RoleKind, frame: u16, index: u16) -> MemberRole {
        MemberRole { kind, frame, index }
    }

    fn member(b: &SteelBuilding, kind: RoleKind, frame: u16, index: u16) -> ElementId {
        b.members.iter().find(|m| m.role == role(kind, frame, index)).map(|m| m.id).expect("member")
    }

    fn set(doc: &mut Document, host: ElementId, f: impl FnOnce(&mut SteelBuilding)) {
        doc.edit("set", |e| {
            if let Some(ElementKind::SteelBuilding(b)) = e.element_mut(host).map(|x| &mut x.kind) {
                f(b);
            }
        });
    }

    fn column_at(doc: &Document, id: ElementId) -> DVec2 {
        match doc.project.element(id).map(|e| &e.kind) {
            Some(ElementKind::Column(c)) => c.position,
            _ => panic!("not a column"),
        }
    }

    fn columns(b: &SteelBuilding) -> usize {
        b.members.iter().filter(|m| m.role.kind == RoleKind::Column).count()
    }

    #[test]
    fn a_new_building_gets_its_members() {
        let (doc, host) = site();
        let b = building(&doc, host);
        assert_eq!(b.members.len(), 134);
        let level = doc.project.element(host).map(|e| e.level);
        for m in &b.members {
            let e = doc.project.element(m.id).expect("the member exists");
            assert!(m.role.fits(&e.kind));
            assert_eq!(Some(e.level), level);
        }
        assert_eq!(doc.project.clone().repair(), 0, "a consistent model");
    }

    #[test]
    fn a_new_span_moves_the_members_and_keeps_their_ids() {
        let (mut doc, host) = site();
        let before = building(&doc, host);
        let right = member(&before, RoleKind::Column, 0, 1);
        set(&mut doc, host, |b| b.span = 24.0);
        let after = building(&doc, host);
        assert_eq!(member(&after, RoleKind::Column, 0, 1), right);
        assert!(close(column_at(&doc, right), DVec2::new(24.0, 0.0)));
        let frames = |b: &SteelBuilding| -> Vec<ElementId> {
            b.members.iter().filter(|m| matches!(m.role.kind, RoleKind::Column | RoleKind::Rafter)).map(|m| m.id).collect()
        };
        assert_eq!(frames(&after), frames(&before));
        // Undo puts back the members as they were, in the same step.
        assert!(doc.undo());
        assert!(close(column_at(&doc, right), DVec2::new(20.0, 0.0)));
        assert_eq!(building(&doc, host), before);
    }

    #[test]
    fn moving_the_building_moves_its_members() {
        let (mut doc, host) = site();
        let right = member(&building(&doc, host), RoleKind::Column, 0, 1);
        doc.edit("move", |e| e.transform_elements(&[host], &PlanTransform::translate(DVec2::new(2.0, 3.0))));
        assert!(close(column_at(&doc, right), DVec2::new(22.0, 3.0)));
    }

    #[test]
    fn bays_add_and_remove_frames() {
        let (mut doc, host) = site();
        let last = member(&building(&doc, host), RoleKind::Column, 5, 0);
        set(&mut doc, host, |b| b.bays = 3);
        assert_eq!(columns(&building(&doc, host)), 8);
        assert!(doc.project.element(last).is_none(), "the last frame's column is deleted");
        set(&mut doc, host, |b| b.bays = 6);
        let b = building(&doc, host);
        assert_eq!(columns(&b), 14);
        assert!(b.members.iter().all(|m| doc.project.element(m.id).is_some()));
    }

    #[test]
    fn a_member_moved_by_hand_is_left_alone() {
        let (mut doc, host) = site();
        let col = member(&building(&doc, host), RoleKind::Column, 0, 0);
        doc.edit("move", |e| e.transform_elements(&[col], &PlanTransform::translate(DVec2::new(1.0, 0.0))));
        let b = building(&doc, host);
        assert!(b.detached.contains(&role(RoleKind::Column, 0, 0)));
        assert!(b.members.iter().all(|m| m.id != col));
        set(&mut doc, host, |b| b.span = 24.0);
        assert!(close(column_at(&doc, col), DVec2::new(1.0, 0.0)), "left where the user put it");
        assert_eq!(columns(&building(&doc, host)), 11, "and not made again");
        // A new mark changes no shape: the member stays managed.
        let other = member(&building(&doc, host), RoleKind::Column, 1, 0);
        doc.edit("mark", |e| {
            if let Some(x) = e.element_mut(other) {
                x.mark = "C1".into();
            }
        });
        assert!(building(&doc, host).members.iter().any(|m| m.id == other));
    }

    #[test]
    fn a_deleted_member_is_not_made_again() {
        let (mut doc, host) = site();
        let col = member(&building(&doc, host), RoleKind::Column, 2, 1);
        doc.edit("delete", |e| e.remove_element(col));
        set(&mut doc, host, |b| b.eave_height = 7.0);
        let b = building(&doc, host);
        assert!(b.detached.contains(&role(RoleKind::Column, 2, 1)));
        assert!(b.members.iter().all(|m| m.role != role(RoleKind::Column, 2, 1)));
        assert_eq!(columns(&b), 11);
    }

    #[test]
    fn deleting_the_building_deletes_its_members() {
        let (mut doc, host) = site();
        let ids: Vec<ElementId> = building(&doc, host).members.iter().map(|m| m.id).collect();
        doc.edit("delete", |e| e.remove_element(host));
        assert!(ids.iter().all(|id| doc.project.element(*id).is_none()));
        assert!(doc.undo());
        assert!(ids.iter().all(|id| doc.project.element(*id).is_some()));
    }

    #[test]
    fn a_copy_gets_its_own_members() {
        let (mut doc, host) = site();
        let original = building(&doc, host);
        let ids: std::collections::BTreeSet<ElementId> = original.members.iter().map(|m| m.id).collect();
        // Copied alone, and copied with its members selected too: one new
        // building with members of its own, no loose duplicates.
        for with_members in [false, true] {
            let mut selection = vec![host];
            if with_members {
                selection.extend(ids.iter().copied());
            }
            let before = doc.project.elements.len();
            let copies = doc.edit("copy", |e| {
                e.copy_elements(&selection, &[PlanTransform::translate(DVec2::new(50.0, 0.0))])
            });
            assert_eq!(copies.len(), 1, "only the building is copied");
            let copy = building(&doc, copies[0]);
            assert_eq!(copy.members.len(), original.members.len());
            assert!(copy.members.iter().all(|m| !ids.contains(&m.id)));
            assert_eq!(doc.project.elements.len(), before + 1 + original.members.len());
            assert_eq!(building(&doc, host), original, "the original keeps its own");
            assert!(doc.undo());
        }
    }

    #[test]
    fn a_changed_type_regenerates() {
        let (mut doc, host) = site();
        let b = building(&doc, host);
        let rafter = member(&b, RoleKind::Rafter, 0, 0);
        doc.edit("type", |e| {
            if let Some(TypeData::Beam { height, .. }) = e.type_mut(b.rafter_type).map(|t| &mut t.data) {
                *height = 0.3;
            }
        });
        let Some(ElementKind::Beam(beam)) = doc.project.element(rafter).map(|e| &e.kind) else { panic!("a beam") };
        assert!((beam.top_offset - 6.3).abs() < 1e-9);
    }

    #[test]
    fn repair_drops_dangling_members_and_resets_bad_values() {
        let (doc, host) = site();
        let b = building(&doc, host);
        let mut p = doc.project.clone();
        p.elements.remove(&member(&b, RoleKind::Girt, 0, 0));
        if let Some(ElementKind::SteelBuilding(x)) = p.elements.get_mut(&host).map(|e| &mut e.kind) {
            x.span = f64::NAN;
            x.detached = vec![role(RoleKind::Purlin, 3, 3), role(RoleKind::Purlin, 3, 3)];
            x.column_type = x.rafter_type;
        }
        assert_eq!(p.repair(), 4);
        let Some(ElementKind::SteelBuilding(x)) = p.element(host).map(|e| &e.kind) else { panic!("kept") };
        assert_eq!(x.span, DEFAULT_SPAN);
        assert_eq!(x.members.len(), b.members.len() - 1);
        assert_eq!(x.detached.len(), 1);
        assert_eq!(p.element_type(x.column_type).map(|t| t.category()), Some(Category::Column));
        assert_eq!(p.repair(), 0);
    }
```

- [ ] **Step 2: Run them to see them fail.** `cargo nextest run -p cc_core steel_building`. Expected: the new tests FAIL (no members are made; `repair_steel_buildings` does not exist, so first it fails to compile: add an empty `pub(crate) fn repair_steel_buildings(&mut self) -> usize { 0 }` to see the assertions fail).

- [ ] **Step 3: Add the accessors and hooks in `document.rs`.** In `impl<'a> EditCtx<'a>`:

```rust
    /// The elements this edit has changed so far, each with its state before
    /// the edit (`None`: it was added). Elements touched but left as they
    /// were are skipped.
    pub(crate) fn element_changes(&self) -> Vec<(ElementId, Option<&Element>)> {
        self.tx
            .elements
            .iter()
            .filter(|(id, c)| c.0.as_ref() != self.project.elements.get(id))
            .map(|(id, c)| (*id, c.0.as_ref()))
            .collect()
    }

    /// The types this edit has added, removed or changed so far.
    pub(crate) fn changed_types(&self) -> std::collections::BTreeSet<ElementTypeId> {
        self.tx.types.iter().filter(|(id, c)| c.0.as_ref() != self.project.types.get(id)).map(|(id, _)| *id).collect()
    }
```

In `remove_element`, after `pending.extend(self.project.pads_of(id)...)`:

```rust
            pending.extend(self.project.steel_members_of(id));
```

In `finish`:

```rust
        // Legacy sections follow their profiles first, so what reads them below sees the new size.
        self.sync_profiles();
        // Steel buildings next: they read what the edit itself did to
        // elements, before the syncs below change any.
        self.sync_steel_buildings();
```

`sync_profiles` changes only types and settings, so running it first does not hide any element change from the steel sync, and a profile edit it turns into a type change does regenerate the buildings using that type.

- [ ] **Step 4: Write the sync** in `steel_building.rs` (add `Element` and `std::collections::{BTreeMap, BTreeSet}` to the imports):

```rust
impl crate::EditCtx<'_> {
    /// Keep steel buildings and their members in step, from what this edit
    /// itself changed (it runs before the syncs that touch elements):
    /// 1. a member the edit deleted, or whose kind, type or level it changed
    ///    while leaving its building alone, is detached (its role leaves
    ///    `members` for `detached`; the element stays as the user left it);
    /// 2. a building the edit added or changed, or whose member types it
    ///    changed, gets the members of its layout (except detached roles):
    ///    existing ones rewritten in place (same ids), missing ones added,
    ///    those whose role is gone deleted. A building added with members
    ///    that are another building's (a copy) or gone (a paste after a
    ///    cut) gets its own.
    pub(crate) fn sync_steel_buildings(&mut self) {
        let p = self.project();
        if !p.elements.values().any(|e| matches!(e.kind, ElementKind::SteelBuilding(_))) {
            return;
        }
        let types = self.changed_types();
        let (mut added, mut changed, mut removed, mut reshaped) =
            (BTreeSet::new(), BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
        for (id, before) in self.element_changes() {
            match (before, p.element(id)) {
                (None, Some(_)) => {
                    added.insert(id);
                    changed.insert(id);
                }
                (Some(_), None) => {
                    removed.insert(id);
                }
                (Some(b), Some(n)) => {
                    changed.insert(id);
                    if b.kind != n.kind || b.type_id != n.type_id || b.level != n.level {
                        reshaped.insert(id);
                    }
                }
                (None, None) => {}
            }
        }
        let mut detach: Vec<(ElementId, Vec<MemberRole>)> = Vec::new();
        let mut regenerate: Vec<ElementId> = Vec::new();
        for e in p.elements.values() {
            let ElementKind::SteelBuilding(b) = &e.kind else { continue };
            let host_changed = changed.contains(&e.id) || b.type_ids().any(|t| types.contains(&t));
            let taken: Vec<MemberRole> = b
                .members
                .iter()
                .filter(|m| removed.contains(&m.id) || (reshaped.contains(&m.id) && !host_changed))
                .map(|m| m.role)
                .collect();
            if !taken.is_empty() {
                detach.push((e.id, taken));
            }
            if host_changed {
                regenerate.push(e.id);
            }
        }
        for (host, roles) in detach {
            if let Some(ElementKind::SteelBuilding(b)) = self.element_mut(host).map(|e| &mut e.kind) {
                b.members.retain(|m| !roles.contains(&m.role));
                for r in roles {
                    if !b.detached.contains(&r) {
                        b.detached.push(r);
                    }
                }
            }
        }
        for host in regenerate {
            self.regenerate_steel_building(host, added.contains(&host));
        }
    }

    /// Give building `host` the members of its layout (see
    /// [`Self::sync_steel_buildings`]); `added` when this edit added it.
    fn regenerate_steel_building(&mut self, host: ElementId, added: bool) {
        let p = self.project();
        let Some(e) = p.element(host) else { return };
        let ElementKind::SteelBuilding(b) = &e.kind else { return };
        let b = (**b).clone();
        let (level, phase_created, phase_demolished, design_option) =
            (e.level, e.phase_created, e.phase_demolished, e.design_option);
        // A copy keeps none of the original's members.
        let others: BTreeSet<ElementId> = if added {
            p.elements
                .values()
                .filter(|o| o.id != host)
                .filter_map(|o| match &o.kind {
                    ElementKind::SteelBuilding(x) => Some(x.members.iter().map(|m| m.id)),
                    _ => None,
                })
                .flatten()
                .collect()
        } else {
            BTreeSet::new()
        };
        let mut wanted: BTreeMap<MemberRole, (ElementTypeId, ElementKind)> = b
            .layout(p)
            .into_iter()
            .filter(|(r, _, _)| !b.detached.contains(r))
            .map(|(r, t, k)| (r, (t, k)))
            .collect();
        let (mut kept, mut gone, mut rewrite) = (Vec::new(), Vec::new(), Vec::new());
        for m in &b.members {
            let Some(x) = p.element(m.id).filter(|x| !others.contains(&m.id) && m.role.fits(&x.kind)) else {
                // Gone, another building's, or no longer a column or beam: dropped from the list.
                continue;
            };
            match wanted.remove(&m.role) {
                Some((t, kind)) => {
                    if x.kind != kind || x.type_id != t || x.level != level {
                        rewrite.push((m.id, t, kind));
                    }
                    kept.push(*m);
                }
                None => gone.push(m.id),
            }
        }
        for (id, t, kind) in rewrite {
            if let Some(x) = self.element_mut(id) {
                (x.kind, x.type_id, x.level) = (kind, t, level);
            }
        }
        for id in gone {
            self.remove_element(id);
        }
        for (role, (type_id, kind)) in wanted {
            let id = ElementId(self.alloc_id());
            self.add_element(Element {
                id,
                level,
                type_id,
                mark: String::new(),
                comments: String::new(),
                kind,
                uid: Default::default(),
                phase_created,
                phase_demolished,
                design_option,
                paints: Vec::new(),
            });
            kept.push(FrameMember { role, id });
        }
        kept.sort_by_key(|m| m.role);
        if kept != b.members
            && let Some(ElementKind::SteelBuilding(x)) = self.element_mut(host).map(|e| &mut e.kind)
        {
            x.members = kept;
        }
    }
}
```

Notes:
- `added`/`changed`/`removed`/`reshaped` are four `BTreeSet<ElementId>`; if type inference fails on the tuple, annotate: `let mut added: BTreeSet<ElementId> = BTreeSet::new();` and so on.
- `p` borrows `self` immutably; it must not be used after the first `self.element_mut`. The code above ends its use before that. If the borrow checker complains about `self.element_changes()` and `p` together, collect `element_changes()` into owned `(ElementId, Option<Element>)` first (`.map(|(id, e)| (id, e.cloned()))`).
- A new building's `phase_created` is still unassigned when the sync runs; `sync_phases`, which runs after, gives the building and its new members the creation phase (both were added by this edit). `sync_options` likewise puts both into the active design option.

- [ ] **Step 5: Leave out the members of copied buildings** in `Project::copy_set` (`transform.rs`), just before `out`:

```rust
        // A copied steel building makes its own members: its originals are not copied.
        let managed: std::collections::BTreeSet<ElementId> = set.iter().flat_map(|id| self.steel_members_of(*id)).collect();
        out.retain(|e| !managed.contains(&e.id));
        out
```

- [ ] **Step 6: Write the repair** in `steel_building.rs`:

```rust
/// `slot` if it is a type of `category`, else the category's default type
/// (`Some(1)`: one repair); `None` when there is none.
fn fix_type(p: &Project, slot: &mut ElementTypeId, category: Category) -> Option<usize> {
    if p.element_type(*slot).is_some_and(|t| t.category() == category) {
        return Some(0);
    }
    *slot = p.default_type(category)?;
    Some(1)
}

impl Project {
    /// Steel buildings from a file: out-of-range numbers back to their
    /// defaults, missing or wrong types to the category's default (the
    /// building goes when there is none), members that are gone, not a
    /// column or beam of their role, repeated or another building's
    /// dropped, and repeated detached roles dropped. Returns the repairs.
    pub(crate) fn repair_steel_buildings(&mut self) -> usize {
        let ids: Vec<ElementId> = self
            .elements
            .values()
            .filter(|e| matches!(e.kind, ElementKind::SteelBuilding(_)))
            .map(|e| e.id)
            .collect();
        let mut repairs = 0;
        let mut claimed = BTreeSet::new();
        let mut dropped = Vec::new();
        for id in ids {
            let Some(ElementKind::SteelBuilding(b)) = self.elements.get(&id).map(|e| &e.kind) else { continue };
            let mut b = (**b).clone();
            let mut n = b.reset_out_of_range();
            let types = (|| {
                let mut k = fix_type(self, &mut b.column_type, Category::Column)?;
                k += fix_type(self, &mut b.rafter_type, Category::Beam)?;
                for s in [&mut b.purlins, &mut b.girts].into_iter().flatten() {
                    k += fix_type(self, &mut s.type_id, Category::Beam)?;
                }
                Some(k)
            })();
            let Some(k) = types else {
                dropped.push(id);
                continue;
            };
            n += k;
            let mut roles = BTreeSet::new();
            let before = b.members.len();
            b.members.retain(|m| {
                self.element(m.id).is_some_and(|e| m.role.fits(&e.kind)) && roles.insert(m.role) && claimed.insert(m.id)
            });
            n += before - b.members.len();
            let mut seen = BTreeSet::new();
            let before = b.detached.len();
            b.detached.retain(|r| seen.insert(*r));
            n += before - b.detached.len();
            if n > 0 {
                if let Some(ElementKind::SteelBuilding(x)) = self.elements.get_mut(&id).map(|e| &mut e.kind) {
                    **x = b;
                }
                repairs += n;
            }
        }
        for id in dropped {
            self.elements.remove(&id);
            repairs += 1;
        }
        repairs
    }
}
```

Call it from `Project::repair` (`repair.rs`) right after the element-validity `loop { .. }`:

```rust
        // Steel buildings: numbers, types and members (after invalid elements went).
        repairs += self.repair_steel_buildings();
```

If the closure borrowing `self` and `b` together does not compile, write the type fixes as four sequential `match fix_type(..) { Some(k) => n += k, None => { dropped.push(id); continue 'outer } }` with a labeled loop.

- [ ] **Step 7: Run the tests.** `cargo nextest run -p cc_core steel_building`. Expected: all pass. Then the whole crate: `cargo nextest run -p cc_core`. Expected: pass (the copy, pad and transform tests are unchanged).

If `a_new_building_gets_its_members` fails on `repair() == 0`, print what the repair changes on a clone. A material name the template lacks would be one cause; the test uses the template's first material.

- [ ] **Step 8: Clippy, fmt, commit.** `cargo clippy -p cc_core --all-targets -- -D warnings`, `cargo fmt --all`, `git add -A && git commit -s -m "Keep steel building members in step with their building"`.

---

### Task 3: `steel_building_create`, `element_get` and `element_set` (cc_engine)

**Files:**
- Create:
  - `crates/cc_engine/src/commands/steel_building.rs`
  - `crates/cc_engine/tests/steel_building.rs`
- Modify:
  - `crates/cc_engine/src/commands/structure_material.rs`: add `catalog_type`; `make_default` uses it for steel and timber.
  - `crates/cc_engine/src/commands/mod.rs`: `pub mod steel_building;`
  - `crates/cc_engine/src/registry.rs`: `steel_building::SteelBuildingCreate,` after `structure_material::StructuralTypeFor,`.
  - `crates/cc_engine/src/commands/query.rs` (`element_json`, the `match &e.kind` that fills `derived`): `ElementKind::SteelBuilding(b) => super::steel_building::derived(b, &mut derived),`.
  - `crates/cc_engine/src/commands/edit.rs` (`patched`): the `SteelBuilding` check.
  - `crates/cc_engine/tests/main.rs`: `mod steel_building;` in alphabetical order (after `mod skp;`).
  - `assets/i18n/*.ftl`: `tpl-type-steel-building`, `tx-steel-building`.
  - `docs/commands.md`: regenerated.

**Interfaces:**
- Consumes (Tasks 1–2): `cc_core::steel_building::{SteelBuilding, FrameRoof, Secondary, RoleKind, host_type, DEFAULT_*}`, `Project::steel_members_of`, the sync.
- Produces:
  - `pub(crate) fn catalog_type(s: &mut Session, category: Category, series: &str, designation: &str) -> Result<ElementTypeId>` in `structure_material.rs`.
  - `pub const FRAME_DEFAULT: (&str, &str) = ("W", "W310x38,7");`, `pub const SECONDARY_DEFAULT: (&str, &str) = ("NBR6355-Ue", "Ue 150x60x20x2,00");`
  - `pub struct SecondaryMembers { pub type_ref: Option<Ref> /* "type" */, pub spacing: Option<f64> }`
  - `pub struct SteelBuildingCreate { level, origin, rotation, span, eave_height, slope_percent, roof, bays, bay_spacing, column_type, rafter_type, purlins: Option<Option<SecondaryMembers>>, girts: Option<Option<SecondaryMembers>> }` (all `Option`, `Default`); command `steel_building_create`, returns `{ id, members: [ids] }`.
  - `element_get` `derived.members` = `{ columns, rafters, purlins, girts }` (id lists in role order) and `derived.detached` (a count).

- [ ] **Step 1: Write the failing engine tests** in `crates/cc_engine/tests/steel_building.rs`:

```rust
//! Parametric steel buildings: the host element keeps its columns, rafters,
//! purlins and girts in step with its parameters.

use cc_core::{ElementId, ElementKind};
use cc_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().unwrap()
}

fn types_named(s: &Session, name: &str) -> usize {
    s.project().types.values().filter(|t| t.name == name).count()
}

fn derived(s: &mut Session, host: u64) -> Value {
    run(s, "element_get", json!({"id": host}))["derived"].clone()
}

fn column_x(s: &Session, id: u64) -> f64 {
    match s.project().element(ElementId(id)).map(|e| &e.kind) {
        Some(ElementKind::Column(c)) => c.position.x,
        _ => panic!("not a column"),
    }
}

fn buildings(s: &Session) -> usize {
    s.project().elements.values().filter(|e| matches!(e.kind, ElementKind::SteelBuilding(_))).count()
}

#[test]
fn defaults_make_the_catalog_types_once() {
    let mut s = Session::default();
    let depth = s.doc.undo_depth();
    let out = run(&mut s, "steel_building_create", json!({}));
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    // 6 frames of 2 columns and 2 rafters, plus purlins and girts.
    assert!(out["members"].as_array().unwrap().len() > 24);
    assert_eq!(types_named(&s, "W310x38,7"), 2, "a column type and a beam type");
    assert_eq!(types_named(&s, "Ue 150x60x20x2,00"), 1, "purlins and girts share one");
    run(&mut s, "steel_building_create", json!({"origin": [40, 0]}));
    assert_eq!(types_named(&s, "W310x38,7"), 2);
    assert_eq!(types_named(&s, "Ue 150x60x20x2,00"), 1);
    assert_eq!(buildings(&s), 2);
    run(&mut s, "undo", json!({}));
    run(&mut s, "undo", json!({}));
    assert_eq!(types_named(&s, "W310x38,7"), 0, "undone with the first building");
    assert_eq!(buildings(&s), 0);
}

#[test]
fn element_get_lists_the_members_by_role() {
    let mut s = Session::default();
    let host = id(&run(&mut s, "steel_building_create", json!({"bays": 2, "purlins": null, "girts": null})));
    let d = derived(&mut s, host);
    let len = |k: &str| d["members"][k].as_array().map_or(0, Vec::len);
    assert_eq!([len("columns"), len("rafters"), len("purlins"), len("girts")], [6, 6, 0, 0]);
    assert_eq!(d["detached"], json!(0));
    let params = &run(&mut s, "element_get", json!({"id": host}))["params"];
    assert_eq!(params["span"], json!(20.0));
    assert_eq!(params["roof"], json!("Gable"));
}

#[test]
fn element_set_regenerates_in_one_step() {
    let mut s = Session::default();
    let host = id(&run(&mut s, "steel_building_create", json!({"purlins": null, "girts": null})));
    let before = derived(&mut s, host)["members"].clone();
    let right = before["columns"][1].as_u64().unwrap();
    let depth = s.doc.undo_depth();
    run(&mut s, "element_set", json!({"id": host, "params": {"span": 24.0}}));
    assert_eq!(s.doc.undo_depth(), depth + 1);
    let after = derived(&mut s, host)["members"].clone();
    assert_eq!(after["columns"], before["columns"], "same ids");
    assert!((column_x(&s, right) - 24.0).abs() < 1e-9);
    run(&mut s, "undo", json!({}));
    assert!((column_x(&s, right) - 20.0).abs() < 1e-9);
}

#[test]
fn a_member_moved_by_command_is_detached() {
    let mut s = Session::default();
    let host = id(&run(&mut s, "steel_building_create", json!({"purlins": null, "girts": null})));
    let col = derived(&mut s, host)["members"]["columns"][0].as_u64().unwrap();
    run(&mut s, "elements_move", json!({"ids": [col], "delta": [0.5, 0]}));
    let d = derived(&mut s, host);
    assert_eq!(d["detached"], json!(1));
    assert_eq!(d["members"]["columns"].as_array().unwrap().len(), 11);
    run(&mut s, "element_set", json!({"id": host, "params": {"span": 30.0}}));
    assert!((column_x(&s, col) - 0.5).abs() < 1e-9, "left alone");
}

#[test]
fn out_of_range_values_are_refused() {
    let mut s = Session::default();
    for args in [
        json!({"span": 150}),
        json!({"bays": 0}),
        json!({"slope_percent": -1}),
        json!({"eave_height": 0.5}),
        json!({"purlins": {"spacing": 0.1}}),
        json!({"roof": "dome"}),
        json!({"column_type": "no such type"}),
    ] {
        assert!(s.execute("steel_building_create", args.clone()).is_err(), "{args}");
    }
    assert_eq!(buildings(&s), 0);
    assert_eq!(types_named(&s, "W310x38,7"), 0, "nothing was made before the refusal");
    let host = id(&run(&mut s, "steel_building_create", json!({})));
    let depth = s.doc.undo_depth();
    for params in [json!({"eave_height": 80}), json!({"members": []}), json!({"bay_spacing": 0})] {
        assert!(s.execute("element_set", json!({"id": host, "params": params})).is_err(), "{params}");
    }
    assert_eq!(s.doc.undo_depth(), depth);
}
```

- [ ] **Step 2: Run them to see them fail.** `cargo nextest run -p cc_engine --test main steel_building`. Expected: FAIL with `unknown command: steel_building_create`.

- [ ] **Step 3: Add `catalog_type`** to `structure_material.rs` and use it in `make_default`:

```rust
/// The first `category` type whose profile is catalog row `designation` of
/// `series`, else one made from the row (the profile imported when the
/// project lacks it). What it makes takes undo steps of its own: callers
/// squash them into theirs.
pub(crate) fn catalog_type(
    s: &mut Session,
    category: Category,
    series: &str,
    designation: &str,
) -> Result<ElementTypeId> {
    let p = s.project();
    let is_row = |d: &cc_core::structural_profile::ProfileDef| {
        d.catalog.as_ref().is_some_and(|c| c.series == series && c.designation == designation) || d.name == designation
    };
    let found = p.types_of(category).find(|t| p.type_profile_def(&t.data).is_some_and(is_row)).map(|t| t.id);
    if let Some(id) = found {
        return Ok(id);
    }
    let material = cc_core::profile_catalog::series(series)
        .map_or(StructuralMaterial::Steel, |s| StructuralMaterial::of_profile(s.material));
    let profile = catalog_profile(s, (series, designation), material)?;
    let code = if category == Category::Column { "column" } else { "beam" };
    let out = ProfileTypeCreate { profile: Ref::Name(profile), category: code.into(), ..Default::default() }.run(s)?;
    out["id"].as_u64().map(ElementTypeId).ok_or_else(|| EngineError::not_found("the new type"))
}

fn make_default(s: &mut Session, category: Category, material: StructuralMaterial) -> Result<ElementTypeId> {
    match material {
        StructuralMaterial::Steel => catalog_type(s, category, STEEL_DEFAULT.0, STEEL_DEFAULT.1),
        StructuralMaterial::Timber => catalog_type(s, category, TIMBER_DEFAULT.0, TIMBER_DEFAULT.1),
        StructuralMaterial::Concrete => {
            let profile = concrete_profile(s)?;
            let category = if category == Category::Column { "column" } else { "beam" };
            let out = ProfileTypeCreate { profile: Ref::Name(profile), category: category.into(), ..Default::default() }
                .run(s)?;
            out["id"].as_u64().map(ElementTypeId).ok_or_else(|| EngineError::not_found("the new type"))
        }
    }
}
```

`ProfileDef` lives in `cc_core::structural_profile` (`pub catalog: Option<CatalogRef>`); if it is re-exported as `cc_core::ProfileDef`, use that path. Run `cargo nextest run -p cc_engine --test main structure_materials` after this step: those tests must still pass.

- [ ] **Step 4: Write the command** in `crates/cc_engine/src/commands/steel_building.rs`:

```rust
//! Parametric steel buildings (see `cc_core::steel_building`): the host
//! element and the catalog types its members get by default.

use super::structure_material::catalog_type;
use super::{add_element, edit};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, point};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::steel_building::{self as sb, FrameRoof, RoleKind, Secondary, SteelBuilding};
use cc_core::{Category, ElementKind, ElementType, ElementTypeId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Columns and rafters by default: (series, designation).
pub const FRAME_DEFAULT: (&str, &str) = ("W", "W310x38,7");
/// Purlins and girts by default.
pub const SECONDARY_DEFAULT: (&str, &str) = ("NBR6355-Ue", "Ue 150x60x20x2,00");

/// The type a project gets with its first steel building.
pub fn new_steel_building_type(id: ElementTypeId) -> ElementType {
    sb::host_type(id, t!("tpl-type-steel-building"))
}

/// Purlins or girts.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct SecondaryMembers {
    /// Beam type id or name (default: the catalog's `Ue 150x60x20x2,00`, made when missing).
    #[serde(default, rename = "type")]
    pub type_ref: Option<Ref>,
    /// Distance between lines, meters, 0.3 to 10 (default 1.5): purlins
    /// along the rafter's plan run, girts up the wall.
    #[serde(default)]
    pub spacing: Option<f64>,
}

/// Create a parametric steel building: portal frames (two columns and the
/// rafters of a gable or mono-pitch roof) every `bay_spacing` along it,
/// purlins on the roof and girts on both long sides, as ordinary columns
/// and beams the building keeps in step with its parameters (`element_set`
/// regenerates them, keeping their ids). A member edited or deleted by hand
/// is detached and left alone. Default types come from the profile catalog
/// (`W310x38,7` for columns and rafters, `Ue 150x60x20x2,00` for purlins and
/// girts), made when missing. Out-of-range values are refused. One undo
/// step; returns the building's `id` and its `members`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct SteelBuildingCreate {
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Plan point `[x, y]` of the first frame's left column (default `[0, 0]`).
    #[serde(default)]
    pub origin: Option<Point>,
    /// Turn of the span direction, degrees counter-clockwise (default 0).
    #[serde(default)]
    pub rotation: Option<f64>,
    /// Column axis to column axis, meters, 2 to 100 (default 20).
    #[serde(default)]
    pub span: Option<f64>,
    /// Underside of the rafters at the columns above the level, meters, 1 to 50 (default 6).
    #[serde(default)]
    pub eave_height: Option<f64>,
    /// Roof slope, percent, 0 to 100 (default 10).
    #[serde(default)]
    pub slope_percent: Option<f64>,
    /// `gable` (ridge at mid-span, default) or `mono` (rising toward the far columns).
    #[serde(default)]
    pub roof: Option<String>,
    /// Bays, 1 to 100 (default 5); there is one frame more.
    #[serde(default)]
    pub bays: Option<u32>,
    /// Frame to frame, meters, 1 to 30 (default 6).
    #[serde(default)]
    pub bay_spacing: Option<f64>,
    /// Column type id or name (default: catalog `W310x38,7`).
    #[serde(default)]
    pub column_type: Option<Ref>,
    /// Rafter (beam) type id or name (default: catalog `W310x38,7`).
    #[serde(default)]
    pub rafter_type: Option<Ref>,
    /// Purlins `{ "type", "spacing" }`, or `null` for none (default: on).
    #[serde(default, deserialize_with = "refs::nullable", skip_serializing_if = "Option::is_none")]
    pub purlins: Option<Option<SecondaryMembers>>,
    /// Girts on both long sides `{ "type", "spacing" }`, or `null` for none (default: on).
    #[serde(default, deserialize_with = "refs::nullable", skip_serializing_if = "Option::is_none")]
    pub girts: Option<Option<SecondaryMembers>>,
}

/// The spacing of purlins or girts as asked: `None` for none (`null`).
fn spacing(arg: &Option<Option<SecondaryMembers>>) -> Option<f64> {
    match arg {
        Some(None) => None,
        Some(Some(m)) => Some(m.spacing.unwrap_or(sb::DEFAULT_SECONDARY_SPACING)),
        None => Some(sb::DEFAULT_SECONDARY_SPACING),
    }
}

fn type_arg(arg: &Option<Option<SecondaryMembers>>) -> Option<&Ref> {
    arg.as_ref().and_then(|o| o.as_ref()).and_then(|m| m.type_ref.as_ref())
}

/// The given type, else the catalog row's (made when missing).
fn type_or_row(
    s: &mut Session,
    given: Option<ElementTypeId>,
    category: Category,
    (series, row): (&str, &str),
) -> Result<ElementTypeId> {
    match given {
        Some(t) => Ok(t),
        None => catalog_type(s, category, series, row),
    }
}

impl Command for SteelBuildingCreate {
    const NAME: &'static str = "steel_building_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        // Everything that can be refused is checked before anything is made.
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let roof = match &self.roof {
            Some(r) => FrameRoof::from_code(r)
                .ok_or_else(|| EngineError::invalid(format!("unknown roof '{r}' (gable, mono)")))?,
            None => FrameRoof::Gable,
        };
        let placeholder = ElementTypeId(0);
        let line = |spacing: Option<f64>| spacing.map(|spacing| Secondary { type_id: placeholder, spacing });
        let mut building = SteelBuilding {
            origin: point(self.origin.unwrap_or([0.0, 0.0]))?,
            rotation_deg: self.rotation.unwrap_or(0.0),
            span: self.span.unwrap_or(sb::DEFAULT_SPAN),
            eave_height: self.eave_height.unwrap_or(sb::DEFAULT_EAVE_HEIGHT),
            slope_percent: self.slope_percent.unwrap_or(sb::DEFAULT_SLOPE),
            roof,
            bays: self.bays.unwrap_or(sb::DEFAULT_BAYS),
            bay_spacing: self.bay_spacing.unwrap_or(sb::DEFAULT_BAY_SPACING),
            column_type: placeholder,
            rafter_type: placeholder,
            purlins: line(spacing(&self.purlins)),
            girts: line(spacing(&self.girts)),
            members: Vec::new(),
            detached: Vec::new(),
        };
        building.check().map_err(EngineError::invalid)?;
        let given = |r: Option<&Ref>, c: Category| r.map(|r| refs::type_of(p, c, Some(r))).transpose();
        let column_given = given(self.column_type.as_ref(), Category::Column)?;
        let rafter_given = given(self.rafter_type.as_ref(), Category::Beam)?;
        let purlin_given = given(type_arg(&self.purlins), Category::Beam)?;
        let girt_given = given(type_arg(&self.girts), Category::Beam)?;
        let host_type = p.default_type(Category::SteelBuilding);
        let depth = s.doc.undo_depth();
        let result = (|| -> Result<Value> {
            building.column_type = type_or_row(s, column_given, Category::Column, FRAME_DEFAULT)?;
            building.rafter_type = type_or_row(s, rafter_given, Category::Beam, FRAME_DEFAULT)?;
            if let Some(line) = &mut building.purlins {
                line.type_id = type_or_row(s, purlin_given, Category::Beam, SECONDARY_DEFAULT)?;
            }
            if let Some(line) = &mut building.girts {
                line.type_id = type_or_row(s, girt_given, Category::Beam, SECONDARY_DEFAULT)?;
            }
            building.check_types(s.project()).map_err(EngineError::invalid)?;
            let kind = ElementKind::SteelBuilding(Box::new(building));
            let id = edit(s, t!("tx-steel-building"), |e| {
                let type_id = match host_type {
                    Some(t) => t,
                    None => {
                        let id = ElementTypeId(e.alloc_id());
                        e.add_type(new_steel_building_type(id))
                    }
                };
                Ok(add_element(e, level, type_id, kind))
            })?;
            let members: Vec<u64> = s.project().steel_members_of(id).iter().map(|m| m.0).collect();
            Ok(json!({ "id": id.0, "members": members }))
        })();
        match result {
            Ok(v) => {
                s.doc.squash_since(depth, t!("tx-steel-building"));
                Ok(v)
            }
            Err(e) => {
                s.undo_to(depth);
                Err(e)
            }
        }
    }
}

/// `element_get`'s derived values of a building: its member ids by role
/// kind and how many roles are detached.
pub(crate) fn derived(b: &SteelBuilding, out: &mut serde_json::Map<String, Value>) {
    let ids = |k: RoleKind| -> Vec<u64> { b.members.iter().filter(|m| m.role.kind == k).map(|m| m.id.0).collect() };
    out.insert(
        "members".into(),
        json!({
            "columns": ids(RoleKind::Column),
            "rafters": ids(RoleKind::Rafter),
            "purlins": ids(RoleKind::Purlin),
            "girts": ids(RoleKind::Girt),
        }),
    );
    out.insert("detached".into(), json!(b.detached.len()));
    out.insert("length".into(), json!(b.length()));
}
```

Notes:
- `refs::point` must accept `[f64; 2]` and check finiteness (`crates/cc_engine/src/refs.rs:28`).
- `building` is moved into the closure by `ElementKind::SteelBuilding(Box::new(building))`; if the closure's capture makes that awkward, build `kind` from a `building.clone()`.
- `p` borrows `s`: its last use is `host_type`; the closure borrows `s` mutably afterwards.
- `add_element` here is `commands::add_element(e, level, type_id, kind)`; the sync in `EditCtx::finish` adds the members in the same edit, so `steel_members_of(id)` lists them right after.

- [ ] **Step 5: `element_set` checks.** In `edit.rs` `patched`, add to the `match &kind` after the patch:

```rust
        cc_core::ElementKind::SteelBuilding(b) => {
            if let cc_core::ElementKind::SteelBuilding(old) = &element.kind
                && b.members != old.members
            {
                return Err(EngineError::invalid(
                    "a steel building keeps its own members: change its parameters, or edit a member to detach it",
                ));
            }
            b.check().and_then(|()| b.check_types(p)).map_err(EngineError::invalid)?;
        }
```

`detached` stays editable (the palette's *Reset Members* clears it).

- [ ] **Step 6: Register, derive, keys.**
  - `pub mod steel_building;` in `commands/mod.rs`.
  - `steel_building::SteelBuildingCreate,` in `registry!` after `structure_material::StructuralTypeFor,`.
  - In `query.rs` `element_json`, the arm `ElementKind::SteelBuilding(b) => super::steel_building::derived(b, &mut derived),` in the `match &e.kind` that fills `derived` (the one with the `ElementKind::Shaft(sh)` arm).
  - `mod steel_building;` in `tests/main.rs`.
  - `tpl-type-steel-building` and `tx-steel-building` in the 12 `.ftl` files.

- [ ] **Step 7: Run the tests.** `cargo nextest run -p cc_engine --test main steel_building structure_materials`. Expected: all pass.

If `defaults_make_the_catalog_types_once` fails on the names, print `s.project().type_name(id)` of the new types: `ProfileTypeCreate` names a type after its profile, made unique in its category; adapt the expected names to what it gives (the counts must hold).

- [ ] **Step 8: Reference, checks, commit.**
  - `cargo run -q -p cc_cli -- reference > docs/commands.md`
  - `cargo nextest run -p cc_cli` and `cargo nextest run -p cc_app i18n`
  - `cargo clippy -p cc_engine --all-targets -- -D warnings`, `cargo fmt --all`
  - `git add -A && git commit -s -m "Add steel_building_create and edit steel buildings through element_set"`

---

### Task 4: Plan footprint and IFC export (cc_views, cc_ifc)

**Files:**
- Modify:
  - `crates/cc_views/src/plan.rs`: `draw_steel_buildings`, called in `plan_of` after `draw_shafts` when not a ceiling plan; `is_structural` returns true for `ElementKind::SteelBuilding(_)`; a test.
  - `crates/cc_engine/tests/steel_building.rs`: the IFC test.

**Interfaces:**
- Consumes (Tasks 1–3): `SteelBuilding::footprint`, `SteelBuilding::new`, `steel_building::host_type`, `steel_building_create`.
- Produces: plans carry four `Prim::Line { style: LineStyle::Hidden, element: Some(host) }`, so the host is picked by `Drawing2D::hit_test`.

- [ ] **Step 1: Write the failing tests.** In `plan.rs` `mod tests`:

```rust
    /// A steel building draws its footprint dashed as its own lines, so it
    /// can be picked; its members draw themselves.
    #[test]
    fn steel_buildings_draw_their_footprint() {
        let mut doc = Document::new(Project::from_template());
        let level = doc.project.levels[0].id;
        let host = doc.edit("t", |e| {
            let column = e.project().default_type(Category::Column).unwrap();
            let beam = e.project().default_type(Category::Beam).unwrap();
            let ty = cc_core::ElementTypeId(e.alloc_id());
            e.add_type(cc_core::steel_building::host_type(ty, "Steel building".into()));
            let b = cc_core::steel_building::SteelBuilding::new(column, beam, None);
            let id = ElementId(e.alloc_id());
            e.add_element(Element {
                id,
                level,
                type_id: ty,
                mark: String::new(),
                comments: String::new(),
                uid: Default::default(),
                kind: ElementKind::SteelBuilding(Box::new(b)),
                phase_created: Default::default(),
                phase_demolished: None,
                design_option: None,
                paints: Vec::new(),
            })
        });
        let geom = ModelGeometry::build(&doc.project);
        let d = plan(&doc.project, &geom, level, 1.2, false, 100);
        let lines = d
            .prims
            .iter()
            .filter(|p| matches!(p, Prim::Line { style: LineStyle::Hidden, element: Some(e), .. } if *e == host))
            .count();
        assert_eq!(lines, 4);
        // Mid-bay on a long side: nothing else is there.
        assert_eq!(d.hit_test(DVec2::new(0.0, 3.0), 0.01), Some(host));
    }
```

In `crates/cc_engine/tests/steel_building.rs`:

```rust
#[test]
fn ifc_export_writes_the_members_and_not_the_building() {
    let mut s = Session::default();
    let host = id(&run(&mut s, "steel_building_create", json!({"bays": 2})));
    let text = cc_ifc::export(s.project(), &cc_ifc::ExportOptions::default());
    let p = s.project();
    let count = |f: fn(&ElementKind) -> bool| p.elements.values().filter(|e| f(&e.kind)).count();
    assert_eq!(text.matches("IFCCOLUMN(").count(), count(|k| matches!(k, ElementKind::Column(_))));
    assert_eq!(text.matches("IFCBEAM(").count(), count(|k| matches!(k, ElementKind::Beam(_))));
    let uid = cc_ifc::guid::encode(p.element(ElementId(host)).unwrap().uid.0);
    assert!(!text.contains(&uid), "the building itself is not exported");
}
```

- [ ] **Step 2: Run them.** `cargo nextest run -p cc_views steel_buildings_draw` and `cargo nextest run -p cc_engine --test main ifc_export_writes_the_members`. Expected: the plan test FAILS (0 lines). The IFC test may already pass (export has no arm for the host, like masses); keep it as the regression test.

If the IFC counts differ, check whether export writes cold-formed purlins as `IFCBEAM` or as members (`IFCMEMBER`); count what `cc_ifc` writes for a `Beam` element of a cold-formed type and adapt the expected count, keeping the host check.

- [ ] **Step 3: Draw the footprint** in `plan.rs`:

```rust
/// Steel buildings of a plan's level: their column lines' rectangle, thin
/// and dashed, as the building's own lines so it can be picked (its
/// columns and beams draw themselves).
fn draw_steel_buildings(d: &mut Drawing2D, project: &Project, level: LevelId) {
    for e in project.elements.values().filter(|e| e.level == level) {
        let ElementKind::SteelBuilding(b) = &e.kind else { continue };
        d.polyline(&b.footprint(), true, LineStyle::Hidden, Some(e.id));
    }
}
```

In `plan_of`, after `draw_shafts(&mut d, project, level, cut_z);`:

```rust
    if !ceiling {
        draw_steel_buildings(&mut d, project, level);
    }
```

In `is_structural`, add `| ElementKind::SteelBuilding(_)` to the arm that returns `true`.

- [ ] **Step 4: Run the tests.** `cargo nextest run -p cc_views plan` and `cargo nextest run -p cc_engine --test main steel_building`. Expected: pass.

- [ ] **Step 5: Clippy, fmt, commit.** `cargo clippy -p cc_views -p cc_engine --all-targets -- -D warnings`, `cargo fmt --all`, `git add -A && git commit -s -m "Draw steel buildings in plans and leave them out of IFC"`.

---

### Task 5: Steel Building dialog and properties (cc_app)

**Files:**
- Create: `crates/cc_app/src/steel_building_ui.rs`
- Modify:
  - `crates/cc_app/src/main.rs`: `mod steel_building_ui;` (alphabetical).
  - `crates/cc_app/src/state.rs`: `Dialogs::steel_building: Option<crate::steel_building_ui::SteelBuildingDialog>` (default `None`), after `grid_frame`.
  - `crates/cc_app/src/dialogs.rs`: `crate::steel_building_ui::show(ctx, state);` after the grid frame's.
  - `crates/cc_app/src/grid_frame_ui.rs`: make `combo` `pub(crate)` (the new file reuses it).
  - `crates/cc_app/src/commands.rs`: `UiCommand::SteelBuilding`:
    - in the enum after `GridFrame`;
    - in `all()` after `GridFrame`;
    - icon `Icon::SteelBuilding`; label `action-steel-building`; description `cmd-desc-steel-building`; engine name `"steel_building_create"`;
    - `run`: `SteelBuilding => crate::steel_building_ui::open(state),`.
  - `crates/cc_app/src/ribbon.rs` steel framing group: `&[&[ColumnsAtGrids, BeamsOnGrids, BeamsBetweenColumns], &[GridFrame, Truss, SteelBuilding]]`.
  - `crates/cc_app/src/properties.rs`: `ElementKind::SteelBuilding(b) => crate::steel_building_ui::props(ui, state, id, b),` next to the `LevelMark` arm.
  - `assets/i18n/*.ftl`: `action-steel-building`, `cmd-desc-steel-building`, `dlg-steel-building`, `section-steel-building`, `section-secondary-members`, `prop-span`, `prop-eave-height`, `prop-bays`, `prop-bay-spacing`, `label-rafter-type`, `prop-purlins`, `prop-girts`, `prop-members`, `prop-detached-members`, `action-reset-members`, `opt-catalog-default`, `msg-steel-building-created`.
- Test: `crates/cc_app/src/ui_tests.rs`.

**Interfaces:**
- Consumes: `SteelBuildingCreate`, `SecondaryMembers` (Task 3); `cc_core::steel_building::*` (Task 1); `Project::types_of_material` (`StructuralMaterial::Steel`); `grid_frame_ui::combo`; `engine_edit::element`; `properties::{key, length_drag}`; `table::{row, section, readonly_row, checkbox}`.
- Produces:
  - `SteelBuildingDialog` (fields below), `open(&mut AppState)`, `show(&Context, &mut AppState)`.
  - `pub fn edit(state: &mut AppState, id: ElementId, field: &str, f: impl FnOnce(&mut SteelBuilding))`.
  - `pub fn props(ui: &mut Ui, state: &mut AppState, id: ElementId, building: &SteelBuilding)`.

- [ ] **Step 1: Write the failing UI tests** in `ui_tests.rs`:

```rust
/// *Structure → Steel Building* (steel) opens its dialog; Cancel changes
/// nothing, OK creates the building and its members in one undo step.
#[test]
fn steel_building_dialog_creates_a_building() {
    let mut state = sample_state();
    state.ribbon_tab = RibbonTab::Structure;
    state.prefs.structure_material = cc_core::StructuralMaterial::Steel;
    let depth = state.session.doc.undo_depth();
    let count = |h: &Harness<'static, AppState>, c| h.state().project().elements_of(c).count();
    let mut harness = app_harness(state);
    let columns = count(&harness, cc_core::Category::Column);
    harness.get_by_role_and_label(Role::Button, &t!("action-steel-building")).click();
    harness.run();
    assert!(harness.state().dialogs.steel_building.is_some());
    harness.get_by_label(&t!("action-cancel")).click();
    harness.run();
    assert!(harness.state().dialogs.steel_building.is_none());
    assert_eq!(harness.state().session.doc.undo_depth(), depth, "Cancel changes nothing");
    harness.get_by_role_and_label(Role::Button, &t!("action-steel-building")).click();
    harness.run();
    harness.get_by_label(&t!("action-ok")).click();
    harness.run();
    assert!(harness.state().dialogs.steel_building.is_none());
    assert_eq!(count(&harness, cc_core::Category::SteelBuilding), 1);
    assert_eq!(count(&harness, cc_core::Category::Column), columns + 12, "two columns on each of six frames");
    assert_eq!(harness.state().session.doc.undo_depth(), depth + 1);
}

/// A steel building's rows edit it through `element_set`: a new span moves
/// its columns (same ids), and *Reset Members* brings back a deleted one.
#[test]
fn steel_building_properties_regenerate_it() {
    let mut state = sample_state();
    let args = serde_json::json!({"origin": [100, 100], "purlins": null, "girts": null});
    let out = state.session.execute("steel_building_create", args).unwrap();
    let host = cc_core::ElementId(out["id"].as_u64().unwrap());
    let members = |s: &AppState| match &s.project().element(host).unwrap().kind {
        cc_core::ElementKind::SteelBuilding(b) => (b.members.clone(), b.detached.len()),
        _ => panic!("not a steel building"),
    };
    let (first, _) = members(&state);
    // The path the palette's rows take.
    crate::steel_building_ui::edit(&mut state, host, "span", |b| b.span = 24.0);
    let (after, _) = members(&state);
    let ids = |m: &[cc_core::steel_building::FrameMember]| m.iter().map(|m| m.id).collect::<Vec<_>>();
    assert_eq!(ids(&after), ids(&first), "same members");
    let right = after[1].id;
    let cc_core::ElementKind::Column(c) = &state.project().element(right).unwrap().kind else { panic!("a column") };
    assert!((c.position.x - 124.0).abs() < 1e-9);
    state.session.execute("elements_delete", serde_json::json!({"ids": [right.0]})).unwrap();
    assert_eq!(members(&state).1, 1, "the deleted column is detached");
    state.selection = [host].into_iter().collect();
    let mut harness = themed_harness(state, egui::vec2(360.0, 1600.0), egui::Theme::Light, |ui, state| {
        crate::properties::show(ui, state);
    });
    harness.run();
    assert!(harness.query_by_label(&t!("prop-span")).is_some());
    harness.get_by_label(&t!("action-reset-members")).click();
    harness.run();
    let (members_now, detached) = members(harness.state());
    assert_eq!((members_now.len(), detached), (first.len(), 0));
}
```

- [ ] **Step 2: Run them to see them fail.** `cargo nextest run -p cc_app steel_building`. Expected: compile error (`steel_building_ui` and `UiCommand::SteelBuilding` are missing).

- [ ] **Step 3: Write `steel_building_ui.rs`:**

```rust
//! *Structure → Steel Building*: the dialog that runs
//! `steel_building_create` on the active level, and the properties of a
//! selected building (every parameter, through `element_set`, which
//! regenerates its members in the same undo step).

use crate::dialog::Dialog;
use crate::grid_frame_ui::combo;
use crate::state::AppState;
use crate::t;
use crate::table::{readonly_row, row, section};
use crate::theme::PALETTE;
use cc_core::steel_building::{self as sb, FrameRoof, Secondary, SteelBuilding};
use cc_core::{Category, ElementId, ElementKind, ElementTypeId, ProfileMaterial, StructuralMaterial};
use cc_engine::Ref;
use cc_engine::commands::steel_building::{SecondaryMembers, SteelBuildingCreate};
use egui::{Context, DragValue, RichText, Ui};

#[derive(Debug, Clone)]
pub struct SteelBuildingDialog {
    pub span: f64,
    pub eave_height: f64,
    pub slope_percent: f64,
    pub roof: FrameRoof,
    pub bays: u32,
    pub bay_spacing: f64,
    pub origin: [f64; 2],
    pub rotation: f64,
    /// `None`: the catalog default.
    pub column_type: Option<ElementTypeId>,
    pub rafter_type: Option<ElementTypeId>,
    pub purlins: bool,
    pub purlin_type: Option<ElementTypeId>,
    pub purlin_spacing: f64,
    pub girts: bool,
    pub girt_type: Option<ElementTypeId>,
    pub girt_spacing: f64,
    pub error: Option<String>,
}

impl Default for SteelBuildingDialog {
    fn default() -> Self {
        Self {
            span: sb::DEFAULT_SPAN,
            eave_height: sb::DEFAULT_EAVE_HEIGHT,
            slope_percent: sb::DEFAULT_SLOPE,
            roof: FrameRoof::Gable,
            bays: sb::DEFAULT_BAYS,
            bay_spacing: sb::DEFAULT_BAY_SPACING,
            origin: [0.0, 0.0],
            rotation: 0.0,
            column_type: None,
            rafter_type: None,
            purlins: true,
            purlin_type: None,
            purlin_spacing: sb::DEFAULT_SECONDARY_SPACING,
            girts: true,
            girt_type: None,
            girt_spacing: sb::DEFAULT_SECONDARY_SPACING,
            error: None,
        }
    }
}

pub fn open(state: &mut AppState) {
    state.dialogs.steel_building = Some(SteelBuildingDialog::default());
}

/// The roof choices.
fn roofs() -> [(FrameRoof, String); 2] {
    [(FrameRoof::Gable, t!("roof-gable")), (FrameRoof::Mono, t!("roof-shed"))]
}

/// The catalog default, then the steel types of `category` (cold-formed included).
fn steel_choices(state: &AppState, category: Category) -> Vec<(Option<ElementTypeId>, String)> {
    let p = state.project();
    let mut out = vec![(None, t!("opt-catalog-default"))];
    out.extend(p.types_of_material(category, StructuralMaterial::Steel).map(|t| (Some(t.id), t.name.clone())));
    out
}

fn finished(r: &egui::Response) -> bool {
    r.drag_stopped() || r.lost_focus()
}

pub fn show(ctx: &Context, state: &mut AppState) {
    let Some(mut d) = state.dialogs.steel_building.clone() else { return };
    let columns = steel_choices(state, Category::Column);
    let beams = steel_choices(state, Category::Beam);
    let roofs = roofs();
    let mut ok = None;
    let response = Dialog::new("steel-building", t!("dlg-steel-building")).width(460.0).show(
        ctx,
        |ui| {
            let label = |ui: &mut Ui, k: String| {
                ui.label(RichText::new(k).color(PALETTE.text_muted));
            };
            egui::Grid::new("steel-building-fields").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
                label(ui, t!("prop-span"));
                ui.add(crate::properties::length_drag(&mut d.span).range(sb::SPAN));
                ui.end_row();
                label(ui, t!("prop-eave-height"));
                ui.add(crate::properties::length_drag(&mut d.eave_height).range(sb::EAVE_HEIGHT));
                ui.end_row();
                label(ui, t!("prop-slope"));
                ui.add(DragValue::new(&mut d.slope_percent).range(sb::SLOPE).speed(0.1).suffix(" %"));
                ui.end_row();
                label(ui, t!("prop-roof-shape"));
                combo(ui, "sb-roof", &mut d.roof, &roofs);
                ui.end_row();
                label(ui, t!("prop-bays"));
                ui.add(DragValue::new(&mut d.bays).range(sb::BAYS));
                ui.end_row();
                label(ui, t!("prop-bay-spacing"));
                ui.add(crate::properties::length_drag(&mut d.bay_spacing).range(sb::BAY_SPACING));
                ui.end_row();
                label(ui, t!("fp-origin-x"));
                ui.add(crate::properties::length_drag(&mut d.origin[0]));
                ui.end_row();
                label(ui, t!("fp-origin-y"));
                ui.add(crate::properties::length_drag(&mut d.origin[1]));
                ui.end_row();
                label(ui, t!("prop-rotation"));
                ui.add(DragValue::new(&mut d.rotation).speed(1.0).suffix("°"));
                ui.end_row();
                label(ui, t!("label-column-type"));
                combo(ui, "sb-column", &mut d.column_type, &columns);
                ui.end_row();
                label(ui, t!("label-rafter-type"));
                combo(ui, "sb-rafter", &mut d.rafter_type, &beams);
                ui.end_row();
            });
            for (salt, name, on, type_id, spacing) in [
                ("sb-purlins", t!("prop-purlins"), &mut d.purlins, &mut d.purlin_type, &mut d.purlin_spacing),
                ("sb-girts", t!("prop-girts"), &mut d.girts, &mut d.girt_type, &mut d.girt_spacing),
            ] {
                ui.add_space(4.0);
                ui.checkbox(on, name);
                ui.add_enabled_ui(*on, |ui| {
                    egui::Grid::new(salt).num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
                        label(ui, t!("prop-type"));
                        combo(ui, &format!("{salt}-type"), type_id, &beams);
                        ui.end_row();
                        label(ui, t!("label-spacing"));
                        ui.add(crate::properties::length_drag(spacing).range(sb::SPACING));
                        ui.end_row();
                    });
                });
            }
            if let Some(e) = &d.error {
                ui.add_space(6.0);
                ui.label(RichText::new(e).color(PALETTE.danger));
            }
        },
        |f| {
            ok = Some(f.primary(t!("action-ok")));
            f.secondary(t!("action-cancel"));
        },
    );
    if response.clicked.is_some() && response.clicked == ok {
        let line = |on: bool, type_id: Option<ElementTypeId>, spacing: f64| {
            Some(on.then(|| SecondaryMembers { type_ref: type_id.map(|t| Ref::Id(t.0)), spacing: Some(spacing) }))
        };
        let command = SteelBuildingCreate {
            level: state.active_level().map(|l| Ref::Id(l.0)),
            origin: Some(d.origin),
            rotation: Some(d.rotation),
            span: Some(d.span),
            eave_height: Some(d.eave_height),
            slope_percent: Some(d.slope_percent),
            roof: Some(d.roof.code().into()),
            bays: Some(d.bays),
            bay_spacing: Some(d.bay_spacing),
            column_type: d.column_type.map(|t| Ref::Id(t.0)),
            rafter_type: d.rafter_type.map(|t| Ref::Id(t.0)),
            purlins: line(d.purlins, d.purlin_type, d.purlin_spacing),
            girts: line(d.girts, d.girt_type, d.girt_spacing),
        };
        match state.session.run(command) {
            Ok(out) => {
                let count = out["members"].as_array().map_or(0, Vec::len);
                state.message = t!("msg-steel-building-created", "count" => count);
                if let Some(id) = out["id"].as_u64() {
                    state.selection = [ElementId(id)].into_iter().collect();
                }
                state.dialogs.steel_building = None;
            }
            Err(e) => {
                d.error = Some(e.message);
                state.dialogs.steel_building = Some(d);
            }
        }
        return;
    }
    if response.clicked.is_some() || response.dismissed {
        state.dialogs.steel_building = None;
    } else {
        state.dialogs.steel_building = Some(d);
    }
}

/// Change building `id` as `f` changes a copy of it (`element_set`, which
/// regenerates its members); edits under one `field` merge while dragging.
pub fn edit(state: &mut AppState, id: ElementId, field: &str, f: impl FnOnce(&mut SteelBuilding)) {
    crate::engine_edit::element(state, id, Some(crate::properties::key((id, field))), |e| {
        if let ElementKind::SteelBuilding(b) = &mut e.kind {
            f(b);
        }
    });
}

/// The beam type purlins and girts get when turned on in the palette: the
/// first cold-formed one, else the rafters'.
fn secondary_default(state: &AppState, rafter: ElementTypeId) -> ElementTypeId {
    let p = state.project();
    p.types_of(Category::Beam)
        .find(|t| p.type_profile_def(&t.data).is_some_and(|d| d.material == ProfileMaterial::ColdFormed))
        .map_or(rafter, |t| t.id)
}

/// The rows of a selected steel building.
pub fn props(ui: &mut Ui, state: &mut AppState, id: ElementId, building: &SteelBuilding) {
    let mut b = building.clone();
    let mut done = false;
    let all = |c: Category| -> Vec<(ElementTypeId, String)> {
        state.project().types_of(c).map(|t| (t.id, t.name.clone())).collect()
    };
    let (columns, beams) = (all(Category::Column), all(Category::Beam));
    let fallback = secondary_default(state, b.rafter_type);
    let roofs = roofs();
    let length = |ui: &mut Ui, label: &str, v: &mut f64, range: std::ops::RangeInclusive<f64>| {
        finished(&row(ui, label, |ui| ui.add(crate::properties::length_drag(v).range(range))))
    };
    section(ui, "steel-building", t!("section-steel-building"), |ui| {
        done |= length(ui, &t!("prop-span"), &mut b.span, sb::SPAN);
        done |= length(ui, &t!("prop-eave-height"), &mut b.eave_height, sb::EAVE_HEIGHT);
        done |= finished(&row(ui, &t!("prop-slope"), |ui| {
            ui.add(DragValue::new(&mut b.slope_percent).range(sb::SLOPE).speed(0.1).suffix(" %"))
        }));
        let roof = b.roof;
        row(ui, &t!("prop-roof-shape"), |ui| combo(ui, &format!("sb-roof-{}", id.0), &mut b.roof, &roofs));
        done |= b.roof != roof;
        done |= finished(&row(ui, &t!("prop-bays"), |ui| ui.add(DragValue::new(&mut b.bays).range(sb::BAYS))));
        done |= length(ui, &t!("prop-bay-spacing"), &mut b.bay_spacing, sb::BAY_SPACING);
        done |= length(ui, &t!("fp-origin-x"), &mut b.origin.x, -1e6..=1e6);
        done |= length(ui, &t!("fp-origin-y"), &mut b.origin.y, -1e6..=1e6);
        done |= finished(&row(ui, &t!("prop-rotation"), |ui| {
            ui.add(DragValue::new(&mut b.rotation_deg).speed(1.0).suffix("°"))
        }));
        readonly_row(ui, &t!("prop-members"), building.members.len().to_string());
        readonly_row(ui, &t!("prop-detached-members"), building.detached.len().to_string());
        let reset = ui.add_enabled(!building.detached.is_empty(), egui::Button::new(t!("action-reset-members")));
        if reset.clicked() {
            b.detached.clear();
            done = true;
        }
    });
    section(ui, "steel-building-types", t!("section-type"), |ui| {
        let before = (b.column_type, b.rafter_type);
        row(ui, &t!("label-column-type"), |ui| {
            combo(ui, &format!("sb-column-{}", id.0), &mut b.column_type, &columns)
        });
        row(ui, &t!("label-rafter-type"), |ui| combo(ui, &format!("sb-rafter-{}", id.0), &mut b.rafter_type, &beams));
        done |= (b.column_type, b.rafter_type) != before;
    });
    section(ui, "steel-building-secondary", t!("section-secondary-members"), |ui| {
        for (salt, name, slot) in [("purlins", t!("prop-purlins"), &mut b.purlins), ("girts", t!("prop-girts"), &mut b.girts)]
        {
            let mut on = slot.is_some();
            if row(ui, &name, |ui| crate::table::checkbox(ui, &mut on)).changed() {
                *slot = on.then_some(Secondary { type_id: fallback, spacing: sb::DEFAULT_SECONDARY_SPACING });
                done = true;
            }
            if let Some(s) = slot {
                let type_id = s.type_id;
                row(ui, &t!("prop-type"), |ui| combo(ui, &format!("sb-{salt}-type-{}", id.0), &mut s.type_id, &beams));
                done |= s.type_id != type_id;
                done |= length(ui, &t!("label-spacing"), &mut s.spacing, sb::SPACING);
            }
        }
    });
    if b != *building {
        edit(state, id, "steel-building", move |x| *x = b);
    }
    if done {
        state.session.doc.break_merge();
    }
}
```

Adapt to the real API:
- `PALETTE.danger` is what `grid_frame_ui.rs` uses; keep it.
- `DragValue::range` takes a `RangeInclusive` of the value's type; `sb::SPAN` is a `const`, so passing it moves a copy. If `length_drag(..).range(..)` rejects `RangeInclusive<f64>`, pass `*sb::SPAN.start()..=*sb::SPAN.end()`.
- `all` borrows `state` immutably; it must be dead before `edit(state, ..)`. The vectors it returns are owned, so it is.
- `cc_core::ProfileMaterial` is the profile material enum (`ColdFormed`); if it is only at `cc_core::structural_profile::ProfileMaterial`, import from there.
- `state.selection`'s collection type: `.collect()` builds it whatever it is (the mass test does the same).

- [ ] **Step 4: Wire the command, the ribbon, the palette and the dialog** as listed under *Files*. In `commands.rs`, every per-command `match` gets its `SteelBuilding` arm (the compiler lists them): icon, label, description, engine name, `run`. No shortcut, no availability rule (a project always has a level). Add the 17 keys to the 12 `.ftl` files.

- [ ] **Step 5: Run the tests.** `cargo check --tests -p cc_app`, then `cargo nextest run -p cc_app steel_building i18n icons ribbon`. Expected: pass.

If the dialog's OK is not found because the window is taller than the harness (760 px), reduce the row spacing or put origin and rotation on one row; do not grow the harness.

- [ ] **Step 6: Clippy, fmt, commit.** `cargo clippy -p cc_app --all-targets -- -D warnings`, `cargo fmt --all`, `git add -A && git commit -s -m "Add the Steel Building dialog and properties"`.

---

### Task 6: Documentation and full check

**Files:**
- Modify:
  - `AGENTS.md`: a **Steel buildings** bullet after **Structure by material**: `cc_core::steel_building` (`SteelBuilding`, `MemberRole`/`RoleKind`, `FrameRoof`, bounds and defaults, `layout`, `MAX_MEMBERS`; members are ordinary columns and beams; `EditCtx::finish` → `sync_steel_buildings` right after `sync_profiles`, from `element_changes`/`changed_types`: deleted members and members whose kind, type or level changed with the building unchanged are detached, changed or added buildings and buildings whose types changed regenerate in place keeping ids, a copy gets its own; `remove_element` takes the members along, `copy_set` leaves them out, `repair_steel_buildings`); `cc_engine::commands::steel_building` (`steel_building_create`, `FRAME_DEFAULT`/`SECONDARY_DEFAULT` through `structure_material::catalog_type`, `element_get` `derived.members`/`detached`, `element_set` refuses changed `members`); plans draw the footprint dashed (`plan::draw_steel_buildings`), IFC leaves the host out; the app's `steel_building_ui.rs` (dialog, palette rows, *Reset Members*).
  - `docs/interface.md`, `docs/interface.pt-BR.md`: in the Structure tab paragraph, add *Steel Building* to the steel *Framing* group, then a paragraph on the dialog's fields, the palette rows, detaching and *Reset Members*.
  - `docs/automation.md`, `docs/automacao.md`: after the `grid_frame_create` paragraph, a short paragraph and the example `{"command": "steel_building_create", "args": {"span": 24, "eave_height": 7, "bays": 8, "roof": "mono", "girts": null}}`, plus `{"command": "element_set", "args": {"id": 42, "params": {"span": 30}}}` (prose in English or Portuguese, matching each file).
  - `CHANGELOG.md`: one entry under `## [Unreleased]` → `### Added`: the parametric steel building (portal frames, purlins and girts as ordinary columns and beams, regenerated with their ids kept, detached when edited by hand; `steel_building_create`; *Structure → Framing → Steel Building* and its properties).
  - `docs/superpowers/specs/2026-11-12-steel-building-design.md` and this plan as `docs/superpowers/plans/2026-11-12-steel-building.md`, if not committed yet.

- [ ] **Step 1: Write the docs** above, concise and in the style of the neighboring entries.
- [ ] **Step 2: Full check.**
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo nextest run --workspace`
  - `cargo test --workspace --doc`

  Expected: all green. `docs/commands.md` must match `cargo run -q -p cc_cli -- reference` (a test checks it).
- [ ] **Step 3: Commit.** `git commit -s -m "Document the parametric steel building"`.

---

## Self-review

- Spec coverage:
  - Model and fields with bounds: Task 1.
  - Layout of columns, rafters, purlins and girts, `MAX_MEMBERS`, missing sections: Task 1.
  - Sync (detach, regenerate in place, copies), delete with host, undo, repair: Task 2.
  - `steel_building_create`, catalog defaults through `catalog_type`, one undo step, refusals, `element_get`/`element_set`: Task 3.
  - Plan footprint, nothing in 3D or sections (no geometry is built for the host), IFC without the host: Tasks 1 and 4.
  - App button, dialog, palette, *Reset Members*, i18n: Task 5.
  - Tests listed in the spec: core (Tasks 1–2), engine (Tasks 3–4), app (Task 5).
- Names used across tasks: `SteelBuilding`, `FrameRoof`, `RoleKind`, `MemberRole`, `FrameMember`, `Secondary`, `section_box`, `host_type`, `steel_members_of`, `element_changes`, `changed_types`, `sync_steel_buildings`, `repair_steel_buildings`, `catalog_type`, `SteelBuildingCreate`, `SecondaryMembers`, `FRAME_DEFAULT`, `SECONDARY_DEFAULT`, `steel_building_ui::{open, show, edit, props}`.
