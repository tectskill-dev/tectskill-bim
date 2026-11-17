# Parametric Truss Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the steel building's member-host mechanism into a shared `MemberHost` abstraction and use it for a new parametric `Truss` host element (flat, mono or gable; Pratt, Howe, Warren, king post, queen post or Fink; copies at a spacing) whose chords, diagonals and posts are ordinary beams and columns, created by a rewritten `truss_create` with steel or timber catalog defaults, drawn as dashed span lines in plans and edited from the Structure tab's Truss tool and the properties palette.

**Architecture:**
- `cc_core::member_host` holds the `MemberHost` trait (role keys as `RoleKey(u64)`), the dispatch `ElementKind::member_host()/member_host_mut()`, the generic sync run by `EditCtx::finish` (`sync_member_hosts`, the former `sync_steel_buildings`), the generic repair (`repair_member_hosts`), `Project::host_members_of` (used by `remove_element` and `copy_set`) and the shared helpers `within`, `reset`, `fix_type`. `SteelBuilding` implements it with no change in behavior.
- `cc_core::truss` holds the `Truss` model, its bounds and defaults, the web per pattern and the deterministic `layout`; `Truss` implements `MemberHost`.
- `cc_engine::commands::framing::TrussCreate` (same name, old parameters kept, new ones added) creates a `Truss` host through `structure_material::catalog_type`; `element_get`/`element_set` go through the shared host checks.
- `cc_views` draws each copy's span line dashed; IFC export leaves the host out.
- `cc_app`: `Tool::Truss { material }` with new options, the Truss button in the steel and timber framing groups, `truss_ui.rs` (palette rows) and `member_host_ui.rs` (the members/detached rows and *Reset Members* shared with `steel_building_ui.rs`).

**Tech Stack:** Rust 2024 workspace. egui/eframe app with egui_kittest UI tests. Fluent i18n in 12 languages. cargo-nextest.

**Spec:** `docs/superpowers/specs/2026-11-13-parametric-truss-design.md`

## Global Constraints

- Work only in the worktree `D:\constructcad-truss` (branch `feat/parametric-truss`, stacked on `feat/steel-building`). Never touch `D:\constructcad` or `D:\constructcad-steel`. Before Task 1, once `feat/steel-building` has all its tasks committed: `git -C D:\constructcad-steel worktree add D:\constructcad-truss -b feat/parametric-truss feat/steel-building`, then copy the spec to `docs/superpowers/specs/2026-11-13-parametric-truss-design.md` and this plan to `docs/superpowers/plans/2026-11-13-parametric-truss.md` in the new worktree (committed with Task 1).
- Never push, never run `gh`, never merge. Commit with `git commit -s`. No Claude attribution or Co-Authored-By.
- Non-test code never panics: no `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!` or `unimplemented!` (clippy denies them). No `unsafe`.
- Every new interface or transaction text is an i18n key added to all 12 files in `assets/i18n/`: `pt-BR`, `en`, `es`, `fr`, `de`, `it`, `lo`, `pl`, `uk-UA`, `zh-TW`, `zh-CN`, `ja`. Tests fail otherwise (missing keys, or a language with different keys or placeholders).
- Every engine command parameter has a doc comment. `truss_create` stays registered in `crates/cc_engine/src/registry.rs` (`framing::TrussCreate`). Regenerate the reference with `cargo run -p cc_cli -- reference > docs/commands.md`; a test checks it.
- Integration tests are one binary: a new file in `crates/cc_engine/tests/` needs `mod <name>;` in `crates/cc_engine/tests/main.rs`.
- Tests run with cargo-nextest: `cargo nextest run -p <crate> <filter>` (`--test main` for engine integration tests). Use `cargo check --tests -p <crate>` for type errors. Doctests: `cargo test --workspace --doc`.
- Formatting is rustfmt with `max_width = 120`: `cargo fmt --all`. Clippy: `cargo clippy --workspace --all-targets -- -D warnings`.
- Code, comments and identifiers are in English.
- Names are new and do not clash: `Category::Truss`, `TypeData::Truss {}`, `ElementKind::Truss(Box<Truss>)`, module `cc_core::truss`, `TrussShape`, `TrussPattern`, `TrussRoleKind`, `TrussRole`, `TrussMember`. (`cc_geom::roof_framing` has `MemberKind::Truss` and `TrussPart`, which are unrelated and untouched; `Icon::Truss` already exists and is reused.)
- Values (verbatim from the spec, plus the decisions listed below):
  - Shapes `Flat`, `Mono`, `Gable`; patterns `Pratt`, `Howe`, `Warren`, `KingPost`, `QueenPost`, `Fink`; codes `flat`, `mono`, `gable`, `pratt`, `howe`, `warren`, `kingpost`, `queenpost`, `fink`.
  - `panels` 2..=64; the default from the span, about 1.5 m each; a `Gable` needs an even count (refused otherwise).
  - `copies` 1..=200, default 1; `spacing` 0.3..=20 m, to the left of start→end.
  - `MAX_MEMBERS = 4000`; a vertical shorter than 0.05 m is skipped (`MIN_VERTICAL`).
  - Top chord height above the bottom chord at x: `Flat` h; `Mono` h·x/L; `Gable` h·(1 − |2x/L − 1|).
  - Material defaults: steel chords `Ue 150x60x20x2,00`, web `Ue 100x50x17x2,00`, posts a column type of `Ue 100x50x17x2,00` (series `NBR6355-Ue`); timber chords `Peça 6x16`, web `Peça 6x12`, posts a column type of `Peça 6x12` (series `Timber-Sawn`); concrete is refused.
  - Out-of-range values are refused by commands, not clamped; repair resets them to the defaults.
- Decisions this plan makes where the spec is open (each also listed at the end):
  - `RoleKey(u64)` (kind, group, index packed) is the trait's role currency, so each host keeps its own typed role (`MemberRole`, `TrussRole`) and file format; key order equals the typed roles' `Ord`.
  - `height` 0.1..=20 m (default 1, as the old tool); span length 0.5..=200 m; `DEFAULT_SPACING = 1.5`; `base_offset` any finite number.
  - Pratt and Howe verticals at the inner panel points only (n − 1, as the spec's count); a `Mono` truss also gets the end post at its high end (`index = panels`) for every pattern; a diagonal whose two ends differ in height by less than 0.05 m (it would lie on the bottom chord) is skipped.
  - `pattern_used()`: KingPost/QueenPost/Fink fall back to Pratt unless the shape is `Gable` and `panels` ≥ 2 / 4 / 6 respectively.
  - King post: quarter index `q = round(n/4)` clamped to `1..=n/2−1` (no diagonals when `n = 2`). Queen post: verticals at `t = round(n/3)` clamped likewise and `n − t`, diagonals from their feet to the apex. Fink: `q = max(1, n/4)`, `t = max(ceil(n/3), q + 1)`, diagonals from bottom `t` to top `q` and to the apex, mirrored.
  - Chords and diagonals are beams with `BeamVertical::Center` (their axes through the panel points, as the old truss); verticals are columns turned to the span direction.
  - Diagonal `index` is its position in the pattern's list (stable whether or not it is skipped); chord index = panel; vertical index = panel point.
  - A mirror maps the whole set of copies: the new start/end are the mirrored last copy's, so the copies stay on the mirrored side and a mono truss keeps its high end.
  - `truss_create`: `material` defaults to `steel`; `height` stays required; `bottom_offset` keeps its name and default (level height − height) and sets `base_offset`; `web_type` and `post_type` now default to the material's catalog rows (the old "first steel profile column type" heuristic is removed); `chord_type` sets both chords, `top_chord_type`/`bottom_chord_type` win over it.
  - The palette shows parameters, not `start`/`end` (moved with Move/Rotate/Mirror); a dragged odd panel count on a gable steps to the next even one in the drag's direction.

## i18n keys

Each task adds the keys it uses to all 12 files, appended under a comment `# Parametric trusses`. New keys:

| key | pt-BR | en | es | fr | de | it | lo | pl | uk-UA | zh-TW | zh-CN | ja |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| category-truss | Treliças | Trusses | Cerchas | Fermes | Fachwerkträger | Travature reticolari | ໂຄງຖັກ | Kratownice | Ферми | 桁架 | 桁架 | トラス |
| tpl-type-truss | Treliça | Truss | Cercha | Ferme | Fachwerkträger | Travatura reticolare | ໂຄງຖັກ | Kratownica | Ферма | 桁架 | 桁架 | トラス |
| truss-king-post | Pendural | King post | Pendolón | Poinçon | Einfaches Hängewerk | Monaco | ເສົາກາງດຽວ | Jednowieszakowa | З одним стояком | 單柱式 | 单柱式 | キングポスト |
| truss-queen-post | Dois pendurais | Queen post | Doble pendolón | Double poinçon | Doppeltes Hängewerk | Due monaci | ເສົາຄູ່ | Dwuwieszakowa | З двома стояками | 雙柱式 | 双柱式 | クイーンポスト |
| truss-fink | Fink (W) | Fink (W) | Fink (W) | Fink (W) | Fink (W) | Fink (W) | Fink (W) | Fink (W) | Fink (W) | 芬克式 (W) | 芬克式 (W) | フィンク (W) |
| prop-truss-pattern | Padrão | Pattern | Patrón | Treillis | Ausfachung | Schema | ຮູບແບບ | Układ krzyżulców | Схема ґратки | 腹桿形式 | 腹杆形式 | ラチス形式 |
| prop-truss-copies | Cópias | Copies | Copias | Copies | Kopien | Copie | ສຳເນົາ | Kopie | Копії | 副本數 | 副本数 | コピー数 |
| label-top-chord-type | Tipo do banzo superior | Top chord type | Tipo de cordón superior | Type de membrure supérieure | Obergurttyp | Tipo di corrente superiore | ປະເພດຄານເທິງ | Typ pasa górnego | Тип верхнього пояса | 上弦桿類型 | 上弦杆类型 | 上弦材タイプ |
| label-bottom-chord-type | Tipo do banzo inferior | Bottom chord type | Tipo de cordón inferior | Type de membrure inférieure | Untergurttyp | Tipo di corrente inferiore | ປະເພດຄານລຸ່ມ | Typ pasa dolnego | Тип нижнього пояса | 下弦桿類型 | 下弦杆类型 | 下弦材タイプ |
| label-web-type | Tipo das diagonais | Diagonal type | Tipo de diagonales | Type de diagonales | Diagonalentyp | Tipo di diagonali | ປະເພດທະແຍງ | Typ krzyżulców | Тип розкосів | 斜桿類型 | 斜杆类型 | 斜材タイプ |
| label-post-type | Tipo dos montantes | Post type | Tipo de montantes | Type de montants | Pfostentyp | Tipo di montanti | ປະເພດເສົາຕັ້ງ | Typ słupków | Тип стояків | 直桿類型 | 竖杆类型 | 束材タイプ |

`msg-truss-material` (no placeholder; name the *Material* group as each file's `group-structure-material` value does):
- pt-BR: Treliças são de aço ou madeira: escolha um deles em Material.
- en: Trusses are steel or timber: pick one under Material.
- es: Las cerchas son de acero o madera: elija uno en Material.
- fr: Les fermes sont en acier ou en bois : choisissez-en un dans Matériau.
- de: Fachwerkträger sind aus Stahl oder Holz: Wählen Sie eines unter Material.
- it: Le travature sono in acciaio o legno: sceglierne uno in Materiale.
- lo: ໂຄງຖັກເປັນເຫຼັກ ຫຼື ໄມ້: ເລືອກໜຶ່ງຢ່າງໃນ ວັດສະດຸ.
- pl: Kratownice są stalowe lub drewniane: wybierz jedno w Materiale.
- uk-UA: Ферми бувають сталеві або дерев'яні: виберіть одне в «Матеріал».
- zh-TW: 桁架為鋼或木：請在「材料」中選擇其一。
- zh-CN: 桁架为钢或木：请在"材料"中选择其一。
- ja: トラスは鉄骨または木造です。「材料」でいずれかを選んでください。

Rewritten key `cmd-desc-truss` (replace the existing line in each file):
- pt-BR: Monta uma treliça paramétrica de aço ou madeira: clique as duas pontas; a barra de opções define forma (plana, uma água, duas águas), padrão (Pratt, Howe, Warren, pendural, dois pendurais, Fink), altura, painéis, cópias e espaçamento. Banzos e diagonais são vigas e montantes são pilares, mantidos em dia com a treliça.
- en: Lay out a parametric steel or timber truss: click its two ends; the options bar sets its shape (flat, mono, gable), pattern (Pratt, Howe, Warren, king post, queen post, Fink), height, panels, copies and spacing. Chords and diagonals are beams and posts are columns, kept in step with the truss.
- es: Traza una cercha paramétrica de acero o madera: haga clic en sus dos extremos; la barra de opciones define la forma (plana, a un agua, a dos aguas), el patrón (Pratt, Howe, Warren, pendolón, doble pendolón, Fink), la altura, los paneles, las copias y la separación. Cordones y diagonales son vigas y montantes son pilares, que siguen a la cercha.
- fr: Trace une ferme paramétrique en acier ou en bois : cliquez ses deux extrémités ; la barre d'options règle sa forme (plate, monopente, deux pentes), son treillis (Pratt, Howe, Warren, poinçon, double poinçon, Fink), sa hauteur, ses panneaux, ses copies et leur entraxe. Membrures et diagonales sont des poutres, montants des poteaux, tenus à jour avec la ferme.
- de: Legt einen parametrischen Fachwerkträger aus Stahl oder Holz an: Klicken Sie seine beiden Enden; die Optionsleiste legt Form (flach, Pult, Sattel), Ausfachung (Pratt, Howe, Warren, einfaches Hängewerk, doppeltes Hängewerk, Fink), Höhe, Felder, Kopien und Abstand fest. Gurte und Diagonalen sind Träger, Pfosten sind Stützen, die dem Fachwerkträger folgen.
- it: Disegna una travatura reticolare parametrica in acciaio o legno: fai clic sui due estremi; la barra delle opzioni ne imposta forma (piana, a una falda, a due falde), schema (Pratt, Howe, Warren, monaco, due monaci, Fink), altezza, pannelli, copie e interasse. Correnti e diagonali sono travi, montanti sono pilastri, aggiornati con la travatura.
- lo: ວາງໂຄງຖັກແບບພາຣາມິເຕີຂອງເຫຼັກ ຫຼື ໄມ້: ຄລິກສອງປາຍ; ແຖບຕົວເລືອກກຳນົດຮູບຮ່າງ (ພຽງ, ຄ້ອຍດຽວ, ຈົ່ວ), ຮູບແບບ (Pratt, Howe, Warren, ເສົາກາງດຽວ, ເສົາຄູ່, Fink), ຄວາມສູງ, ຈຳນວນຊ່ອງ, ສຳເນົາ ແລະ ໄລຍະຫ່າງ. ຄານເທິງລຸ່ມ ແລະ ທະແຍງແມ່ນຄານ, ເສົາຕັ້ງແມ່ນເສົາ, ປ່ຽນຕາມໂຄງຖັກ.
- pl: Tworzy parametryczną kratownicę stalową lub drewnianą: kliknij jej dwa końce; pasek opcji ustala kształt (płaska, jednospadowa, dwuspadowa), układ (Pratt, Howe, Warren, jednowieszakowa, dwuwieszakowa, Fink), wysokość, przęsła, kopie i rozstaw. Pasy i krzyżulce to belki, słupki to słupy, aktualizowane razem z kratownicą.
- uk-UA: Створює параметричну сталеву або дерев'яну ферму: клацніть її два кінці; панель параметрів задає форму (плоска, односхила, двосхила), схему ґратки (Pratt, Howe, Warren, з одним стояком, з двома стояками, Fink), висоту, панелі, копії та крок. Пояси й розкоси — балки, стояки — колони, що оновлюються разом із фермою.
- zh-TW: 配置參數化鋼或木桁架：點選兩端；選項列設定形狀（平行、單坡、雙坡）、腹桿形式（Pratt、Howe、Warren、單柱式、雙柱式、芬克式）、高度、節間、副本數與間距。弦桿與斜桿為梁，直桿為柱，隨桁架更新。
- zh-CN: 布置参数化钢或木桁架：点击两端；选项栏设置形状（平行、单坡、双坡）、腹杆形式（Pratt、Howe、Warren、单柱式、双柱式、芬克式）、高度、节间、副本数和间距。弦杆和斜杆为梁，竖杆为柱，随桁架更新。
- ja: 鉄骨または木造のパラメトリックなトラスを配置します。両端をクリックし、オプションバーで形状（平行、片流れ、切妻）、ラチス形式（Pratt、Howe、Warren、キングポスト、クイーンポスト、フィンク）、高さ、パネル数、コピー数、間隔を設定します。弦材と斜材は梁、束材は柱で、トラスに合わせて更新されます。

Removed key: `truss-default-name` (no longer used after Task 4; remove it from all 12 files once `rg -n truss-default-name crates` finds nothing).

Existing keys reused (do not add): `prop-truss` (section title), `prop-shape`, `roof-flat`, `roof-shed` (mono), `roof-gable`, `truss-pratt`, `truss-howe`, `truss-warren`, `prop-truss-height`, `prop-truss-panels`, `truss-panels-auto`, `hint-truss`, `tool-truss`, `tx-create-truss`, `prop-base-offset`, `prop-length`, `label-spacing`, `section-type`, `prop-members`, `prop-detached-members`, `action-reset-members`.

---

### Task 1: The `MemberHost` abstraction, with the steel building ported (cc_core)

**Files:**
- Create: `crates/cc_core/src/member_host.rs`
- Modify:
  - `crates/cc_core/src/lib.rs`: `pub mod member_host;` right before `pub mod mep;`.
  - `crates/cc_core/src/steel_building.rs`: remove `within`, `reset`, `fix_type`, `sync_steel_buildings`, `regenerate_steel_building`, `repair_steel_buildings`, `Project::steel_members_of`; add `RoleKind::ALL`, `MemberRole::{key, from_key}`, `impl MemberHost for SteelBuilding`. Its `#[cfg(test)] mod tests` stays unchanged.
  - `crates/cc_core/src/document.rs` (~257): `pending.extend(self.project.host_members_of(id));`; (~898) `self.sync_member_hosts();` with the comment "Member hosts (steel buildings, trusses) next: ...".
  - `crates/cc_core/src/transform.rs` (~396): `set.iter().flat_map(|id| self.host_members_of(*id))`, comment "A copied member host makes its own members: its originals are not copied."
  - `crates/cc_core/src/repair.rs` (~430): `repairs += self.repair_member_hosts();` with the comment "Member hosts: numbers, types and members (after invalid elements went)."
  - `crates/cc_engine/src/commands/steel_building.rs` (~178): `s.project().host_members_of(id)`.
  - `crates/cc_engine/src/commands/edit.rs` (~138): replace the `SteelBuilding` arm by the generic host check (Step 5).
  - Run `rg -n "steel_members_of" crates` and switch any other caller (the app may have one) to `host_members_of`.
- Test: `crates/cc_core/src/member_host.rs` (`mod tests`), plus the unchanged `steel_building::tests` and `crates/cc_engine/tests/steel_building.rs`.

**Interfaces:**
- Consumes: `EditCtx::{project, element_changes, changed_types, element_mut, remove_element, alloc_id, add_element}` (in `document.rs`), `Project::{element, element_type, default_type}`, steel `SteelBuilding::{layout, type_ids, check, check_types, reset_out_of_range}`, `MemberRole::fits`.
- Produces (module `cc_core::member_host`):
  - `struct RoleKey(pub u64)` (`Copy + Ord + Hash + Debug`), `RoleKey::new(kind: u8, group: u16, index: u16) -> RoleKey`, `RoleKey::parts(self) -> (u8, u16, u16)`.
  - `trait MemberHost { fn member_types(&self) -> Vec<ElementTypeId>; fn member_layout(&self, project: &Project) -> Vec<(RoleKey, ElementTypeId, ElementKind)>; fn member_list(&self) -> Vec<(RoleKey, ElementId)>; fn set_member_list(&mut self, members: &[(RoleKey, ElementId)]); fn detached_roles(&self) -> Vec<RoleKey>; fn set_detached_roles(&mut self, roles: &[RoleKey]); fn role_fits(&self, role: RoleKey, kind: &ElementKind) -> bool; fn check_parameters(&self, project: &Project) -> Result<(), String>; fn repair_parameters(&mut self, project: &Project) -> Option<usize>; }`.
  - `ElementKind::member_host(&self) -> Option<&dyn MemberHost>`, `ElementKind::member_host_mut(&mut self) -> Option<&mut dyn MemberHost>`, `Element::member_host(&self) -> Option<&dyn MemberHost>`.
  - `Project::host_members_of(&self, host: ElementId) -> Vec<ElementId>`.
  - `EditCtx::sync_member_hosts(&mut self)` (`pub(crate)`), `Project::repair_member_hosts(&mut self) -> usize` (`pub(crate)`).
  - `pub(crate) fn within(name: &str, v: f64, range: &RangeInclusive<f64>) -> Result<(), String>`, `pub(crate) fn reset(v: &mut f64, range: &RangeInclusive<f64>, default: f64) -> usize`, `pub(crate) fn fix_type(p: &Project, slot: &mut ElementTypeId, category: Category) -> Option<usize>`.
  - `steel_building::RoleKind::ALL: [RoleKind; 4]`, `MemberRole::key(self) -> RoleKey`, `MemberRole::from_key(RoleKey) -> Option<MemberRole>`.

- [ ] **Step 1: Write the failing test** at the bottom of the new `crates/cc_core/src/member_host.rs` (start the file with only this test module and `use` lines so it compiles to a failure):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::steel_building::{MemberRole, RoleKind};

    #[test]
    fn role_keys_keep_the_typed_order_and_round_trip() {
        let roles = [
            MemberRole { kind: RoleKind::Column, frame: 0, index: 1 },
            MemberRole { kind: RoleKind::Column, frame: 1, index: 0 },
            MemberRole { kind: RoleKind::Rafter, frame: 0, index: 0 },
            MemberRole { kind: RoleKind::Girt, frame: 3, index: 1002 },
        ];
        for w in roles.windows(2) {
            assert!(w[0] < w[1] && w[0].key() < w[1].key());
        }
        for r in roles {
            assert_eq!(MemberRole::from_key(r.key()), Some(r));
        }
        assert_eq!(RoleKey::new(9, 1, 2).parts(), (9, 1, 2));
        assert_eq!(MemberRole::from_key(RoleKey::new(9, 0, 0)), None, "no such kind");
    }

    #[test]
    fn only_hosts_dispatch() {
        let (doc, host) = crate::steel_building::tests::site();
        let p = &doc.project;
        let h = p.element(host).and_then(|e| e.member_host()).expect("a host");
        assert_eq!(h.member_list().len(), 134);
        assert!(h.detached_roles().is_empty());
        let member = h.member_list()[0].1;
        assert!(p.element(member).and_then(|e| e.member_host()).is_none(), "a column is no host");
        assert_eq!(p.host_members_of(host).len(), 134);
        assert!(p.host_members_of(member).is_empty());
    }
}
```

`steel_building::tests` and its `site()` are `pub(super)`; make the module `pub(crate) mod tests` and `site`/`building` `pub(crate)` so other modules' tests reuse them (a change to test code only).

- [ ] **Step 2: Run it to see it fail.** `cargo nextest run -p cc_core member_host`. Expected: compile errors (`RoleKey`, `key`, `member_host`, `host_members_of` missing).

- [ ] **Step 3: Write the module** (above the tests):

```rust
//! Member hosts: elements with no geometry of their own (steel buildings,
//! trusses) that own ordinary columns and beams laid out from their
//! parameters. Every edit keeps them in step ([`crate::EditCtx`] runs
//! [`crate::EditCtx::sync_member_hosts`]): a changed host rewrites its
//! members in place, keeping their ids while their roles exist; a member
//! the user edited or deleted is detached, and the host leaves its role
//! alone from then on. Each host keeps its own typed roles; the shared code
//! sees them as [`RoleKey`]s, whose order is the typed roles' order.

use crate::{Category, Element, ElementId, ElementKind, ElementTypeId, Project};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;

/// A member's role as the shared code sees it: kind, group (frame, copy)
/// and index packed so that the order is the typed role's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RoleKey(pub u64);

impl RoleKey {
    pub fn new(kind: u8, group: u16, index: u16) -> Self {
        Self((u64::from(kind) << 32) | (u64::from(group) << 16) | u64::from(index))
    }

    pub fn parts(self) -> (u8, u16, u16) {
        (((self.0 >> 32) & 0xff) as u8, ((self.0 >> 16) & 0xffff) as u16, (self.0 & 0xffff) as u16)
    }
}

/// A host of members: what the shared sync and repair need from it.
pub trait MemberHost {
    /// The member types it uses (an edit of one regenerates it).
    fn member_types(&self) -> Vec<ElementTypeId>;
    /// Its members as its parameters lay them out, in a fixed order.
    fn member_layout(&self, project: &Project) -> Vec<(RoleKey, ElementTypeId, ElementKind)>;
    /// The members it manages, by role.
    fn member_list(&self) -> Vec<(RoleKey, ElementId)>;
    /// Replace the managed members (keys it cannot read are left out).
    fn set_member_list(&mut self, members: &[(RoleKey, ElementId)]);
    /// Roles the user took over: not made again.
    fn detached_roles(&self) -> Vec<RoleKey>;
    fn set_detached_roles(&mut self, roles: &[RoleKey]);
    /// Whether an element of `kind` can fill `role`.
    fn role_fits(&self, role: RoleKey, kind: &ElementKind) -> bool;
    /// Whether its numbers are in range and its types of the right categories.
    fn check_parameters(&self, project: &Project) -> Result<(), String>;
    /// From a file: numbers back to their defaults, types to the category's
    /// default; returns the repairs, `None` when the host must go.
    fn repair_parameters(&mut self, project: &Project) -> Option<usize>;
}

impl ElementKind {
    /// The host behind this element, if it is one.
    pub fn member_host(&self) -> Option<&dyn MemberHost> {
        match self {
            ElementKind::SteelBuilding(b) => Some(b.as_ref() as &dyn MemberHost),
            _ => None,
        }
    }

    pub fn member_host_mut(&mut self) -> Option<&mut dyn MemberHost> {
        match self {
            ElementKind::SteelBuilding(b) => Some(b.as_mut() as &mut dyn MemberHost),
            _ => None,
        }
    }
}

impl Element {
    pub fn member_host(&self) -> Option<&dyn MemberHost> {
        self.kind.member_host()
    }
}

impl Project {
    /// The members a host manages (none for other elements).
    pub fn host_members_of(&self, host: ElementId) -> Vec<ElementId> {
        self.element(host)
            .and_then(|e| e.member_host())
            .map(|h| h.member_list().into_iter().map(|(_, id)| id).collect())
            .unwrap_or_default()
    }
}

pub(crate) fn within(name: &str, v: f64, range: &RangeInclusive<f64>) -> Result<(), String> {
    if v.is_finite() && range.contains(&v) {
        Ok(())
    } else {
        Err(format!("{name} must be between {} and {}", range.start(), range.end()))
    }
}

/// Reset `v` to `default` when it is out of `range`; returns 1 when it did.
pub(crate) fn reset(v: &mut f64, range: &RangeInclusive<f64>, default: f64) -> usize {
    if v.is_finite() && range.contains(v) {
        0
    } else {
        *v = default;
        1
    }
}

/// `slot` if it is a type of `category`, else the category's default type
/// (`Some(1)`: one repair); `None` when there is none.
pub(crate) fn fix_type(p: &Project, slot: &mut ElementTypeId, category: Category) -> Option<usize> {
    if p.element_type(*slot).is_some_and(|t| t.category() == category) {
        return Some(0);
    }
    *slot = p.default_type(category)?;
    Some(1)
}

impl crate::EditCtx<'_> {
    /// Keep member hosts and their members in step, from what this edit
    /// itself changed (it runs before the syncs that touch elements):
    /// 1. a member the edit deleted, or whose kind, type or level it changed
    ///    while leaving its host alone, is detached (its role leaves the
    ///    member list for the detached roles; the element stays as the user
    ///    left it);
    /// 2. a host the edit added or changed, or whose member types it
    ///    changed, gets the members of its layout (except detached roles):
    ///    existing ones rewritten in place (same ids), missing ones added,
    ///    those whose role is gone deleted. A host added with members that
    ///    are another host's (a copy) or gone (a paste after a cut) gets its own.
    pub(crate) fn sync_member_hosts(&mut self) {
        let p = self.project();
        if !p.elements.values().any(|e| e.member_host().is_some()) {
            return;
        }
        let types = self.changed_types();
        let mut added: BTreeSet<ElementId> = BTreeSet::new();
        let mut changed: BTreeSet<ElementId> = BTreeSet::new();
        let mut removed: BTreeSet<ElementId> = BTreeSet::new();
        let mut reshaped: BTreeSet<ElementId> = BTreeSet::new();
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
        let mut detach: Vec<(ElementId, Vec<RoleKey>)> = Vec::new();
        let mut regenerate: Vec<ElementId> = Vec::new();
        for e in p.elements.values() {
            let Some(h) = e.member_host() else { continue };
            let host_changed = changed.contains(&e.id) || h.member_types().iter().any(|t| types.contains(t));
            let taken: Vec<RoleKey> = h
                .member_list()
                .into_iter()
                .filter(|(_, id)| removed.contains(id) || (reshaped.contains(id) && !host_changed))
                .map(|(r, _)| r)
                .collect();
            if !taken.is_empty() {
                detach.push((e.id, taken));
            }
            if host_changed {
                regenerate.push(e.id);
            }
        }
        for (host, roles) in detach {
            if let Some(h) = self.element_mut(host).and_then(|e| e.kind.member_host_mut()) {
                let mut list = h.member_list();
                list.retain(|(r, _)| !roles.contains(r));
                h.set_member_list(&list);
                let mut detached = h.detached_roles();
                for r in roles {
                    if !detached.contains(&r) {
                        detached.push(r);
                    }
                }
                h.set_detached_roles(&detached);
            }
        }
        for host in regenerate {
            self.regenerate_members(host, added.contains(&host));
        }
    }

    /// Give host `host` the members of its layout (see
    /// [`Self::sync_member_hosts`]); `added` when this edit added it.
    fn regenerate_members(&mut self, host: ElementId, added: bool) {
        let p = self.project();
        let Some(e) = p.element(host) else { return };
        let Some(h) = e.member_host() else { return };
        let (level, phase_created, phase_demolished, design_option) =
            (e.level, e.phase_created, e.phase_demolished, e.design_option);
        let (members, detached) = (h.member_list(), h.detached_roles());
        // A copy keeps none of the original's members.
        let others: BTreeSet<ElementId> = if added {
            p.elements
                .values()
                .filter(|o| o.id != host)
                .filter_map(|o| o.member_host())
                .flat_map(|o| o.member_list().into_iter().map(|(_, id)| id))
                .collect()
        } else {
            BTreeSet::new()
        };
        let mut wanted: BTreeMap<RoleKey, (ElementTypeId, ElementKind)> = h
            .member_layout(p)
            .into_iter()
            .filter(|(r, _, _)| !detached.contains(r))
            .map(|(r, t, k)| (r, (t, k)))
            .collect();
        let (mut kept, mut gone, mut rewrite) = (Vec::new(), Vec::new(), Vec::new());
        // Each element serves one role only (a crafted list may repeat one).
        let mut used = BTreeSet::new();
        for &(role, id) in &members {
            // A role made detached by hand: the element is the user's now.
            if detached.contains(&role) {
                continue;
            }
            let Some(x) = p
                .element(id)
                .filter(|x| !others.contains(&id) && x.id != host && h.role_fits(role, &x.kind) && used.insert(id))
            else {
                // Gone, another host's, or no longer a column or beam: dropped from the list.
                continue;
            };
            match wanted.remove(&role) {
                Some((t, kind)) => {
                    if x.kind != kind || x.type_id != t || x.level != level {
                        rewrite.push((id, t, kind));
                    }
                    kept.push((role, id));
                }
                None => gone.push(id),
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
            kept.push((role, id));
        }
        kept.sort_by_key(|m| m.0);
        if kept != members
            && let Some(h) = self.element_mut(host).and_then(|e| e.kind.member_host_mut())
        {
            h.set_member_list(&kept);
        }
    }
}

impl Project {
    /// Member hosts from a file: numbers and types repaired by the host
    /// (it goes when a type has no fallback), members that are gone, do not
    /// fit their role, repeat a role or are another host's dropped, and
    /// repeated detached roles dropped. Returns the repairs.
    pub(crate) fn repair_member_hosts(&mut self) -> usize {
        let ids: Vec<ElementId> =
            self.elements.values().filter(|e| e.member_host().is_some()).map(|e| e.id).collect();
        let mut repairs = 0;
        let mut claimed = BTreeSet::new();
        let mut dropped = Vec::new();
        for id in ids {
            let Some(mut kind) = self.elements.get(&id).map(|e| e.kind.clone()) else { continue };
            let Some(h) = kind.member_host_mut() else { continue };
            let Some(mut n) = h.repair_parameters(self) else {
                dropped.push(id);
                continue;
            };
            let list = h.member_list();
            let mut roles = BTreeSet::new();
            let kept: Vec<(RoleKey, ElementId)> = list
                .iter()
                .copied()
                .filter(|&(r, m)| {
                    self.element(m).is_some_and(|e| h.role_fits(r, &e.kind)) && roles.insert(r) && claimed.insert(m)
                })
                .collect();
            n += list.len() - kept.len();
            h.set_member_list(&kept);
            let detached = h.detached_roles();
            let mut seen = BTreeSet::new();
            let unique: Vec<RoleKey> = detached.iter().copied().filter(|r| seen.insert(*r)).collect();
            n += detached.len() - unique.len();
            h.set_detached_roles(&unique);
            if n > 0 {
                if let Some(e) = self.elements.get_mut(&id) {
                    e.kind = kind;
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

If the borrow checker rejects `h.role_fits` inside the `filter` closure while `h: &mut dyn MemberHost`, take `let fits = &*h;` before the closure and call `fits.role_fits`, then use `h` again after the closure ends.

- [ ] **Step 4: Port the steel building.** In `crates/cc_core/src/steel_building.rs`:
  - change the imports to `use crate::member_host::{MemberHost, RoleKey, fix_type, reset, within};` plus `Beam, Category, Column, ElementId, ElementKind, ElementType, ElementTypeId, PlanTransform, Project, TypeData, WallTop` (drop `Element`, `BTreeMap`, `BTreeSet` if unused outside tests; tests import what they need);
  - delete the private `within`, `reset`, `fix_type`, the whole `impl crate::EditCtx<'_>` block, `Project::steel_members_of` and `Project::repair_steel_buildings`;
  - add after `enum RoleKind`:

```rust
impl RoleKind {
    pub const ALL: [RoleKind; 4] = [RoleKind::Column, RoleKind::Rafter, RoleKind::Purlin, RoleKind::Girt];
}
```

  - add to `impl MemberRole`:

```rust
    /// Its key for the shared member-host code (same order as the role).
    pub fn key(self) -> RoleKey {
        RoleKey::new(self.kind as u8, self.frame, self.index)
    }

    pub fn from_key(key: RoleKey) -> Option<Self> {
        let (kind, frame, index) = key.parts();
        Some(Self { kind: *RoleKind::ALL.get(usize::from(kind))?, frame, index })
    }
```

  - add the trait impl:

```rust
impl MemberHost for SteelBuilding {
    fn member_types(&self) -> Vec<ElementTypeId> {
        self.type_ids().collect()
    }

    fn member_layout(&self, project: &Project) -> Vec<(RoleKey, ElementTypeId, ElementKind)> {
        self.layout(project).into_iter().map(|(r, t, k)| (r.key(), t, k)).collect()
    }

    fn member_list(&self) -> Vec<(RoleKey, ElementId)> {
        self.members.iter().map(|m| (m.role.key(), m.id)).collect()
    }

    fn set_member_list(&mut self, members: &[(RoleKey, ElementId)]) {
        self.members = members
            .iter()
            .filter_map(|&(k, id)| Some(FrameMember { role: MemberRole::from_key(k)?, id }))
            .collect();
    }

    fn detached_roles(&self) -> Vec<RoleKey> {
        self.detached.iter().map(|r| r.key()).collect()
    }

    fn set_detached_roles(&mut self, roles: &[RoleKey]) {
        self.detached = roles.iter().filter_map(|k| MemberRole::from_key(*k)).collect();
    }

    fn role_fits(&self, role: RoleKey, kind: &ElementKind) -> bool {
        MemberRole::from_key(role).is_some_and(|r| r.fits(kind))
    }

    fn check_parameters(&self, project: &Project) -> Result<(), String> {
        self.check().and_then(|()| self.check_types(project))
    }

    fn repair_parameters(&mut self, project: &Project) -> Option<usize> {
        let mut n = self.reset_out_of_range();
        n += fix_type(project, &mut self.column_type, Category::Column)?;
        n += fix_type(project, &mut self.rafter_type, Category::Beam)?;
        for s in [&mut self.purlins, &mut self.girts].into_iter().flatten() {
            n += fix_type(project, &mut s.type_id, Category::Beam)?;
        }
        Some(n)
    }
}
```

  - wire `document.rs`, `transform.rs`, `repair.rs` and the engine caller as listed under *Files*.

- [ ] **Step 5: Generic host check in `element_set`.** In `crates/cc_engine/src/commands/edit.rs`, after the `discriminant` check and before `match &kind`, add:

```rust
    if let Some(new) = kind.member_host() {
        if let Some(old) = element.kind.member_host()
            && new.member_list() != old.member_list()
        {
            return Err(EngineError::invalid(
                "a host keeps its own members: change its parameters, or edit a member to detach it",
            ));
        }
        new.check_parameters(p).map_err(EngineError::invalid)?;
    }
```

and delete the `cc_core::ElementKind::SteelBuilding(b) => { .. }` arm.

- [ ] **Step 6: Run the tests.** `cargo nextest run -p cc_core member_host steel_building` and `cargo nextest run -p cc_engine --test main steel_building`. Expected: all pass, the steel tests unchanged (in particular `repair_drops_dangling_members_and_resets_bad_values` still counts 4 repairs and `a_new_span_moves_the_members_and_keeps_their_ids` still compares the building with `before` after undo).

- [ ] **Step 7: Clippy, fmt, commit.** `cargo clippy -p cc_core -p cc_engine --all-targets -- -D warnings`, `cargo fmt --all`, then:

```bash
git add -A
git commit -s -m "Share the steel building's member sync as member hosts"
```

---

### Task 2: The truss model and its layout (cc_core)

**Files:**
- Create: `crates/cc_core/src/truss.rs`
- Modify:
  - `crates/cc_core/src/lib.rs`: `pub mod truss;` after `pub mod transform;`.
  - `crates/cc_core/src/element.rs`: variant `/// A parametric truss: it owns the beams and columns of its chords and web.` `Truss(Box<crate::truss::Truss>),` after `SteelBuilding(..)`; `Element::category` arm `ElementKind::Truss(_) => Category::Truss,`; `Element::translate` arm `ElementKind::Truss(t) => { t.start += delta; t.end += delta; }`.
  - `crates/cc_core/src/transform.rs`: `Element::transform` arm `ElementKind::Truss(tr) => tr.transform(t),`.
  - `crates/cc_core/src/types.rs`:
    - `Category::Truss` (doc `/// Parametric trusses: hosts of chords, diagonals and posts.`) right after `SteelBuilding`;
    - `Category::ALL` becomes `[Category; 36]` with `Category::Truss` after `Category::SteelBuilding` (not in `MODEL` nor `SCHEDULABLE`);
    - `Category::key`: `Category::Truss => "category-truss",`;
    - `TypeData::Truss {}` (doc `/// Trusses (no properties of their own; types name them).`) after `SteelBuilding {}`;
    - every `TypeData` match listing `TypeData::SteelBuilding {}` (around lines 605, 679, 994) also lists `| TypeData::Truss {}`; the category match gets `TypeData::Truss {} => Category::Truss,`.
  - `crates/cc_core/src/repair.rs`: `element_is_valid` arm `ElementKind::Truss(t) => t.start.is_finite() && t.end.is_finite(),`; add `| TypeData::Truss {}` next to `| TypeData::SteelBuilding {} => {}`.
  - `crates/cc_core/src/schedule.rs` (~204, ~293): `Category::Truss` with `SteelBuilding` (`&[]`, `Vec::new()`).
  - `crates/cc_core/src/member_host.rs`: dispatch arms `ElementKind::Truss(t) => Some(t.as_ref() as &dyn MemberHost),` and the `_mut` one.
  - Other crates' exhaustive matches the compiler flags: `crates/cc_views/src/legend.rs` (~259) and `crates/cc_ifc/src/export.rs` (~1723) add `| TypeData::Truss {}` (and in the IFC comment: "... steel buildings, trusses and pads have no IFC type object"); `crates/cc_app/src/commands.rs` (~1699, ~1740) `Category::Truss => t!("category-truss"),`; `crates/cc_app/src/tools.rs` (~390) add `| Category::Truss` to the `return None` arm; `crates/cc_app/src/ribbon.rs` (~54) `Category::Truss => Icon::Truss,`; `crates/cc_app/src/properties.rs` (~541) `ElementKind::Truss(_) => {}` for now (Task 6 fills it); `crates/cc_engine/src/commands/query.rs` none yet (Task 4).
  - `assets/i18n/*.ftl` (12): `category-truss`, `tpl-type-truss`.
- Test: `crates/cc_core/src/truss.rs` (`mod tests`).

**Interfaces:**
- Consumes (Task 1): `member_host::{MemberHost, RoleKey, fix_type, reset, within}`.
- Produces (module `cc_core::truss`):
  - consts `MAX_MEMBERS: usize = 4000`, `MIN_VERTICAL: f64 = 0.05`, `LENGTH`, `HEIGHT`, `SPACING: RangeInclusive<f64>`, `PANELS`, `COPIES: RangeInclusive<u32>`, `DEFAULT_LENGTH = 6.0`, `DEFAULT_HEIGHT = 1.0`, `DEFAULT_SPACING = 1.5`, `PANEL_LENGTH = 1.5`.
  - `enum TrussShape { Flat (default), Mono, Gable }` with `ALL`, `code`, `from_code`, `key`.
  - `enum TrussPattern { Pratt (default), Howe, Warren, KingPost, QueenPost, Fink }` with `ALL`, `code`, `from_code`, `key`, `needs_gable() -> bool`, `min_panels() -> u32`.
  - `enum TrussRoleKind { TopChord, BottomChord, Diagonal, Vertical }` with `ALL`; `struct TrussRole { pub kind: TrussRoleKind, pub copy: u16, pub index: u16 }` with `fits`, `key`, `from_key`; `struct TrussMember { pub role: TrussRole, pub id: ElementId }`.
  - `struct Truss { start, end, base_offset, shape, height, pattern, panels, top_chord_type, bottom_chord_type, web_type, post_type, copies, spacing, members, detached }` with `new(chord: ElementTypeId, web: ElementTypeId, post: ElementTypeId) -> Self`, `length()`, `direction()`, `left()`, `rise(x)`, `pattern_used()`, `span_lines() -> Vec<[DVec2; 2]>`, `check()`, `check_types(&Project)`, `reset_out_of_range() -> usize`, `type_ids() -> impl Iterator<Item = ElementTypeId>`, `transform(&PlanTransform)`, `layout() -> Vec<(TrussRole, ElementTypeId, ElementKind)>`.
  - `default_panels(length: f64, shape: TrussShape) -> u32`, `host_type(id: ElementTypeId, name: String) -> ElementType`.
  - `ElementKind::Truss(Box<Truss>)`, `Category::Truss`, `TypeData::Truss {}`.

- [ ] **Step 1: Write the module's tests first** (the `mod tests` below, with the module's `use` lines and empty item stubs only where needed to get to a compile failure). The tests:

```rust
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{ColumnSection, Document, Element, LevelId};

    fn add_type(e: &mut crate::EditCtx, data: TypeData) -> ElementTypeId {
        let id = ElementTypeId(e.alloc_id());
        e.add_type(ElementType { id, name: format!("T{}", id.0), data, info: Default::default(), uid: Default::default() })
    }

    fn element(id: ElementId, level: LevelId, type_id: ElementTypeId, kind: ElementKind) -> Element {
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

    /// A document with a flat Pratt truss from (0, 0) to (6, 0), 1 m deep,
    /// 4 panels, chords and diagonals of a 0.06 x 0.15 beam type and posts
    /// of a 0.06 x 0.1 column type, made by `f` before it is added.
    pub(crate) fn site_with(f: impl FnOnce(&mut Truss)) -> (Document, ElementId) {
        let mut doc = Document::new(Project::from_template());
        let host = doc.edit("truss", |e| {
            let level = e.project().levels[0].id;
            let material = e.project().materials[0].name.clone();
            let beam = add_type(
                e,
                TypeData::Beam {
                    width: 0.06,
                    height: 0.15,
                    material: material.clone(),
                    shape: Default::default(),
                    profile: None,
                    profile_rotation_deg: 0.0,
                    profile_mirror: false,
                },
            );
            let post = add_type(
                e,
                TypeData::Column {
                    section: ColumnSection::Rectangular { width: 0.06, depth: 0.1 },
                    material,
                    profile: None,
                    profile_rotation_deg: 0.0,
                    profile_mirror: false,
                },
            );
            let ty = add_type(e, TypeData::Truss {});
            let mut t = Truss::new(beam, beam, post);
            f(&mut t);
            let id = ElementId(e.alloc_id());
            e.add_element(element(id, level, ty, ElementKind::Truss(Box::new(t))))
        });
        (doc, host)
    }

    pub(crate) fn truss(doc: &Document, host: ElementId) -> Truss {
        match doc.project.element(host).map(|e| &e.kind) {
            Some(ElementKind::Truss(t)) => (**t).clone(),
            _ => panic!("not a truss"),
        }
    }

    fn counts(t: &Truss) -> [usize; 4] {
        let l = t.layout();
        TrussRoleKind::ALL.map(|k| l.iter().filter(|(r, _, _)| r.kind == k).count())
    }

    fn close(a: DVec2, b: DVec2) -> bool {
        a.distance(b) < 1e-9
    }

    #[test]
    fn member_counts_per_shape_and_pattern() {
        let (doc, host) = site_with(|_| {});
        let mut t = truss(&doc, host);
        // [top chords, bottom chords, diagonals, verticals], 4 panels.
        let cases = [
            (TrussShape::Flat, TrussPattern::Pratt, [4, 4, 4, 3]),
            (TrussShape::Flat, TrussPattern::Howe, [4, 4, 4, 3]),
            (TrussShape::Flat, TrussPattern::Warren, [4, 4, 4, 0]),
            // On a gable the end panels' Pratt diagonals would lie on the bottom chord.
            (TrussShape::Gable, TrussPattern::Pratt, [4, 4, 2, 3]),
            (TrussShape::Gable, TrussPattern::Howe, [4, 4, 4, 3]),
            (TrussShape::Gable, TrussPattern::KingPost, [4, 4, 2, 1]),
            // A mono truss gets its high end post; its first Pratt diagonal is skipped.
            (TrussShape::Mono, TrussPattern::Pratt, [4, 4, 3, 4]),
            // Not a gable: king post falls back to Pratt.
            (TrussShape::Flat, TrussPattern::KingPost, [4, 4, 4, 3]),
            // Fink needs 6 panels: Pratt on a 4-panel gable.
            (TrussShape::Gable, TrussPattern::Fink, [4, 4, 2, 3]),
        ];
        for (shape, pattern, want) in cases {
            (t.shape, t.pattern) = (shape, pattern);
            assert_eq!(counts(&t), want, "{shape:?} {pattern:?}");
        }
        t.shape = TrussShape::Gable;
        t.panels = 6;
        for (pattern, want) in [(TrussPattern::QueenPost, [6, 6, 2, 2]), (TrussPattern::Fink, [6, 6, 4, 0])] {
            t.pattern = pattern;
            assert_eq!(counts(&t), want, "{pattern:?}");
        }
        // A flat Pratt truss of n panels: 2n chords, n − 1 verticals, n diagonals.
        (t.shape, t.pattern, t.panels) = (TrussShape::Flat, TrussPattern::Pratt, 10);
        assert_eq!(counts(&t), [10, 10, 10, 9]);
    }

    #[test]
    fn the_gable_apex_and_the_king_post_members() {
        let (doc, host) = site_with(|t| {
            (t.shape, t.pattern, t.height, t.base_offset) = (TrussShape::Gable, TrussPattern::KingPost, 1.5, 2.0);
        });
        let t = truss(&doc, host);
        let l = t.layout();
        let find = |kind, index| {
            l.iter()
                .find(|(r, _, _)| *r == TrussRole { kind, copy: 0, index })
                .map(|(_, _, k)| k.clone())
                .expect("role")
        };
        // The king post: mid-span, from the bottom chord axis to the apex.
        let ElementKind::Column(post) = find(TrussRoleKind::Vertical, 2) else { panic!("a column") };
        assert!(close(post.position, DVec2::new(3.0, 0.0)));
        assert!((post.base_offset - 2.0).abs() < 1e-9 && (post.height - 1.5).abs() < 1e-9);
        // Two diagonals from its foot to the quarter points of the top chords.
        let ElementKind::Beam(d0) = find(TrussRoleKind::Diagonal, 0) else { panic!("a beam") };
        assert!(close(d0.start, DVec2::new(3.0, 0.0)) && close(d0.end, DVec2::new(1.5, 0.0)));
        assert!((d0.top_offset - 2.0).abs() < 1e-9 && (d0.end_offset.unwrap_or(0.0) - 2.75).abs() < 1e-9);
        let ElementKind::Beam(d1) = find(TrussRoleKind::Diagonal, 1) else { panic!("a beam") };
        assert!(close(d1.end, DVec2::new(4.5, 0.0)));
        // The top chord rises from the support to the apex.
        let ElementKind::Beam(top) = find(TrussRoleKind::TopChord, 1) else { panic!("a beam") };
        assert!((top.top_offset - 2.75).abs() < 1e-9 && (top.end_offset.unwrap_or(0.0) - 3.5).abs() < 1e-9);
        assert_eq!(top.vertical, crate::BeamVertical::Center);
        assert!((t.rise(3.0) - 1.5).abs() < 1e-9);
    }

    #[test]
    fn an_odd_panel_count_on_a_gable_is_refused() {
        let (doc, host) = site_with(|_| {});
        let mut t = truss(&doc, host);
        (t.shape, t.panels) = (TrussShape::Gable, 5);
        assert!(t.check().is_err());
        assert!(t.layout().is_empty());
        assert_eq!(t.reset_out_of_range(), 1);
        assert_eq!(t.panels, 6);
        t.shape = TrussShape::Flat;
        t.panels = 5;
        assert!(t.check().is_ok(), "a flat truss takes any count");
        for bad in [1, 65] {
            t.panels = bad;
            assert!(t.check().is_err(), "{bad}");
        }
    }

    #[test]
    fn copies_are_offset_by_the_spacing_to_the_left() {
        let (doc, host) = site_with(|t| (t.copies, t.spacing) = (3, 2.0));
        let t = truss(&doc, host);
        let l = t.layout();
        assert_eq!(l.len(), 3 * 15);
        let start = |copy| {
            l.iter()
                .find(|(r, _, _)| *r == TrussRole { kind: TrussRoleKind::BottomChord, copy, index: 0 })
                .and_then(|(_, _, k)| if let ElementKind::Beam(b) = k { Some(b.start) } else { None })
                .expect("a bottom chord")
        };
        assert!(close(start(2), DVec2::new(0.0, 4.0)), "left of +x is +y");
        let lines = t.span_lines();
        assert_eq!(lines.len(), 3);
        assert!(close(lines[1][0], DVec2::new(0.0, 2.0)) && close(lines[1][1], DVec2::new(6.0, 2.0)));
    }

    #[test]
    fn a_mirror_maps_every_copy() {
        let (doc, host) = site_with(|t| (t.copies, t.spacing, t.shape) = (2, 2.0, TrussShape::Mono));
        let mut t = truss(&doc, host);
        let m = PlanTransform::mirror_across(DVec2::new(3.0, 0.0), DVec2::new(0.0, 1.0));
        let want: Vec<[DVec2; 2]> = t.span_lines().iter().map(|[a, b]| [m.apply(*a), m.apply(*b)]).collect();
        t.transform(&m);
        let got = t.span_lines();
        for w in &want {
            assert!(got.iter().any(|g| close(g[0], w[0]) && close(g[1], w[1])), "{w:?} in {got:?}");
        }
    }

    #[test]
    fn default_panels_follow_the_span() {
        assert_eq!(default_panels(6.0, TrussShape::Flat), 4);
        assert_eq!(default_panels(7.5, TrussShape::Flat), 5);
        assert_eq!(default_panels(7.5, TrussShape::Gable), 6);
        assert_eq!(default_panels(1.0, TrussShape::Flat), 2);
        assert_eq!(default_panels(500.0, TrussShape::Gable), 64);
        for p in TrussPattern::ALL {
            assert_eq!(TrussPattern::from_code(p.code()), Some(p));
        }
        assert_eq!(TrussShape::from_code(" GABLE "), Some(TrussShape::Gable));
        assert_eq!(TrussPattern::from_code("k"), None);
    }

    #[test]
    fn layouts_stop_at_the_limit() {
        let (doc, host) = site_with(|t| {
            (t.end, t.panels, t.copies) = (DVec2::new(100.0, 0.0), 64, 200);
        });
        assert_eq!(truss(&doc, host).layout().len(), MAX_MEMBERS);
    }
}
```

- [ ] **Step 2: Run them to see them fail.** `cargo nextest run -p cc_core truss`. Expected: compile errors (`Truss`, `TrussShape`, … missing).

- [ ] **Step 3: Write the module** (above the tests):

```rust
//! Parametric trusses: a host element that holds the parameters of a plane
//! truss (its span line, shape, height, web pattern, panels, member types
//! and copies) and owns the ordinary beams (chords, diagonals) and columns
//! (verticals) made from them, kept in step like a steel building's (see
//! [`crate::member_host`]).
//!
//! Local coordinates: x along start → end (`L` long), the copies to the
//! left of it every `spacing`, heights above the level from `base_offset`
//! (the bottom chord's axis).

use crate::member_host::{MemberHost, RoleKey, fix_type, reset, within};
use crate::{
    Beam, BeamVertical, Category, Column, ElementId, ElementKind, ElementType, ElementTypeId, PlanTransform, Project,
    TypeData, WallTop,
};
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;

/// Most members one truss (all copies) makes; the layout stops there.
pub const MAX_MEMBERS: usize = 4000;
/// A vertical shorter than this is skipped, and so is a diagonal whose ends
/// differ less in height (it would lie on the bottom chord), meters.
pub const MIN_VERTICAL: f64 = 0.05;

pub const LENGTH: RangeInclusive<f64> = 0.5..=200.0;
pub const HEIGHT: RangeInclusive<f64> = 0.1..=20.0;
pub const PANELS: RangeInclusive<u32> = 2..=64;
pub const COPIES: RangeInclusive<u32> = 1..=200;
pub const SPACING: RangeInclusive<f64> = 0.3..=20.0;

pub const DEFAULT_LENGTH: f64 = 6.0;
pub const DEFAULT_HEIGHT: f64 = 1.0;
pub const DEFAULT_SPACING: f64 = 1.5;
/// The panel length the default panel count aims at, meters.
pub const PANEL_LENGTH: f64 = 1.5;

/// The outline of the truss: chords parallel, the top chord rising toward
/// the end (mono), or toward mid-span from both supports (gable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TrussShape {
    #[default]
    Flat,
    Mono,
    Gable,
}

impl TrussShape {
    pub const ALL: [TrussShape; 3] = [TrussShape::Flat, TrussShape::Mono, TrussShape::Gable];

    pub fn code(self) -> &'static str {
        match self {
            TrussShape::Flat => "flat",
            TrussShape::Mono => "mono",
            TrussShape::Gable => "gable",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.code().eq_ignore_ascii_case(code.trim()))
    }

    /// The i18n key of its name.
    pub fn key(self) -> &'static str {
        match self {
            TrussShape::Flat => "roof-flat",
            TrussShape::Mono => "roof-shed",
            TrussShape::Gable => "roof-gable",
        }
    }
}

/// The web between the chords.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TrussPattern {
    #[default]
    Pratt,
    Howe,
    Warren,
    KingPost,
    QueenPost,
    Fink,
}

impl TrussPattern {
    pub const ALL: [TrussPattern; 6] = [
        TrussPattern::Pratt,
        TrussPattern::Howe,
        TrussPattern::Warren,
        TrussPattern::KingPost,
        TrussPattern::QueenPost,
        TrussPattern::Fink,
    ];

    pub fn code(self) -> &'static str {
        match self {
            TrussPattern::Pratt => "pratt",
            TrussPattern::Howe => "howe",
            TrussPattern::Warren => "warren",
            TrussPattern::KingPost => "kingpost",
            TrussPattern::QueenPost => "queenpost",
            TrussPattern::Fink => "fink",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.code().eq_ignore_ascii_case(code.trim()))
    }

    pub fn key(self) -> &'static str {
        match self {
            TrussPattern::Pratt => "truss-pratt",
            TrussPattern::Howe => "truss-howe",
            TrussPattern::Warren => "truss-warren",
            TrussPattern::KingPost => "truss-king-post",
            TrussPattern::QueenPost => "truss-queen-post",
            TrussPattern::Fink => "truss-fink",
        }
    }

    /// Whether it is made for a gable only.
    pub fn needs_gable(self) -> bool {
        matches!(self, TrussPattern::KingPost | TrussPattern::QueenPost | TrussPattern::Fink)
    }

    /// The fewest panels it can be laid out on.
    pub fn min_panels(self) -> u32 {
        match self {
            TrussPattern::QueenPost => 4,
            TrussPattern::Fink => 6,
            _ => 2,
        }
    }
}

/// What a member is in its truss.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TrussRoleKind {
    TopChord,
    BottomChord,
    Diagonal,
    Vertical,
}

impl TrussRoleKind {
    pub const ALL: [TrussRoleKind; 4] =
        [TrussRoleKind::TopChord, TrussRoleKind::BottomChord, TrussRoleKind::Diagonal, TrussRoleKind::Vertical];
}

/// A member's place: its kind, its copy and its index there (chords: the
/// panel; verticals: the panel point; diagonals: their place in the pattern).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TrussRole {
    pub kind: TrussRoleKind,
    pub copy: u16,
    pub index: u16,
}

impl TrussRole {
    /// A column for verticals, a beam for the rest.
    pub fn fits(self, kind: &ElementKind) -> bool {
        match self.kind {
            TrussRoleKind::Vertical => matches!(kind, ElementKind::Column(_)),
            _ => matches!(kind, ElementKind::Beam(_)),
        }
    }

    pub fn key(self) -> RoleKey {
        RoleKey::new(self.kind as u8, self.copy, self.index)
    }

    pub fn from_key(key: RoleKey) -> Option<Self> {
        let (kind, copy, index) = key.parts();
        Some(Self { kind: *TrussRoleKind::ALL.get(usize::from(kind))?, copy, index })
    }
}

/// A member the truss manages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrussMember {
    pub role: TrussRole,
    pub id: ElementId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Truss {
    /// The span line in plan, between the supports.
    pub start: DVec2,
    pub end: DVec2,
    /// The bottom chord's axis above the level at the supports, meters.
    pub base_offset: f64,
    #[serde(default)]
    pub shape: TrussShape,
    /// Chord axis to chord axis (flat), or the rise at the apex or high end, meters.
    pub height: f64,
    #[serde(default)]
    pub pattern: TrussPattern,
    /// Bottom-chord panels (even on a gable).
    pub panels: u32,
    pub top_chord_type: ElementTypeId,
    pub bottom_chord_type: ElementTypeId,
    pub web_type: ElementTypeId,
    /// The column type of the verticals.
    pub post_type: ElementTypeId,
    #[serde(default = "one")]
    pub copies: u32,
    /// Copy to copy, to the left of start → end, meters.
    #[serde(default = "default_spacing")]
    pub spacing: f64,
    #[serde(default)]
    pub members: Vec<TrussMember>,
    #[serde(default)]
    pub detached: Vec<TrussRole>,
}

fn one() -> u32 {
    1
}

fn default_spacing() -> f64 {
    DEFAULT_SPACING
}

/// About one panel per [`PANEL_LENGTH`], 2 to 64, even on a gable.
pub fn default_panels(length: f64, shape: TrussShape) -> u32 {
    let n = if length.is_finite() { (length / PANEL_LENGTH).round().clamp(2.0, 64.0) as u32 } else { 4 };
    if shape == TrussShape::Gable && n % 2 == 1 { n + 1 } else { n }
}

/// The type a project gets with its first truss.
pub fn host_type(id: ElementTypeId, name: String) -> ElementType {
    ElementType { id, name, data: TypeData::Truss {}, info: Default::default(), uid: Default::default() }
}

/// A panel point: its index along the span and whether it is on the top chord.
type Node = (usize, bool);

/// The panel point nearest `n / part`, kept off the supports and mid-span;
/// `None` when the half span has no inner point.
fn nearest(n: usize, part: f64) -> Option<usize> {
    let half = n / 2;
    (half >= 2).then(|| ((n as f64 / part).round() as usize).clamp(1, half - 1))
}

/// The web of `pattern` over `n` panels: the panel points with a vertical,
/// and the diagonals, in a fixed order (a diagonal's index is its place here).
fn web(pattern: TrussPattern, n: usize) -> (Vec<usize>, Vec<(Node, Node)>) {
    let mid = n / 2;
    // Panel i lies left of mid-span.
    let left = |i: usize| 2 * i + 1 < n;
    let inner: Vec<usize> = (1..n).collect();
    match pattern {
        // Diagonals slope down toward the middle.
        TrussPattern::Pratt => (
            inner,
            (0..n).map(|i| if left(i) { ((i, true), (i + 1, false)) } else { ((i, false), (i + 1, true)) }).collect(),
        ),
        // Diagonals slope up toward the middle.
        TrussPattern::Howe => (
            inner,
            (0..n).map(|i| if left(i) { ((i, false), (i + 1, true)) } else { ((i, true), (i + 1, false)) }).collect(),
        ),
        TrussPattern::Warren => (
            Vec::new(),
            (0..n).map(|i| if i % 2 == 1 { ((i, true), (i + 1, false)) } else { ((i, false), (i + 1, true)) }).collect(),
        ),
        TrussPattern::KingPost => {
            let diagonals = nearest(n, 4.0)
                .map(|q| vec![((mid, false), (q, true)), ((mid, false), (n - q, true))])
                .unwrap_or_default();
            (vec![mid], diagonals)
        }
        TrussPattern::QueenPost => match nearest(n, 3.0) {
            Some(t) => (vec![t, n - t], vec![((t, false), (mid, true)), ((n - t, false), (mid, true))]),
            None => (Vec::new(), Vec::new()),
        },
        TrussPattern::Fink => {
            let q = (n / 4).max(1);
            let t = n.div_ceil(3).max(q + 1);
            (
                Vec::new(),
                vec![
                    ((t, false), (q, true)),
                    ((t, false), (mid, true)),
                    ((n - t, false), (n - q, true)),
                    ((n - t, false), (mid, true)),
                ],
            )
        }
    }
}

/// A chord or diagonal from `a` to `b` (heights above the level), its axis through both.
fn member(a: DVec2, za: f64, b: DVec2, zb: f64) -> Beam {
    let mut beam = Beam::new(a, b, za);
    beam.vertical = BeamVertical::Center;
    if (zb - za).abs() > 1e-9 {
        beam.end_offset = Some(zb);
    }
    beam
}

/// Members collected by a layout, stopping at [`MAX_MEMBERS`].
struct Members(Vec<(TrussRole, ElementTypeId, ElementKind)>);

impl Members {
    fn full(&self) -> bool {
        self.0.len() >= MAX_MEMBERS
    }

    fn push(&mut self, kind: TrussRoleKind, copy: u32, index: usize, type_id: ElementTypeId, element: ElementKind) {
        let (Ok(copy), Ok(index)) = (u16::try_from(copy), u16::try_from(index)) else { return };
        if !self.full() {
            self.0.push((TrussRole { kind, copy, index }, type_id, element));
        }
    }
}

impl Truss {
    /// A flat Pratt truss 6 m long on the x axis, 1 m deep, at the level.
    pub fn new(chord_type: ElementTypeId, web_type: ElementTypeId, post_type: ElementTypeId) -> Self {
        Self {
            start: DVec2::ZERO,
            end: DVec2::new(DEFAULT_LENGTH, 0.0),
            base_offset: 0.0,
            shape: TrussShape::Flat,
            height: DEFAULT_HEIGHT,
            pattern: TrussPattern::Pratt,
            panels: default_panels(DEFAULT_LENGTH, TrussShape::Flat),
            top_chord_type: chord_type,
            bottom_chord_type: chord_type,
            web_type,
            post_type,
            copies: 1,
            spacing: DEFAULT_SPACING,
            members: Vec::new(),
            detached: Vec::new(),
        }
    }

    pub fn length(&self) -> f64 {
        self.start.distance(self.end)
    }

    /// The unit vector along start → end (zero when they meet).
    pub fn direction(&self) -> DVec2 {
        (self.end - self.start).normalize_or_zero()
    }

    /// The unit vector to the left of start → end, where copies go.
    pub fn left(&self) -> DVec2 {
        self.direction().perp()
    }

    /// The top chord's axis above the bottom chord's at distance `x` from the start.
    pub fn rise(&self, x: f64) -> f64 {
        let l = self.length();
        if l <= 0.0 {
            return 0.0;
        }
        let t = (x / l).clamp(0.0, 1.0);
        match self.shape {
            TrussShape::Flat => self.height,
            TrussShape::Mono => self.height * t,
            TrussShape::Gable => self.height * (1.0 - (2.0 * t - 1.0).abs()),
        }
    }

    /// The pattern it is laid out with: a gable-only pattern falls back to
    /// Pratt on another shape, or on fewer panels than it needs.
    pub fn pattern_used(&self) -> TrussPattern {
        let p = self.pattern;
        if p.needs_gable() && (self.shape != TrussShape::Gable || self.panels < p.min_panels()) {
            TrussPattern::Pratt
        } else {
            p
        }
    }

    /// Each copy's span line.
    pub fn span_lines(&self) -> Vec<[DVec2; 2]> {
        let left = self.left();
        (0..self.copies.min(*COPIES.end()))
            .map(|k| {
                let shift = left * (f64::from(k) * self.spacing);
                [self.start + shift, self.end + shift]
            })
            .collect()
    }

    /// Whether every number is in its range (the first one that is not, named).
    pub fn check(&self) -> Result<(), String> {
        if !(self.start.is_finite() && self.end.is_finite() && self.base_offset.is_finite()) {
            return Err("start, end and base_offset must be finite numbers".into());
        }
        within("the span length", self.length(), &LENGTH)?;
        within("height", self.height, &HEIGHT)?;
        within("spacing", self.spacing, &SPACING)?;
        if !PANELS.contains(&self.panels) {
            return Err(format!("panels must be between {} and {}", PANELS.start(), PANELS.end()));
        }
        if !COPIES.contains(&self.copies) {
            return Err(format!("copies must be between {} and {}", COPIES.start(), COPIES.end()));
        }
        if self.shape == TrussShape::Gable && self.panels % 2 == 1 {
            return Err("a gable truss needs an even number of panels".into());
        }
        Ok(())
    }

    /// Whether the chord and web types are beam types and the post type a column type.
    pub fn check_types(&self, project: &Project) -> Result<(), String> {
        let is = |t: ElementTypeId, c: Category| project.element_type(t).is_some_and(|x| x.category() == c);
        if !is(self.post_type, Category::Column) {
            return Err(format!("type {} is not a column type", self.post_type.0));
        }
        match [self.top_chord_type, self.bottom_chord_type, self.web_type].into_iter().find(|t| !is(*t, Category::Beam)) {
            Some(t) => Err(format!("type {} is not a beam type", t.0)),
            None => Ok(()),
        }
    }

    /// Put every out-of-range number back to its default (an odd gable
    /// count up to the next even one); returns how many.
    pub fn reset_out_of_range(&mut self) -> usize {
        let mut n = 0;
        let length = self.length();
        if !(length.is_finite() && LENGTH.contains(&length)) {
            let d = if self.direction() == DVec2::ZERO { DVec2::X } else { self.direction() };
            self.end = self.start + d * DEFAULT_LENGTH;
            n += 1;
        }
        n += reset(&mut self.height, &HEIGHT, DEFAULT_HEIGHT) + reset(&mut self.spacing, &SPACING, DEFAULT_SPACING);
        if !self.base_offset.is_finite() {
            self.base_offset = 0.0;
            n += 1;
        }
        if !PANELS.contains(&self.panels) {
            self.panels = default_panels(self.length(), self.shape);
            n += 1;
        }
        if self.shape == TrussShape::Gable && self.panels % 2 == 1 {
            self.panels += 1;
            n += 1;
        }
        if !COPIES.contains(&self.copies) {
            self.copies = 1;
            n += 1;
        }
        n
    }

    /// The member types it uses.
    pub fn type_ids(&self) -> impl Iterator<Item = ElementTypeId> + '_ {
        [self.top_chord_type, self.bottom_chord_type, self.web_type, self.post_type].into_iter()
    }

    /// Move, turn or mirror it. A mirror maps the whole row of copies: the
    /// new span line is the mirrored last copy's, so the copies stay where
    /// the mirror puts them and a mono truss keeps its high end.
    pub fn transform(&mut self, t: &PlanTransform) {
        let shift = if t.mirror {
            self.left() * (f64::from(self.copies.saturating_sub(1)) * self.spacing)
        } else {
            DVec2::ZERO
        };
        self.start = t.apply(self.start + shift);
        self.end = t.apply(self.end + shift);
    }

    /// Its members: role, type and element, copy by copy (chords, verticals,
    /// diagonals), at most [`MAX_MEMBERS`]. Empty when a number is out of range.
    pub fn layout(&self) -> Vec<(TrussRole, ElementTypeId, ElementKind)> {
        let mut out = Members(Vec::new());
        if self.check().is_err() {
            return out.0;
        }
        let n = self.panels as usize;
        let (l, dir, left) = (self.length(), self.direction(), self.left());
        let (mut posts, diagonals) = web(self.pattern_used(), n);
        // A mono truss closes at its high end.
        if self.shape == TrussShape::Mono && !posts.contains(&n) {
            posts.push(n);
        }
        let x = |i: usize| l * i as f64 / n as f64;
        let z = |(i, top): Node| self.base_offset + if top { self.rise(x(i)) } else { 0.0 };
        let angle = dir.to_angle().to_degrees();
        for k in 0..self.copies {
            if out.full() {
                break;
            }
            let origin = self.start + left * (f64::from(k) * self.spacing);
            let at = |i: usize| origin + dir * x(i);
            for i in 0..n {
                let top = member(at(i), z((i, true)), at(i + 1), z((i + 1, true)));
                out.push(TrussRoleKind::TopChord, k, i, self.top_chord_type, ElementKind::Beam(top));
                let bottom = member(at(i), z((i, false)), at(i + 1), z((i + 1, false)));
                out.push(TrussRoleKind::BottomChord, k, i, self.bottom_chord_type, ElementKind::Beam(bottom));
            }
            for &i in &posts {
                let height = self.rise(x(i));
                if height < MIN_VERTICAL {
                    continue;
                }
                let column = Column {
                    position: at(i),
                    rotation_deg: angle,
                    base_offset: self.base_offset,
                    height,
                    top: WallTop::Unconnected,
                    structural: true,
                };
                out.push(TrussRoleKind::Vertical, k, i, self.post_type, ElementKind::Column(column));
            }
            for (j, &(a, b)) in diagonals.iter().enumerate() {
                if (z(a) - z(b)).abs() < MIN_VERTICAL {
                    continue;
                }
                let beam = member(at(a.0), z(a), at(b.0), z(b));
                out.push(TrussRoleKind::Diagonal, k, j, self.web_type, ElementKind::Beam(beam));
            }
        }
        out.0
    }
}

impl MemberHost for Truss {
    fn member_types(&self) -> Vec<ElementTypeId> {
        self.type_ids().collect()
    }

    fn member_layout(&self, _project: &Project) -> Vec<(RoleKey, ElementTypeId, ElementKind)> {
        self.layout().into_iter().map(|(r, t, k)| (r.key(), t, k)).collect()
    }

    fn member_list(&self) -> Vec<(RoleKey, ElementId)> {
        self.members.iter().map(|m| (m.role.key(), m.id)).collect()
    }

    fn set_member_list(&mut self, members: &[(RoleKey, ElementId)]) {
        self.members =
            members.iter().filter_map(|&(k, id)| Some(TrussMember { role: TrussRole::from_key(k)?, id })).collect();
    }

    fn detached_roles(&self) -> Vec<RoleKey> {
        self.detached.iter().map(|r| r.key()).collect()
    }

    fn set_detached_roles(&mut self, roles: &[RoleKey]) {
        self.detached = roles.iter().filter_map(|k| TrussRole::from_key(*k)).collect();
    }

    fn role_fits(&self, role: RoleKey, kind: &ElementKind) -> bool {
        TrussRole::from_key(role).is_some_and(|r| r.fits(kind))
    }

    fn check_parameters(&self, project: &Project) -> Result<(), String> {
        self.check().and_then(|()| self.check_types(project))
    }

    fn repair_parameters(&mut self, project: &Project) -> Option<usize> {
        let mut n = self.reset_out_of_range();
        n += fix_type(project, &mut self.top_chord_type, Category::Beam)?;
        n += fix_type(project, &mut self.bottom_chord_type, Category::Beam)?;
        n += fix_type(project, &mut self.web_type, Category::Beam)?;
        n += fix_type(project, &mut self.post_type, Category::Column)?;
        Some(n)
    }
}
```

Notes for the implementer:
- `glam::DVec2::to_angle` and `perp` exist (the steel building and the old truss use them).
- `Column`'s fields are those the steel building's layout fills; if `cargo check` reports more, copy the steel building's struct literal.
- The `Gable`/`Fink` 4-panel case in the tests expects the Pratt fallback (`min_panels` 6).

- [ ] **Step 4: Wire the variant** as listed under *Files* (element, transform, types, repair, schedule, member_host dispatch, the other crates' arms, the two i18n keys from the table). Use `cargo check --workspace --tests` to list every exhaustive match.

- [ ] **Step 5: Run the tests.** `cargo nextest run -p cc_core truss`. Expected: pass.

- [ ] **Step 6: Clippy, fmt, commit.** `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all`, then:

```bash
git add -A
git commit -s -m "Add the parametric truss element and its layout"
```

---

### Task 3: Trusses kept in step (cc_core)

**Files:**
- Modify: `crates/cc_core/src/truss.rs` (`mod tests` only; the sync, removal, copy and repair are Task 1's generic code, now dispatched to `Truss` by Task 2).
- Test: the same module.

**Interfaces:**
- Consumes: `truss::tests::{site_with, truss}` (Task 2), `EditCtx::{transform_elements, copy_elements, paste_elements, remove_element, element_mut, type_mut}`, `Project::{copy_set, repair, host_members_of}`, `Document::{edit, undo, redo}`.
- Produces: nothing new (tests that pin the shared mechanism on a second host kind).

- [ ] **Step 1: Write the tests** at the end of `truss::tests`:

```rust
    fn member(t: &Truss, kind: TrussRoleKind, copy: u16, index: u16) -> ElementId {
        let role = TrussRole { kind, copy, index };
        t.members.iter().find(|m| m.role == role).map(|m| m.id).expect("member")
    }

    fn set(doc: &mut Document, host: ElementId, f: impl FnOnce(&mut Truss)) {
        doc.edit("set", |e| {
            if let Some(ElementKind::Truss(t)) = e.element_mut(host).map(|x| &mut x.kind) {
                f(t);
            }
        });
    }

    #[test]
    fn a_new_truss_gets_its_members() {
        let (doc, host) = site_with(|_| {});
        let t = truss(&doc, host);
        assert_eq!(t.members.len(), 15);
        let level = doc.project.element(host).map(|e| e.level);
        for m in &t.members {
            let e = doc.project.element(m.id).expect("the member exists");
            assert!(m.role.fits(&e.kind));
            assert_eq!(Some(e.level), level);
        }
        assert_eq!(doc.project.clone().repair(), 0, "a consistent model");
    }

    #[test]
    fn new_panels_keep_the_ids_of_the_roles_that_stay_and_undo_in_one_step() {
        let (mut doc, host) = site_with(|_| {});
        let before = truss(&doc, host);
        let start = doc.project.elements.clone();
        let first = member(&before, TrussRoleKind::BottomChord, 0, 0);
        set(&mut doc, host, |t| t.panels = 6);
        let after = truss(&doc, host);
        assert_eq!(after.members.len(), 12 + 5 + 6);
        assert_eq!(member(&after, TrussRoleKind::BottomChord, 0, 0), first, "same id");
        let Some(ElementKind::Beam(b)) = doc.project.element(first).map(|e| &e.kind) else { panic!("a beam") };
        assert!(close(b.end, DVec2::new(1.0, 0.0)), "now a 1 m panel");
        assert!(doc.undo());
        assert_eq!(truss(&doc, host), before);
        assert_eq!(doc.project.elements, start);
    }

    #[test]
    fn a_detached_member_is_left_alone() {
        let (mut doc, host) = site_with(|_| {});
        let post = member(&truss(&doc, host), TrussRoleKind::Vertical, 0, 2);
        doc.edit("move", |e| e.transform_elements(&[post], &PlanTransform::translate(DVec2::new(0.0, 1.0))));
        let t = truss(&doc, host);
        assert!(t.detached.contains(&TrussRole { kind: TrussRoleKind::Vertical, copy: 0, index: 2 }));
        set(&mut doc, host, |t| t.height = 2.0);
        let Some(ElementKind::Column(c)) = doc.project.element(post).map(|e| &e.kind) else { panic!("a column") };
        assert!(close(c.position, DVec2::new(3.0, 1.0)) && (c.height - 1.0).abs() < 1e-9, "where the user left it");
        assert_eq!(truss(&doc, host).members.len(), 14, "and not made again");
    }

    #[test]
    fn deleting_the_truss_deletes_its_members() {
        let (mut doc, host) = site_with(|t| t.copies = 2);
        let ids = doc.project.host_members_of(host);
        assert_eq!(ids.len(), 30);
        doc.edit("delete", |e| e.remove_element(host));
        assert!(ids.iter().all(|id| doc.project.element(*id).is_none()));
        assert!(doc.undo());
        assert!(ids.iter().all(|id| doc.project.element(*id).is_some()));
    }

    #[test]
    fn a_copy_gets_its_own_members() {
        let (mut doc, host) = site_with(|_| {});
        let original = truss(&doc, host);
        let ids: std::collections::BTreeSet<ElementId> = original.members.iter().map(|m| m.id).collect();
        let mut selection = vec![host];
        selection.extend(ids.iter().copied());
        let before = doc.project.elements.len();
        let copies =
            doc.edit("copy", |e| e.copy_elements(&selection, &[PlanTransform::translate(DVec2::new(0.0, 10.0))]));
        assert_eq!(copies.len(), 1, "only the truss is copied");
        let copy = truss(&doc, copies[0]);
        assert_eq!(copy.members.len(), 15);
        assert!(copy.members.iter().all(|m| !ids.contains(&m.id)));
        assert_eq!(doc.project.elements.len(), before + 16);
        assert_eq!(truss(&doc, host), original);
        assert!(doc.undo());
        assert_eq!(doc.project.elements.len(), before);
    }

    #[test]
    fn a_changed_post_type_regenerates() {
        let (mut doc, host) = site_with(|_| {});
        let t = truss(&doc, host);
        let chord = member(&t, TrussRoleKind::TopChord, 0, 0);
        doc.edit("type", |e| {
            // Any edit of a type the truss uses rewrites its members.
            if let Some(ty) = e.type_mut(t.post_type) {
                ty.name = "Renamed".into();
            }
        });
        assert_eq!(member(&truss(&doc, host), TrussRoleKind::TopChord, 0, 0), chord, "same ids");
    }

    #[test]
    fn repair_resets_bad_values_and_drops_dangling_members() {
        let (doc, host) = site_with(|_| {});
        let t = truss(&doc, host);
        let mut p = doc.project.clone();
        p.elements.remove(&member(&t, TrussRoleKind::Diagonal, 0, 0));
        if let Some(ElementKind::Truss(x)) = p.elements.get_mut(&host).map(|e| &mut e.kind) {
            (x.shape, x.panels) = (TrussShape::Gable, 5);
            x.post_type = x.web_type;
        }
        // Odd gable count, the missing member, the wrong post type.
        assert_eq!(p.repair(), 3);
        let Some(ElementKind::Truss(x)) = p.element(host).map(|e| &e.kind) else { panic!("kept") };
        assert_eq!(x.panels, 6);
        assert_eq!(p.element_type(x.post_type).map(|t| t.category()), Some(Category::Column));
        assert_eq!(p.repair(), 0);
    }
```

- [ ] **Step 2: Run them.** `cargo nextest run -p cc_core truss`. Expected: pass, since Tasks 1 and 2 did the work. If one fails, the generic code mishandles a second host kind: fix it in `member_host.rs`, not in the test. Known checks:
  - `copy_elements` may filter members through `copy_set` (`host_members_of`): if the copied count is off, check `crates/cc_core/src/transform.rs` uses `host_members_of`.
  - `type_mut` is the `EditCtx` method the steel test uses; if renaming does not show in `changed_types`, change the post type's `info` instead (any `ElementType` field the transaction tracks).

- [ ] **Step 3: Commit.**

```bash
git add -A
git commit -s -m "Test trusses kept in step by the shared member sync"
```

---

### Task 4: `truss_create` rewritten, `element_get`/`element_set` (cc_engine)

**Files:**
- Modify:
  - `crates/cc_engine/src/commands/framing.rs`: rewrite `TrussCreate` and its doc comment; delete `type PanelPoint`, `fn diagonals`, `fn post_type`; keep `member`, `point3`, `BraceCreate`, `BracesBetweenColumns`; add `TrussRows`, `truss_rows`, `new_truss_type`, `pub(crate) fn derived`.
  - `crates/cc_engine/src/commands/structure_material.rs`: move `type_or_row` here from `steel_building.rs` as `pub(crate)` (the steel command imports it from here).
  - `crates/cc_engine/src/commands/query.rs` (~250): `ElementKind::Truss(t) => super::framing::derived(t, &mut derived),`.
  - `crates/cc_engine/tests/commands.rs` (~1355): the truss part of `trusses_braces_numbering_and_structural_plans`.
  - `crates/cc_engine/tests/profiles.rs` (~658): replace `truss_posts_prefer_a_steel_profile_column_type`.
  - `assets/i18n/*.ftl`: remove `truss-default-name` (12 files) after checking `rg -n truss-default-name crates` is empty.
  - `docs/commands.md`: regenerated.
- Create: `crates/cc_engine/tests/truss.rs`; `crates/cc_engine/tests/main.rs` gets `mod truss;` between `mod topography;` and `mod underlay;`.

**Interfaces:**
- Consumes: `cc_core::truss::*` (Task 2); `structure_material::{catalog_type, material_arg}`; `refs::{level_or_first, type_of, point, Ref, Point}`; `super::{add_element, edit}`; `Session::{project, doc}`; `Document::{undo_depth, squash_since}`; `Session::undo_to`; `Project::{default_type, level_height, host_members_of}`.
- Produces:
  - `TrussCreate { start, end, height, panels, pattern, bottom_offset, level, chord_type, web_type, post_type, shape, top_chord_type, bottom_chord_type, copies, spacing, material }` (all new fields `Option`, `#[serde(default)]`), `Command::NAME = "truss_create"`, returns `{ "id": u64, "members": [u64] }`.
  - `pub fn truss_rows(m: StructuralMaterial) -> Option<TrussRows>` with `pub struct TrussRows { pub chord: (&'static str, &'static str), pub web: (&'static str, &'static str) }` (posts use `web`'s row as a column type; `None` for concrete).
  - `pub fn new_truss_type(id: ElementTypeId) -> ElementType`.
  - `pub(crate) fn type_or_row(s: &mut Session, given: Option<ElementTypeId>, category: Category, row: (&str, &str)) -> Result<ElementTypeId>` in `structure_material.rs`.
  - `element_get` `derived` for a truss: `members { top_chords, bottom_chords, diagonals, verticals }`, `detached`, `length`, `pattern_used`.

- [ ] **Step 1: Write the failing engine tests** in `crates/cc_engine/tests/truss.rs`:

```rust
//! Parametric trusses: `truss_create` makes a host that keeps its chords,
//! diagonals and posts in step with its parameters.

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

fn trusses(s: &Session) -> usize {
    s.project().elements.values().filter(|e| matches!(e.kind, ElementKind::Truss(_))).count()
}

fn derived(s: &mut Session, host: u64) -> Value {
    run(s, "element_get", json!({"id": host}))["derived"].clone()
}

const SPAN: [[f64; 2]; 2] = [[0.0, 0.0], [6.0, 0.0]];

#[test]
fn steel_defaults_make_the_catalog_types_once() {
    let mut s = Session::default();
    let depth = s.doc.undo_depth();
    let out = run(&mut s, "truss_create", json!({"start": SPAN[0], "end": SPAN[1], "height": 1.0, "panels": 4}));
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    assert_eq!(out["members"].as_array().unwrap().len(), 15);
    assert_eq!(types_named(&s, "Ue 150x60x20x2,00"), 1, "the chords' beam type");
    assert_eq!(types_named(&s, "Ue 100x50x17x2,00"), 2, "the web's beam type and the posts' column type");
    run(&mut s, "truss_create", json!({"start": [0, 5], "end": [6, 5], "height": 1.0, "material": "steel"}));
    assert_eq!(types_named(&s, "Ue 150x60x20x2,00"), 1);
    assert_eq!(trusses(&s), 2);
    run(&mut s, "undo", json!({}));
    run(&mut s, "undo", json!({}));
    assert_eq!(types_named(&s, "Ue 150x60x20x2,00"), 0, "undone with the first truss");
}

#[test]
fn timber_defaults_and_concrete_refused() {
    let mut s = Session::default();
    let args = json!({"start": SPAN[0], "end": SPAN[1], "height": 1.2, "material": "timber", "shape": "gable",
        "pattern": "kingpost", "panels": 4, "copies": 3, "spacing": 0.6});
    let out = run(&mut s, "truss_create", args);
    assert_eq!(out["members"].as_array().unwrap().len(), 33, "11 per copy");
    assert_eq!(types_named(&s, "Peça 6x16"), 1);
    assert_eq!(types_named(&s, "Peça 6x12"), 2);
    let d = derived(&mut s, id(&out));
    assert_eq!(d["pattern_used"], json!("kingpost"));
    assert_eq!(d["members"]["verticals"].as_array().unwrap().len(), 3);
    let depth = s.doc.undo_depth();
    let concrete = json!({"start": SPAN[0], "end": SPAN[1], "height": 1.0, "material": "concrete"});
    assert!(s.execute("truss_create", concrete).is_err());
    assert_eq!(s.doc.undo_depth(), depth);
    assert_eq!(trusses(&s), 1);
}

#[test]
fn bad_values_are_refused_before_anything_is_made() {
    let mut s = Session::default();
    for extra in [
        json!({"pattern": "k"}),
        json!({"shape": "dome"}),
        json!({"shape": "gable", "panels": 5}),
        json!({"panels": 1}),
        json!({"copies": 0}),
        json!({"spacing": 0.1}),
        json!({"material": "glass"}),
        json!({"post_type": "no such type"}),
    ] {
        let mut args = json!({"start": SPAN[0], "end": SPAN[1], "height": 1.0});
        args.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        assert!(s.execute("truss_create", args.clone()).is_err(), "{args}");
    }
    assert!(s.execute("truss_create", json!({"start": [0, 0], "end": [0.1, 0], "height": 1.0})).is_err());
    assert_eq!(trusses(&s), 0);
    assert_eq!(types_named(&s, "Ue 150x60x20x2,00"), 0);
}

#[test]
fn element_set_of_panels_regenerates_in_one_step() {
    let mut s = Session::default();
    let host = id(&run(&mut s, "truss_create", json!({"start": SPAN[0], "end": SPAN[1], "height": 1.0, "panels": 4})));
    let before = derived(&mut s, host)["members"].clone();
    let depth = s.doc.undo_depth();
    run(&mut s, "element_set", json!({"id": host, "params": {"panels": 6}}));
    assert_eq!(s.doc.undo_depth(), depth + 1);
    let after = derived(&mut s, host)["members"].clone();
    assert_eq!(after["bottom_chords"].as_array().unwrap().len(), 6);
    assert_eq!(after["bottom_chords"][0], before["bottom_chords"][0], "same id for the first panel");
    let params = &run(&mut s, "element_get", json!({"id": host}))["params"];
    assert_eq!((params["panels"].clone(), params["shape"].clone()), (json!(6), json!("Flat")));
    for bad in [json!({"members": []}), json!({"shape": "Gable", "panels": 5}), json!({"height": 0})] {
        assert!(s.execute("element_set", json!({"id": host, "params": bad})).is_err(), "{bad}");
    }
    assert_eq!(s.doc.undo_depth(), depth + 1);
    run(&mut s, "undo", json!({}));
    assert_eq!(derived(&mut s, host)["members"], before);
}

#[test]
fn deleting_a_member_detaches_it() {
    let mut s = Session::default();
    let host = id(&run(&mut s, "truss_create", json!({"start": SPAN[0], "end": SPAN[1], "height": 1.0, "panels": 4})));
    let post = derived(&mut s, host)["members"]["verticals"][0].as_u64().unwrap();
    run(&mut s, "elements_delete", json!({"ids": [post]}));
    assert_eq!(derived(&mut s, host)["detached"], json!(1));
    run(&mut s, "element_set", json!({"id": host, "params": {"height": 1.5}}));
    assert!(s.project().element(ElementId(post)).is_none(), "not made again");
    assert_eq!(derived(&mut s, host)["members"]["verticals"].as_array().unwrap().len(), 2);
}
```

Update `crates/cc_engine/tests/commands.rs`, the start of `trusses_braces_numbering_and_structural_plans`:

```rust
    // A flat Pratt truss, 6 m in 4 panels, 1 m deep: 8 chords, 4 diagonals, 3 posts.
    let out = run(
        &mut s,
        "truss_create",
        json!({"start": [0, 0], "end": [6, 0], "height": 1.0, "panels": 4, "bottom_offset": 2.0}),
    );
    assert_eq!(out["members"].as_array().unwrap().len(), 15, "{out}");
    let host = run(&mut s, "element_get", json!({"id": out["id"]}));
    assert_eq!(host["category"], json!("truss"), "{host}");
    // Its analytical model: all members meet at the 10 panel points.
    let m = run(&mut s, "analytical_model", json!({}));
    assert_eq!(m["nodes"].as_array().unwrap().len(), 10, "{m}");
```

(Drop the `groups_list` assertion. Keep the `"pattern": "k"` refusal below it. Check what `element_get` calls the category field — look at another test's `element_get` output, e.g. `rg -n '\["category"\]' crates/cc_engine/tests`; if it is not there, assert on `host["params"]["panels"] == json!(4)` instead. If the node count is not 10, print `m`: the chords now end at every panel point, so coincident ends must merge; a different count means the analytical model does not merge them and the expected value is what it reports for the 10 distinct points plus any extra end nodes; investigate before changing the number.)

Replace `truss_posts_prefer_a_steel_profile_column_type` in `crates/cc_engine/tests/profiles.rs` with:

```rust
#[test]
fn truss_posts_default_to_the_material_catalog_column_type() {
    let mut s = Session::default();
    let out = run(&mut s, "truss_create", json!({"start": [0, 0], "end": [6, 0], "height": 1.0, "panels": 4}));
    let posts: Vec<u64> = out["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i.as_u64().unwrap())
        .filter(|i| matches!(s.project().element(ElementId(*i)).unwrap().kind, ElementKind::Column(_)))
        .collect();
    assert_eq!(posts.len(), 3);
    for post in posts {
        let e = s.project().element(ElementId(post)).unwrap();
        let t = s.project().element_type(e.type_id).unwrap();
        assert_eq!((t.name.as_str(), t.category()), ("Ue 100x50x17x2,00", cc_core::Category::Column));
    }
    // A given post type wins.
    let column = run(&mut s, "profile_type_create", json!({"profile": "Ue 150x60x20x2,00", "category": "column"}));
    let out = run(
        &mut s,
        "truss_create",
        json!({"start": [0, 5], "end": [6, 5], "height": 1.0, "panels": 4, "post_type": column["id"]}),
    );
    let host = s.project().element(ElementId(out["id"].as_u64().unwrap())).unwrap();
    let ElementKind::Truss(t) = &host.kind else { panic!("a truss") };
    assert_eq!(t.post_type.0, column["id"].as_u64().unwrap());
}
```

(Remove now-unused imports of that file, e.g. `import_first` if nothing else uses it; `cargo check --tests -p cc_engine` warns.)

- [ ] **Step 2: Run them to see them fail.** `cargo nextest run -p cc_engine --test main truss`. Expected: failures (`members` missing from the output, unknown fields `material`, `shape`, `copies`).

- [ ] **Step 3: Move `type_or_row`** from `commands/steel_building.rs` to `commands/structure_material.rs` (same body, `pub(crate)`, doc `/// The given type, else the catalog row's (made when missing).`) and import it in `steel_building.rs` (`use super::structure_material::{catalog_type, type_or_row};`, dropping `catalog_type` there if it becomes unused).

- [ ] **Step 4: Rewrite `TrussCreate`** in `framing.rs`. Imports become:

```rust
use super::structure_material::{material_arg, type_or_row};
use super::{add_element, edit, ids_json};
use crate::error::{EngineError, Result};
use crate::refs::{self, Point, Ref, point};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::truss::{self as tr, Truss, TrussPattern, TrussRoleKind, TrussShape};
use cc_core::{Beam, BeamVertical, Category, ElementKind, ElementType, ElementTypeId, LevelId, StructuralMaterial};
use glam::DVec3;
```

(keep whatever `BraceCreate`/`BracesBetweenColumns` still need; let `cargo check` prune). Then:

```rust
/// The catalog rows a truss of a material gets by default: (series,
/// designation) of the chords and of the web (the posts are a column type of
/// the web's row).
#[derive(Debug, Clone, Copy)]
pub struct TrussRows {
    pub chord: (&'static str, &'static str),
    pub web: (&'static str, &'static str),
}

/// Steel and timber rows; `None` for concrete (trusses are not offered).
pub fn truss_rows(m: StructuralMaterial) -> Option<TrussRows> {
    match m {
        StructuralMaterial::Steel => Some(TrussRows {
            chord: ("NBR6355-Ue", "Ue 150x60x20x2,00"),
            web: ("NBR6355-Ue", "Ue 100x50x17x2,00"),
        }),
        StructuralMaterial::Timber => {
            Some(TrussRows { chord: ("Timber-Sawn", "Peça 6x16"), web: ("Timber-Sawn", "Peça 6x12") })
        }
        StructuralMaterial::Concrete => None,
    }
}

/// The type a project gets with its first truss.
pub fn new_truss_type(id: ElementTypeId) -> ElementType {
    tr::host_type(id, t!("tpl-type-truss"))
}

/// Lay out a parametric plane truss from `start` to `end`: a `flat`
/// (default), `mono` or `gable` outline `height` deep (the rise at the apex
/// or high end for mono and gable), `panels` bottom-chord panels and the web
/// of `pattern` (`pratt`, the default, `howe`, `warren`, or for gables
/// `kingpost`, `queenpost` and `fink`; a gable-only pattern on another shape
/// is laid out as Pratt). Chords and diagonals are beams, verticals are
/// columns, all ordinary elements the truss keeps in step with its
/// parameters (`element_set` regenerates them, keeping their ids); a member
/// edited or deleted by hand is detached and left alone. `copies` trusses
/// are made `spacing` apart to the left of start → end. Default types come
/// from the profile catalog for the `material` (steel: `Ue 150x60x20x2,00`
/// chords, `Ue 100x50x17x2,00` web and posts; timber: `Peça 6x16` chords,
/// `Peça 6x12` web and posts), made when missing; concrete is refused.
/// Out-of-range values are refused. One undo step; returns the truss's `id`
/// and its `members`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct TrussCreate {
    /// Start of the span line in plan (a support).
    pub start: Point,
    /// End of the span line in plan (the other support), 0.5 to 200 m from the start.
    pub end: Point,
    /// Chord axis to chord axis (flat), or the rise at the apex or high end (mono, gable), meters, 0.1 to 20.
    pub height: f64,
    /// Bottom-chord panels, 2 to 64, even on a gable (default: about one per 1.5 m).
    #[serde(default)]
    pub panels: Option<u32>,
    /// `pratt` (default), `howe`, `warren`, `kingpost`, `queenpost` or `fink`.
    #[serde(default)]
    pub pattern: Option<String>,
    /// Elevation of the bottom chord's axis above the level (default: the level height minus `height`).
    #[serde(default)]
    pub bottom_offset: Option<f64>,
    /// Level id or name (default: the lowest level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Beam type of both chords (default: the material's catalog chord).
    #[serde(default)]
    pub chord_type: Option<Ref>,
    /// Beam type of the diagonals (default: the material's catalog web row).
    #[serde(default)]
    pub web_type: Option<Ref>,
    /// Column type of the verticals (default: a column type of the material's catalog web row).
    #[serde(default)]
    pub post_type: Option<Ref>,
    /// `flat` (default), `mono` (rising toward `end`) or `gable` (rising toward mid-span).
    #[serde(default)]
    pub shape: Option<String>,
    /// Beam type of the top chord (default: `chord_type`, else the catalog chord).
    #[serde(default)]
    pub top_chord_type: Option<Ref>,
    /// Beam type of the bottom chord (default: `chord_type`, else the catalog chord).
    #[serde(default)]
    pub bottom_chord_type: Option<Ref>,
    /// How many trusses, 1 to 200 (default 1).
    #[serde(default)]
    pub copies: Option<u32>,
    /// Distance between copies, to the left of start → end, meters, 0.3 to 20 (default 1.5).
    #[serde(default)]
    pub spacing: Option<f64>,
    /// `steel` (default) or `timber`: the catalog rows of the default types.
    #[serde(default)]
    pub material: Option<String>,
}

impl Command for TrussCreate {
    const NAME: &'static str = "truss_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        // Everything that can be refused is checked before anything is made.
        let p = s.project();
        let level = refs::level_or_first(p, self.level.as_ref())?;
        let material = match &self.material {
            Some(m) => material_arg(m)?,
            None => StructuralMaterial::Steel,
        };
        let rows = truss_rows(material)
            .ok_or_else(|| EngineError::invalid("trusses are made of steel or timber, not concrete"))?;
        let shape = match &self.shape {
            Some(c) => TrussShape::from_code(c)
                .ok_or_else(|| EngineError::invalid(format!("unknown shape '{c}' (flat, mono, gable)")))?,
            None => TrussShape::Flat,
        };
        let pattern = match &self.pattern {
            Some(c) => TrussPattern::from_code(c).ok_or_else(|| {
                EngineError::invalid(format!(
                    "pattern must be pratt, howe, warren, kingpost, queenpost or fink, not '{c}'"
                ))
            })?,
            None => TrussPattern::Pratt,
        };
        let (start, end) = (point(self.start)?, point(self.end)?);
        let bottom = self.bottom_offset.unwrap_or_else(|| p.level_height(level, 3.0) - self.height);
        let placeholder = ElementTypeId(0);
        let mut truss = Truss {
            start,
            end,
            base_offset: bottom,
            shape,
            height: self.height,
            pattern,
            panels: self.panels.unwrap_or_else(|| tr::default_panels(start.distance(end), shape)),
            top_chord_type: placeholder,
            bottom_chord_type: placeholder,
            web_type: placeholder,
            post_type: placeholder,
            copies: self.copies.unwrap_or(1),
            spacing: self.spacing.unwrap_or(tr::DEFAULT_SPACING),
            members: Vec::new(),
            detached: Vec::new(),
        };
        truss.check().map_err(EngineError::invalid)?;
        let given = |r: Option<&Ref>, c: Category| r.map(|r| refs::type_of(p, c, Some(r))).transpose();
        let chord = self.chord_type.as_ref();
        let top_given = given(self.top_chord_type.as_ref().or(chord), Category::Beam)?;
        let bottom_given = given(self.bottom_chord_type.as_ref().or(chord), Category::Beam)?;
        let web_given = given(self.web_type.as_ref(), Category::Beam)?;
        let post_given = given(self.post_type.as_ref(), Category::Column)?;
        let host_type = p.default_type(Category::Truss);
        let depth = s.doc.undo_depth();
        let result = (|| -> Result<Value> {
            truss.top_chord_type = type_or_row(s, top_given, Category::Beam, rows.chord)?;
            truss.bottom_chord_type = type_or_row(s, bottom_given, Category::Beam, rows.chord)?;
            truss.web_type = type_or_row(s, web_given, Category::Beam, rows.web)?;
            truss.post_type = type_or_row(s, post_given, Category::Column, rows.web)?;
            truss.check_types(s.project()).map_err(EngineError::invalid)?;
            let kind = ElementKind::Truss(Box::new(truss));
            let id = edit(s, t!("tx-create-truss"), |e| {
                let type_id = match host_type {
                    Some(t) => t,
                    None => {
                        let id = ElementTypeId(e.alloc_id());
                        e.add_type(new_truss_type(id))
                    }
                };
                Ok(add_element(e, level, type_id, kind))
            })?;
            let members: Vec<u64> = s.project().host_members_of(id).iter().map(|m| m.0).collect();
            Ok(json!({ "id": id.0, "members": members }))
        })();
        match result {
            Ok(v) => {
                s.doc.squash_since(depth, t!("tx-create-truss"));
                Ok(v)
            }
            Err(e) => {
                s.undo_to(depth);
                Err(e)
            }
        }
    }
}

/// `element_get`'s derived values of a truss: its member ids by role kind,
/// how many roles are detached, its span length and the pattern it is laid
/// out with (a gable-only pattern falls back to Pratt).
pub(crate) fn derived(t: &Truss, out: &mut serde_json::Map<String, Value>) {
    let ids = |k: TrussRoleKind| -> Vec<u64> {
        t.members.iter().filter(|m| m.role.kind == k).map(|m| m.id.0).collect()
    };
    out.insert(
        "members".into(),
        json!({
            "top_chords": ids(TrussRoleKind::TopChord),
            "bottom_chords": ids(TrussRoleKind::BottomChord),
            "diagonals": ids(TrussRoleKind::Diagonal),
            "verticals": ids(TrussRoleKind::Vertical),
        }),
    );
    out.insert("detached".into(), json!(t.detached.len()));
    out.insert("length".into(), json!(t.length()));
    out.insert("pattern_used".into(), json!(t.pattern_used().code()));
}
```

Notes:
- `type_or_row(.., Category::Column, rows.web)` makes a column type of the web's profile through `catalog_type` (the beam type of the same row stays separate, as the steel building's `W310x38,7` column and beam types are).
- The level-height default and `bottom_offset` are kept from the old command. `check` refuses a non-finite one.
- `LevelId` and `ids_json` stay only if `BracesBetweenColumns` still uses them.

- [ ] **Step 5: Wire `element_get`** (`query.rs` arm). `element_set` needs nothing new (Task 1's generic check covers trusses).

- [ ] **Step 6: Regenerate the reference and remove the unused key.** `cargo run -q -p cc_cli -- reference > docs/commands.md`; `rg -n truss-default-name crates` (expect nothing), then delete the `truss-default-name` line from the 12 `.ftl` files.

- [ ] **Step 7: Run the tests.** `cargo nextest run -p cc_engine --test main truss trusses_braces profiles steel_building` and `cargo nextest run -p cc_engine reference i18n` (the reference and key tests). Expected: pass.

- [ ] **Step 8: Clippy, fmt, commit.** `cargo clippy -p cc_engine --all-targets -- -D warnings`, `cargo fmt --all`, then:

```bash
git add -A
git commit -s -m "Make truss_create a parametric truss with steel and timber defaults"
```

---

### Task 5: Plan span lines and IFC export (cc_views, cc_ifc)

`crates/cc_views` may still be in flux on the base branch; this branch starts from the finished `feat/steel-building`, so `draw_steel_buildings` and its test exist as below. If not, stop and rebase on the finished base first.

**Files:**
- Modify:
  - `crates/cc_views/src/plan.rs`: `is_structural` arm adds `| ElementKind::Truss(_)`; `draw_trusses`, called in `plan_of` next to `draw_steel_buildings` (inside the same `if !ceiling`); a test.
  - `crates/cc_engine/tests/truss.rs`: the IFC test.

**Interfaces:**
- Consumes: `Truss::span_lines` (Task 2), `truss::tests::site_with` is not visible across crates, so the views test builds the truss by hand; `Drawing2D::line(a, b, style, element)`.
- Produces: plans carry one `Prim::Line { style: LineStyle::Hidden, element: Some(host) }` per copy.

- [ ] **Step 1: Write the failing tests.** In `plan.rs` `mod tests`, after `steel_buildings_draw_their_footprint`:

```rust
    /// A truss draws each copy's span line dashed as its own line, so it can
    /// be picked; its members draw themselves.
    #[test]
    fn trusses_draw_their_span_lines() {
        let mut doc = Document::new(Project::from_template());
        let level = doc.project.levels[0].id;
        let host = doc.edit("t", |e| {
            let column = e.project().default_type(Category::Column).unwrap();
            let beam = e.project().default_type(Category::Beam).unwrap();
            let ty = cc_core::ElementTypeId(e.alloc_id());
            e.add_type(cc_core::truss::host_type(ty, "Truss".into()));
            let mut t = cc_core::truss::Truss::new(beam, beam, column);
            (t.copies, t.spacing, t.base_offset) = (2, 3.0, 2.0);
            let id = ElementId(e.alloc_id());
            e.add_element(Element {
                id,
                level,
                type_id: ty,
                mark: String::new(),
                comments: String::new(),
                uid: Default::default(),
                kind: ElementKind::Truss(Box::new(t)),
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
        assert_eq!(lines, 2);
    }
```

(Only the count is asserted: the span line lies on the chords' axes, so a pick there may hit a chord first; the host is picked where its line is drawn alone, which depends on the chord width. If you want a pick assertion, use `d.hit_test` at a point on the span line and accept either the host or a member.)

In `crates/cc_engine/tests/truss.rs`:

```rust
#[test]
fn ifc_export_writes_the_members_and_not_the_truss() {
    let mut s = Session::default();
    let host = id(&run(&mut s, "truss_create", json!({"start": SPAN[0], "end": SPAN[1], "height": 1.0, "copies": 2})));
    let text = cc_ifc::export(s.project(), &cc_ifc::ExportOptions::default());
    let p = s.project();
    let count = |f: fn(&ElementKind) -> bool| p.elements.values().filter(|e| f(&e.kind)).count();
    assert_eq!(text.matches("IFCCOLUMN(").count(), count(|k| matches!(k, ElementKind::Column(_))));
    assert_eq!(text.matches("IFCBEAM(").count(), count(|k| matches!(k, ElementKind::Beam(_))));
    let uid = cc_ifc::guid::encode(p.element(ElementId(host)).unwrap().uid.0);
    assert!(!text.contains(&uid), "the truss itself is not exported");
}
```

(Mirror whatever the committed `ifc_export_writes_the_members_and_not_the_building` in `tests/steel_building.rs` asserts; if it was adapted to `IFCMEMBER` for cold-formed sections, adapt this one the same way.)

- [ ] **Step 2: Run them.** `cargo nextest run -p cc_views trusses_draw` (FAILS: 0 lines) and `cargo nextest run -p cc_engine --test main ifc_export_writes_the_members_and_not_the_truss` (likely passes already; keep it as the regression test).

- [ ] **Step 3: Draw the span lines** in `plan.rs`:

```rust
/// Trusses of a plan's level: each copy's span line, thin and dashed, as
/// the truss's own line so it can be picked (its members draw themselves).
fn draw_trusses(d: &mut Drawing2D, project: &Project, level: LevelId) {
    for e in project.elements.values().filter(|e| e.level == level) {
        let ElementKind::Truss(t) = &e.kind else { continue };
        for [a, b] in t.span_lines() {
            d.line(a, b, LineStyle::Hidden, Some(e.id));
        }
    }
}
```

and in `plan_of`:

```rust
    if !ceiling {
        draw_steel_buildings(&mut d, project, level);
        draw_trusses(&mut d, project, level);
    }
```

- [ ] **Step 4: Run the tests.** `cargo nextest run -p cc_views plan` and `cargo nextest run -p cc_engine --test main truss`. Expected: pass.

- [ ] **Step 5: Clippy, fmt, commit.** `cargo clippy -p cc_views -p cc_engine --all-targets -- -D warnings`, `cargo fmt --all`, then:

```bash
git add -A
git commit -s -m "Draw trusses' span lines in plans and leave the host out of IFC"
```

---

### Task 6: The Truss tool, palette rows and *Reset Members* (cc_app)

**Files:**
- Create: `crates/cc_app/src/member_host_ui.rs`, `crates/cc_app/src/truss_ui.rs`
- Modify:
  - `crates/cc_app/src/main.rs`: `mod member_host_ui;` (alphabetical, after the `mass…`/`material…` modules) and `mod truss_ui;` after `mod tools;`.
  - `crates/cc_app/src/tools.rs`:
    - `Tool::Truss { material: cc_core::StructuralMaterial }` (doc: `/// Click the two ends of a truss (options in `AppState::truss`) of the Structure tab's material.`);
    - remove `Tool::Truss` from `type_id`, `category` and `with_type` (they fall to `_ => None` / the default arm; if `with_type` has no catch-all, add `Tool::Truss { material } => Tool::Truss { material },`);
    - `TrussOptions` rewritten, `TRUSS_PATTERNS` removed, `truss_command` added, `truss_tool` rewritten; the canvas dispatch (~1581) becomes `(Tool::Truss { material }, Some(level)) => truss_tool(ui, resp, painter, state, level, *material, xf),`.
  - `crates/cc_app/src/ribbon.rs`: the Truss options bar (~1280) rewritten (Step 5); the timber framing group `&[&[ColumnsAtGrids, BeamsOnGrids, BeamsBetweenColumns], &[GridFrame, Truss]]` (the steel group already lists `Truss`).
  - `crates/cc_app/src/commands.rs`: `run` arm `Truss` (~1925) and availability (~1480) (Step 6).
  - `crates/cc_app/src/properties.rs`: `ElementKind::Truss(t) => crate::truss_ui::props(ui, state, id, t),` next to the `SteelBuilding` arm (replacing Task 2's `{}`).
  - `crates/cc_app/src/steel_building_ui.rs`: use `member_host_ui::{finished, length, member_rows}` (Step 4).
  - `assets/i18n/*.ftl` (12): `truss-king-post`, `truss-queen-post`, `truss-fink`, `prop-truss-pattern`, `prop-truss-copies`, `label-top-chord-type`, `label-bottom-chord-type`, `label-web-type`, `label-post-type`, `msg-truss-material`; rewrite `cmd-desc-truss`.
- Test: `crates/cc_app/src/ui_tests.rs`.

**Interfaces:**
- Consumes: `TrussCreate`, `truss_rows` (Task 4); `cc_core::truss::*` (Task 2); `cc_core::member_host::MemberHost` (Task 1); `grid_frame_ui::combo`; `engine_edit::element`; `properties::{key, length_drag}`; `table::{row, section, readonly_row}`; `Session::run`; `state.prefs.structure_material`; `snap_at`, `grid_crossings`, `draw_snap`, `temp_dimension`, `PREVIEW` (the old `truss_tool` uses them).
- Produces:
  - `pub struct TrussOptions { pub shape: TrussShape, pub pattern: TrussPattern, pub height: f64, pub panels: u32 /* 0 = auto */, pub copies: u32, pub spacing: f64 }` + `Default`.
  - `pub fn truss_command(o: &TrussOptions, material: StructuralMaterial, level: LevelId, a: DVec2, b: DVec2) -> TrussCreate`.
  - `pub fn patterns_for(shape: TrussShape) -> Vec<TrussPattern>` (in `truss_ui.rs`).
  - `member_host_ui::{finished(&Response) -> bool, length(&mut Ui, &str, &mut f64, RangeInclusive<f64>) -> bool, member_rows(&mut Ui, &dyn MemberHost) -> bool}`.
  - `truss_ui::{edit(state, id, field, f: impl FnOnce(&mut Truss)), props(ui, state, id, &Truss), tool_options(ui, state)}`.

- [ ] **Step 1: Write the failing UI tests** in `ui_tests.rs`:

```rust
/// The Truss tool shows for steel and timber (not concrete) and starts with
/// the tab's material.
#[test]
fn truss_tool_follows_the_structure_material() {
    use cc_core::StructuralMaterial as M;
    for (material, shown) in [(M::Concrete, false), (M::Steel, true), (M::Timber, true)] {
        let mut state = sample_state();
        state.ribbon_tab = RibbonTab::Structure;
        state.prefs.structure_material = material;
        let mut harness = ribbon_harness(state, 1600.0);
        let label = crate::commands::UiCommand::Truss.label(harness.state());
        assert_eq!(harness.query_by_label(&label).is_some(), shown, "{material:?}");
        if shown {
            harness.get_by_role_and_label(Role::Button, &label).click();
            harness.run();
            assert_eq!(harness.state().tool, crate::tools::Tool::Truss { material });
        }
    }
}

/// The options the tool runs with make a parametric truss of the tab's material.
#[test]
fn truss_tool_options_make_a_parametric_truss() {
    use cc_core::truss::{TrussPattern, TrussShape};
    let mut state = sample_state();
    state.truss = crate::tools::TrussOptions {
        shape: TrussShape::Gable,
        pattern: TrussPattern::KingPost,
        height: 1.5,
        panels: 4,
        copies: 3,
        spacing: 1.2,
    };
    let level = state.project().levels[0].id;
    let (a, b) = (glam::DVec2::new(100.0, 100.0), glam::DVec2::new(106.0, 100.0));
    let command = crate::tools::truss_command(&state.truss, cc_core::StructuralMaterial::Timber, level, a, b);
    let out = state.session.run(command).expect("truss");
    let host = cc_core::ElementId(out["id"].as_u64().unwrap());
    let cc_core::ElementKind::Truss(t) = &state.project().element(host).unwrap().kind else { panic!("a truss") };
    assert_eq!((t.shape, t.pattern, t.panels, t.copies), (TrussShape::Gable, TrussPattern::KingPost, 4, 3));
    assert_eq!(t.members.len(), 33);
    assert!(state.project().types.values().any(|x| x.name == "Peça 6x16"), "timber chords");
}

/// A truss's rows edit it through `element_set`: new panels regenerate it,
/// and *Reset Members* brings back a deleted post.
#[test]
fn truss_properties_regenerate_it() {
    let mut state = sample_state();
    let args = serde_json::json!({"start": [100, 100], "end": [106, 100], "height": 1.0, "panels": 4});
    let out = state.session.execute("truss_create", args).unwrap();
    let host = cc_core::ElementId(out["id"].as_u64().unwrap());
    let get = |s: &AppState| match &s.project().element(host).unwrap().kind {
        cc_core::ElementKind::Truss(t) => (**t).clone(),
        _ => panic!("not a truss"),
    };
    // The path the palette's rows take.
    crate::truss_ui::edit(&mut state, host, "panels", |t| t.panels = 6);
    assert_eq!(get(&state).members.len(), 12 + 5 + 6);
    let post = get(&state).members.iter().find(|m| m.role.kind == cc_core::truss::TrussRoleKind::Vertical).unwrap().id;
    state.session.execute("elements_delete", serde_json::json!({"ids": [post.0]})).unwrap();
    assert_eq!(get(&state).detached.len(), 1);
    state.selection = [host].into_iter().collect();
    let mut harness = themed_harness(state, egui::vec2(360.0, 1600.0), egui::Theme::Light, |ui, state| {
        crate::properties::show(ui, state);
    });
    harness.run();
    assert!(harness.query_by_label(&t!("prop-truss-pattern")).is_some());
    harness.get_by_label(&t!("action-reset-members")).click();
    harness.run();
    let t = get(harness.state());
    assert_eq!((t.members.len(), t.detached.len()), (23, 0));
}
```

(`ribbon_harness`, `themed_harness`, `sample_state` and `Role` are the helpers the steel and Structure-tab tests use. If `state.tool` is named differently, use the field `set_tool` writes.)

- [ ] **Step 2: Run them to see them fail.** `cargo nextest run -p cc_app truss`. Expected: compile errors (`Tool::Truss { material }`, `TrussOptions` fields, `truss_command`, `truss_ui` missing).

- [ ] **Step 3: Write `member_host_ui.rs`:**

```rust
//! Rows shared by the properties of member hosts (steel buildings,
//! trusses): the member counts and *Reset Members*, and length rows that
//! report when an edit ends.

use crate::t;
use crate::table::{readonly_row, row};
use cc_core::member_host::MemberHost;
use egui::Ui;
use std::ops::RangeInclusive;

/// Whether a drag or a typed value ended.
pub(crate) fn finished(r: &egui::Response) -> bool {
    r.drag_stopped() || r.lost_focus()
}

/// A length row; true when the edit ended.
pub(crate) fn length(ui: &mut Ui, label: &str, v: &mut f64, range: RangeInclusive<f64>) -> bool {
    finished(&row(ui, label, |ui| ui.add(crate::properties::length_drag(v).range(range))))
}

/// The members and detached counts and *Reset Members* (enabled when a
/// role is detached); true when it was clicked: the caller clears the
/// host's detached roles in its own edit.
pub(crate) fn member_rows(ui: &mut Ui, host: &dyn MemberHost) -> bool {
    readonly_row(ui, &t!("prop-members"), host.member_list().len().to_string());
    let detached = host.detached_roles().len();
    readonly_row(ui, &t!("prop-detached-members"), detached.to_string());
    ui.add_enabled(detached > 0, egui::Button::new(t!("action-reset-members"))).clicked()
}
```

- [ ] **Step 4: Share it with the steel building.** In `steel_building_ui.rs`: delete its own `finished` and `length` (import `crate::member_host_ui::{finished, length}`; keep using them in the dialog) and replace

```rust
        readonly_row(ui, &t!("prop-members"), building.members.len().to_string());
        readonly_row(ui, &t!("prop-detached-members"), building.detached.len().to_string());
        let reset = ui.add_enabled(!building.detached.is_empty(), egui::Button::new(t!("action-reset-members")));
        if reset.clicked() {
            b.detached.clear();
            done = true;
        }
```

with

```rust
        if crate::member_host_ui::member_rows(ui, building) {
            b.detached.clear();
            done = true;
        }
```

(`building: &SteelBuilding` coerces to `&dyn MemberHost`.) Drop `readonly_row` from its imports if unused. `cargo nextest run -p cc_app steel_building` must stay green.

- [ ] **Step 5: Tool, options and command builder** in `tools.rs` (replacing `TrussOptions`, `TRUSS_PATTERNS` and `truss_tool`):

```rust
/// Truss tool options, kept between uses.
#[derive(Debug, Clone, PartialEq)]
pub struct TrussOptions {
    pub shape: cc_core::truss::TrussShape,
    pub pattern: cc_core::truss::TrussPattern,
    /// Chord to chord (flat), or the rise (mono, gable), meters.
    pub height: f64,
    /// Number of panels; 0 picks about one per 1.5 m.
    pub panels: u32,
    pub copies: u32,
    /// Copy to copy, meters.
    pub spacing: f64,
}

impl Default for TrussOptions {
    fn default() -> Self {
        Self {
            shape: Default::default(),
            pattern: Default::default(),
            height: cc_core::truss::DEFAULT_HEIGHT,
            panels: 0,
            copies: 1,
            spacing: cc_core::truss::DEFAULT_SPACING,
        }
    }
}

/// The command the tool runs for a truss from `a` to `b`.
pub fn truss_command(
    o: &TrussOptions,
    material: cc_core::StructuralMaterial,
    level: LevelId,
    a: DVec2,
    b: DVec2,
) -> cc_engine::commands::framing::TrussCreate {
    // Auto panels on a gable: the engine's default is even already.
    let panels = (o.panels > 0).then(|| {
        if o.shape == cc_core::truss::TrussShape::Gable && o.panels % 2 == 1 { o.panels + 1 } else { o.panels }
    });
    cc_engine::commands::framing::TrussCreate {
        start: a.to_array(),
        end: b.to_array(),
        height: o.height,
        panels,
        pattern: Some(o.pattern.code().into()),
        shape: Some(o.shape.code().into()),
        copies: Some(o.copies),
        spacing: Some(o.spacing),
        material: Some(material.code().into()),
        level: Some(Ref::Id(level.0)),
        ..Default::default()
    }
}

/// Two clicks: the ends of a truss, laid out with the options bar's values
/// and the Structure tab's material; the new truss is selected.
fn truss_tool(
    ui: &Ui,
    resp: &Response,
    painter: &Painter,
    state: &mut AppState,
    level: LevelId,
    material: cc_core::StructuralMaterial,
    xf: &Xf,
) {
    let Some(raw) = resp.hover_pos().map(|p| xf.to_world(p)) else { return };
    let shift = ui.input(|i| i.modifiers.shift);
    let crossings = grid_crossings(state.project());
    let anchor = state.interaction.anchor;
    let s = snap_at(state, Some(level), raw, anchor, &crossings, xf.scale, shift);
    draw_snap(painter, xf, s);
    if let Some(a) = anchor {
        painter.line_segment([xf.to_screen(a), xf.to_screen(s.point)], Stroke::new(1.5, PREVIEW));
        temp_dimension(painter, xf, a, s.point, None, None, 0.1);
    }
    if resp.clicked() {
        match anchor {
            None => state.interaction.anchor = Some(s.point),
            Some(a) if a.distance(s.point) >= *cc_core::truss::LENGTH.start() => {
                let command = truss_command(&state.truss, material, level, a, s.point);
                if let Some(out) = crate::actions::run_command(state, command)
                    && let Some(id) = out["id"].as_u64()
                {
                    state.selection = [ElementId(id)].into_iter().collect();
                }
                state.interaction.anchor = None;
            }
            _ => {}
        }
    }
}
```

In `ribbon.rs`, replace the `Tool::Truss { .. } => { .. }` options block with `Tool::Truss { .. } => crate::truss_ui::tool_options(ui, state),`.

- [ ] **Step 6: The ribbon command.** In `commands.rs`:
  - `run`:

```rust
        Truss => {
            let material = state.prefs.structure_material;
            if material != cc_core::StructuralMaterial::Concrete {
                state.set_tool(Tool::Truss { material });
                state.message = t!("hint-truss");
            }
        }
```

  - availability: replace `Truss if project.default_type(Category::Beam).is_none() => Some(t!("msg-no-type")),` with `Truss if state.prefs.structure_material == cc_core::StructuralMaterial::Concrete => Some(t!("msg-truss-material")),` (check how the surrounding arms reach `state`; if the function only has `project`, add the material as the steel building's availability does, or read it from the `state` parameter the function already takes).
  - In `ribbon.rs::structure_tab` timber: `stacks(ui, state, &[&[ColumnsAtGrids, BeamsOnGrids, BeamsBetweenColumns], &[GridFrame, Truss]]);`.

- [ ] **Step 7: Write `truss_ui.rs`:**

```rust
//! Trusses in the app: the Truss tool's options bar and the properties of a
//! selected truss (every parameter through `element_set`, which regenerates
//! its members in the same undo step).

use crate::grid_frame_ui::combo;
use crate::member_host_ui::{finished, length, member_rows};
use crate::state::AppState;
use crate::t;
use crate::table::{readonly_row, row, section};
use cc_core::truss::{self as tr, Truss, TrussPattern, TrussShape};
use cc_core::{Category, ElementId, ElementKind, ElementTypeId};
use egui::{DragValue, Ui};

/// The patterns offered for a shape (gable-only ones on gables).
pub fn patterns_for(shape: TrussShape) -> Vec<TrussPattern> {
    TrussPattern::ALL.into_iter().filter(|p| !p.needs_gable() || shape == TrussShape::Gable).collect()
}

fn shapes() -> Vec<(TrussShape, String)> {
    TrussShape::ALL.into_iter().map(|s| (s, crate::i18n::tr(s.key()))).collect()
}

fn patterns(shape: TrussShape) -> Vec<(TrussPattern, String)> {
    patterns_for(shape).into_iter().map(|p| (p, crate::i18n::tr(p.key()))).collect()
}

/// `panels` after an edit: even on a gable, stepping the way it moved from `before`.
fn even_on_gable(shape: TrussShape, panels: u32, before: u32) -> u32 {
    if shape != TrussShape::Gable || panels % 2 == 0 {
        return panels;
    }
    let stepped = if panels < before { panels.saturating_sub(1) } else { panels + 1 };
    stepped.clamp(*tr::PANELS.start(), *tr::PANELS.end())
}

/// The options bar of the Truss tool.
pub fn tool_options(ui: &mut Ui, state: &mut AppState) {
    let o = &mut state.truss;
    ui.label(t!("prop-shape"));
    combo(ui, "truss-shape", &mut o.shape, &shapes());
    let offered = patterns(o.shape);
    if !offered.iter().any(|(p, _)| *p == o.pattern) {
        o.pattern = TrussPattern::Pratt;
    }
    ui.label(t!("prop-truss-pattern"));
    combo(ui, "truss-pattern", &mut o.pattern, &offered);
    ui.label(t!("prop-truss-height"));
    ui.add(crate::properties::length_drag(&mut o.height).speed(0.01).range(tr::HEIGHT));
    ui.label(t!("prop-truss-panels"));
    ui.add(
        DragValue::new(&mut o.panels)
            .range(0..=*tr::PANELS.end())
            .custom_formatter(|n, _| if n < 0.5 { t!("truss-panels-auto") } else { format!("{n:.0}") }),
    );
    ui.label(t!("prop-truss-copies"));
    ui.add(DragValue::new(&mut o.copies).range(tr::COPIES));
    ui.label(t!("label-spacing"));
    ui.add(crate::properties::length_drag(&mut o.spacing).speed(0.01).range(tr::SPACING));
    ui.separator();
    ui.label(crate::theme::muted(t!("hint-truss")));
}

/// Change truss `id` as `f` changes a copy of it (`element_set`, which
/// regenerates its members); edits under one `field` merge while dragging.
pub fn edit(state: &mut AppState, id: ElementId, field: &str, f: impl FnOnce(&mut Truss)) {
    crate::engine_edit::element(state, id, Some(crate::properties::key((id, field))), |e| {
        if let ElementKind::Truss(t) = &mut e.kind {
            f(t);
        }
    });
}

/// The rows of a selected truss.
pub fn props(ui: &mut Ui, state: &mut AppState, id: ElementId, truss: &Truss) {
    let mut t = truss.clone();
    let mut done = false;
    let all = |c: Category| -> Vec<(ElementTypeId, String)> {
        state.project().types_of(c).map(|x| (x.id, x.name.clone())).collect()
    };
    let (columns, beams) = (all(Category::Column), all(Category::Beam));
    section(ui, "truss", t!("prop-truss"), |ui| {
        let shape = t.shape;
        row(ui, &t!("prop-shape"), |ui| combo(ui, &format!("truss-shape-{}", id.0), &mut t.shape, &shapes()));
        if t.shape != shape {
            t.panels = even_on_gable(t.shape, t.panels, t.panels);
            done = true;
        }
        let pattern = t.pattern;
        let offered = patterns(t.shape);
        row(ui, &t!("prop-truss-pattern"), |ui| {
            combo(ui, &format!("truss-pattern-{}", id.0), &mut t.pattern, &offered)
        });
        done |= t.pattern != pattern;
        done |= length(ui, &t!("prop-truss-height"), &mut t.height, tr::HEIGHT);
        let panels = t.panels;
        done |= finished(&row(ui, &t!("prop-truss-panels"), |ui| {
            ui.add(DragValue::new(&mut t.panels).range(tr::PANELS))
        }));
        t.panels = even_on_gable(t.shape, t.panels, panels);
        done |= length(ui, &t!("prop-base-offset"), &mut t.base_offset, -1e4..=1e4);
        readonly_row(ui, &t!("prop-length"), crate::format::length(truss.length()));
        done |= finished(&row(ui, &t!("prop-truss-copies"), |ui| {
            ui.add(DragValue::new(&mut t.copies).range(tr::COPIES))
        }));
        done |= length(ui, &t!("label-spacing"), &mut t.spacing, tr::SPACING);
        if member_rows(ui, truss) {
            t.detached.clear();
            done = true;
        }
    });
    section(ui, "truss-types", t!("section-type"), |ui| {
        let before = (t.top_chord_type, t.bottom_chord_type, t.web_type, t.post_type);
        row(ui, &t!("label-top-chord-type"), |ui| {
            combo(ui, &format!("truss-top-{}", id.0), &mut t.top_chord_type, &beams)
        });
        row(ui, &t!("label-bottom-chord-type"), |ui| {
            combo(ui, &format!("truss-bottom-{}", id.0), &mut t.bottom_chord_type, &beams)
        });
        row(ui, &t!("label-web-type"), |ui| combo(ui, &format!("truss-web-{}", id.0), &mut t.web_type, &beams));
        row(ui, &t!("label-post-type"), |ui| combo(ui, &format!("truss-post-{}", id.0), &mut t.post_type, &columns));
        done |= (t.top_chord_type, t.bottom_chord_type, t.web_type, t.post_type) != before;
    });
    if t != *truss {
        edit(state, id, "truss", move |x| *x = t);
    }
    if done {
        state.session.doc.break_merge();
    }
}
```

Adapt to the real API:
- The length display: use what other palettes use for a read-only length (`rg -n "readonly_row\(ui, &t!\(\"prop-length\"\)" crates/cc_app/src`); if there is no `crate::format::length`, use `cc_views::format` the same way those rows do, else `format!("{:.3}", truss.length())`.
- `DragValue::range` takes a `RangeInclusive` of the value's type; `tr::HEIGHT` and friends are `const`s, so passing one copies it.
- `combo` is `pub(crate)` in `grid_frame_ui.rs` (the steel building made it so).
- `crate::theme::muted` is what the old options bar used (`theme::muted`); keep its exact path from `ribbon.rs`.

- [ ] **Step 8: i18n.** Add the 10 new keys from the table and `msg-truss-material` to the 12 files under `# Parametric trusses`; replace each file's `cmd-desc-truss` line with the new text.

- [ ] **Step 9: Run the tests.** `cargo check --tests -p cc_app`, then `cargo nextest run -p cc_app truss steel_building structure_material i18n icons ribbon`. Expected: pass.

If the properties harness does not find *Reset Members* (the palette is taller than 1600 px), collapse the type section in the test by clicking its header first, or query by role and label; do not change the layout for the test.

- [ ] **Step 10: Clippy, fmt, commit.** `cargo clippy -p cc_app --all-targets -- -D warnings`, `cargo fmt --all`, then:

```bash
git add -A
git commit -s -m "Truss tool options, truss properties and shared Reset Members"
```

---

### Task 7: Documentation and full check

**Files:**
- Modify:
  - `AGENTS.md`:
    - in the **Steel buildings** bullet, replace the sync sentence's names: "`EditCtx::finish` → `sync_member_hosts` (`cc_core::member_host`, shared with trusses) ..." and "`remove_element` takes the members along (`Project::host_members_of`) and `copy_set` leaves them out; `repair_member_hosts`"; mention that `SteelBuilding` implements `MemberHost`;
    - a new **Member hosts and trusses** bullet right after it: `cc_core::member_host` (`MemberHost`: member types, layout, member list and detached roles as `RoleKey`s whose order is the typed roles', `role_fits`, `check_parameters`, `repair_parameters`; `ElementKind::member_host`/`member_host_mut`; the generic `sync_member_hosts`, `repair_member_hosts`, `host_members_of`); `cc_core::truss` (`ElementKind::Truss(Truss)`, category `Truss`, `TypeData::Truss {}` created with the first one: `start`/`end` span line, `base_offset` of the bottom chord axis, `TrussShape` flat/mono/gable with `rise`, `height`, `TrussPattern` Pratt/Howe/Warren/king post/queen post/Fink with `pattern_used` falling back to Pratt, `panels` 2–64 even on a gable, top/bottom chord, web and post types, `copies` and `spacing` to the left; `layout` makes per-panel chord beams and diagonals (`BeamVertical::Center`) and post columns, `TrussRole`/`TrussRoleKind`, `MIN_VERTICAL`, `MAX_MEMBERS`; a mirror maps the copies); `cc_engine::commands::framing` `truss_create` (`material` steel or timber, `truss_rows` through `structure_material::catalog_type`/`type_or_row`, one undo step, `{id, members}`; `element_get` `derived.members`/`detached`/`pattern_used`); plans draw each copy's span line dashed (`plan::draw_trusses`), IFC leaves the host out; the app's `Tool::Truss { material }` (steel and timber framing groups, `TrussOptions`, `truss_command`), `truss_ui.rs` (options bar, palette rows) and `member_host_ui.rs` (shared counts and *Reset Members*).
  - `docs/interface.md`, `docs/interface.pt-BR.md`: in the Structure tab paragraph, timber's grid group also has Truss; then a paragraph on the Truss tool (two clicks; the options bar's shape, pattern, height, panels, copies, spacing; types from the tab's material), its properties, detaching and *Reset Members*.
  - `docs/automation.md`, `docs/automacao.md`: a short paragraph after the `steel_building_create` one, with `{"command": "truss_create", "args": {"start": [0, 0], "end": [12, 0], "height": 2, "shape": "gable", "pattern": "fink", "panels": 6, "copies": 5, "spacing": 1.2, "material": "timber"}}` and `{"command": "element_set", "args": {"id": 42, "params": {"panels": 8}}}` (prose in each file's language).
  - `CHANGELOG.md` under `## [Unreleased]`:
    - `### Added`: parametric trusses for steel and timber (flat, mono-pitch and gable outlines; Pratt, Howe, Warren, king post, queen post and Fink webs; copies at a spacing; chords, diagonals and posts as ordinary beams and columns regenerated with their ids kept, detached when edited by hand; *Structure → Framing → Truss* in the steel and timber groups with its options and properties, *Reset Members*);
    - `### Changed` (create the heading if missing): `truss_create` now makes a parametric truss element and returns `{id, members}` instead of a group of loose members (`created`, `group`, `name` are gone); default types come from the material's catalog rows.
- [ ] **Step 1: Write the docs** above, concise and in the style of the neighboring entries.
- [ ] **Step 2: Full check.**
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo nextest run --workspace`
  - `cargo test --workspace --doc`

  Expected: all green. `docs/commands.md` must match `cargo run -q -p cc_cli -- reference` (a test checks it). The `examples/graphs/house.ccgraph` and `examples/small_house.ccmacro` tests must still pass (if either calls `truss_create` and reads `created`, update it to `members`).
- [ ] **Step 3: Commit.**

```bash
git add -A
git commit -s -m "Document parametric trusses and member hosts"
```

---

## Self-review

- Spec coverage:
  - `MemberHost` trait in `cc_core::member_host`, `member_host()`/`member_host_mut()` and the dispatch by `ElementKind`, steel building ported with its tests unchanged: Task 1.
  - `Truss` host, category `Truss`, `TypeData::Truss {}`, every parameter with its bounds, Gable even panels, pattern fallback in `check`/`pattern_used`: Task 2.
  - Members (per-panel chords, sloped diagonals through `end_offset`, post columns from axis to axis, 0.05 m skip, copies to the left, `MAX_MEMBERS`): Task 2.
  - Geometry (panel points, top chord heights, webs per pattern, roles `{kind, copy, index}`): Task 2.
  - Shared sync, removal, copy, repair on a truss (create, change panels, detach, delete host, copy, undo): Tasks 1 and 3.
  - `truss_create` compatibility plus `shape`, new patterns, chord/post types, `copies`, `spacing`, `material`; `{id, members}`; breaking change and updated tests; material defaults through `catalog_type`; concrete refused: Task 4.
  - `element_get`/`element_set` of parameters, `members` refused, one undo step: Tasks 1 and 4.
  - Analytical model, schedules, IFC unchanged (host skipped): Tasks 4 (analytical test kept) and 5 (IFC test).
  - Plans draw dashed span lines per copy: Task 5.
  - App: tool in steel and timber groups, two clicks, options bar (shape, pattern, height, panels, copies, spacing), the tab's material, palette rows and *Reset Members* shared with the building, 12 languages: Task 6.
  - Docs and full check: Task 7.
- Names used across tasks: `RoleKey`, `MemberHost` (`member_types`, `member_layout`, `member_list`, `set_member_list`, `detached_roles`, `set_detached_roles`, `role_fits`, `check_parameters`, `repair_parameters`), `host_members_of`, `sync_member_hosts`, `regenerate_members`, `repair_member_hosts`, `within`, `reset`, `fix_type`, `MemberRole::{key, from_key}`, `RoleKind::ALL`, `Truss`, `TrussShape`, `TrussPattern`, `TrussRoleKind`, `TrussRole`, `TrussMember`, `default_panels`, `host_type`, `span_lines`, `pattern_used`, `rise`, `TrussCreate`, `TrussRows`, `truss_rows`, `new_truss_type`, `type_or_row`, `framing::derived`, `draw_trusses`, `TrussOptions`, `truss_command`, `truss_ui::{tool_options, edit, props, patterns_for}`, `member_host_ui::{finished, length, member_rows}`.
- Decisions where the spec was open: listed under Global Constraints.
