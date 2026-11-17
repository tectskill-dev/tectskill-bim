# Structure Tab by Material Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split the Structure ribbon tab by material (Concrete / Steel / Timber), give column and beam tools types of the selected material (creating a catalog default when missing), and add a multi-storey grid frame command with optional secondary beams.

**Architecture:**
- `cc_core` learns the structural material of a column or beam type.
- `cc_engine` gains two edit commands: `structural_type_for` (find or make a type) and `grid_frame_create` (columns and beams over a range of levels, in one undo step).
- `cc_app` adds:
  - a selector group to the Structure tab;
  - a material filter on the column and beam tools;
  - a grid frame dialog.

**Tech Stack:** Rust 2024 workspace. egui/eframe app with egui_kittest UI tests. Fluent i18n in 12 languages. cargo-nextest.

**Spec:** `docs/superpowers/specs/2026-11-12-structure-tab-materials-design.md`

## Global Constraints

- Work only in the worktree `D:\constructcad-structab` (branch `feat/structure-tab-materials`). Never touch `D:\constructcad`.
- Never push, never run `gh`, never merge. Commit with `git commit -s`. No Claude attribution or Co-Authored-By.
- Non-test code never panics: no `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!` or `unimplemented!` (clippy denies them). No `unsafe`.
- Every new interface or transaction text is an i18n key added to all 12 files in `assets/i18n/`: `pt-BR`, `en`, `es`, `fr`, `de`, `it`, `lo`, `pl`, `uk-UA`, `zh-TW`, `zh-CN`, `ja`. Tests fail otherwise.
- Every engine command parameter has a doc comment. Register the command in `crates/cc_engine/src/registry.rs` (`registry!`). Regenerate the reference with `cargo run -p cc_cli -- reference > docs/commands.md`; a test checks it.
- Integration tests are one binary: a new file in `crates/cc_engine/tests/` needs `mod <name>;` in `crates/cc_engine/tests/main.rs`.
- Tests run with cargo-nextest: `cargo nextest run -p <crate> <filter>`. Use `cargo check --tests -p <crate>` for type errors.
- Formatting is rustfmt with `max_width = 120`: `cargo fmt --all`. Clippy: `cargo clippy --workspace --all-targets -- -D warnings`.
- Code, comments and identifiers are in English.
- Defaults (verbatim from the spec):
  - Steel: catalog `W` row `W200x26,6`.
  - Timber: catalog `Glulam` row `MLC 9x32`.
  - Concrete: a `rect` profile 0.20 x 0.40 named `20x40`.
  - `MAX_MEMBERS` = 2000.
  - Minimum secondary spacing: 0.3 m.
  - Parallel tolerance: 1°.
  - Secondary count per bay: `ceil(width / spacing) - 1`.

## i18n keys (used by Tasks 2–7)

Each task that uses a key adds it to all 12 files, appended under a comment `# Structure by material`. The translations:

| key | pt-BR | en | es | fr | de | it | lo | pl | uk-UA | zh-TW | zh-CN | ja |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| tx-structural-type | Criar tipo estrutural | Create structural type | Crear tipo estructural | Créer un type structurel | Tragwerkstyp erstellen | Crea tipo strutturale | ສ້າງປະເພດໂຄງສ້າງ | Utwórz typ konstrukcyjny | Створити конструктивний тип | 建立結構類型 | 创建结构类型 | 構造タイプを作成 |
| tx-grid-frame | Criar estrutura em grelha | Create grid frame | Crear estructura en retícula | Créer une ossature sur la trame | Rasterskelett erstellen | Crea telaio sulla griglia | ສ້າງໂຄງສ້າງຕາມກຣິດ | Utwórz szkielet na siatce osi | Створити каркас за сіткою осей | 建立軸網框架 | 创建轴网框架 | 通り芯フレームを作成 |
| group-structure-material | Material | Material | Material | Matériau | Material | Materiale | ວັດສະດຸ | Materiał | Матеріал | 材料 | 材料 | 材料 |
| action-grid-frame | Estrutura em grelha | Grid Frame | Estructura en retícula | Ossature sur trame | Rasterskelett | Telaio su griglia | ໂຄງສ້າງຕາມກຣິດ | Szkielet na siatce | Каркас за сіткою | 軸網框架 | 轴网框架 | 通り芯フレーム |
| dlg-grid-frame | Estrutura em grelha | Grid Frame | Estructura en retícula | Ossature sur trame | Rasterskelett | Telaio su griglia | ໂຄງສ້າງຕາມກຣິດ | Szkielet na siatce | Каркас за сіткою | 軸網框架 | 轴网框架 | 通り芯フレーム |
| label-base-level | Nível base | Base level | Nivel base | Niveau de base | Basisebene | Livello di base | ລະດັບຖານ | Poziom bazowy | Базовий рівень | 基準樓層 | 基准标高 | 基準レベル |
| label-top-level | Nível de topo | Top level | Nivel superior | Niveau supérieur | Oberste Ebene | Livello superiore | ລະດັບເທິງ | Poziom górny | Верхній рівень | 頂部樓層 | 顶部标高 | 上端レベル |
| label-column-type | Tipo de pilar | Column type | Tipo de pilar | Type de poteau | Stützentyp | Tipo di pilastro | ປະເພດເສົາ | Typ słupa | Тип колони | 柱類型 | 柱类型 | 柱タイプ |
| label-beam-type | Tipo de viga | Beam type | Tipo de viga | Type de poutre | Trägertyp | Tipo di trave | ປະເພດຄານ | Typ belki | Тип балки | 梁類型 | 梁类型 | 梁タイプ |
| label-secondary-beams | Vigas secundárias | Secondary beams | Vigas secundarias | Poutres secondaires | Nebenträger | Travi secondarie | ຄານຮອງ | Belki drugorzędne | Другорядні балки | 次梁 | 次梁 | 小梁 |
| label-parallel-to | Paralelas a | Parallel to | Paralelas a | Parallèles à | Parallel zu | Parallele a | ຂະໜານກັບ | Równoległe do | Паралельно до | 平行於 | 平行于 | 平行 |
| label-spacing | Espaçamento | Spacing | Separación | Espacement | Abstand | Interasse | ໄລຍະຫ່າງ | Rozstaw | Крок | 間距 | 间距 | 間隔 |

Two longer keys:

`cmd-desc-grid-frame`:
- pt-BR: Coloca pilares nas interseções dos eixos e vigas ao longo dos eixos em cada pavimento de um intervalo de níveis, com vigas secundárias opcionais, em uma única etapa.
- en: Place columns at the grid intersections and beams along the grids on every storey of a range of levels, with optional secondary beams, in one step.
- es: Coloca pilares en las intersecciones de los ejes y vigas a lo largo de los ejes en cada planta de un rango de niveles, con vigas secundarias opcionales, en un solo paso.
- fr: Place des poteaux aux intersections de la trame et des poutres le long des axes à chaque étage d'une plage de niveaux, avec des poutres secondaires facultatives, en une seule étape.
- de: Setzt Stützen an die Rasterschnittpunkte und Träger entlang der Achsen in jedem Geschoss eines Ebenenbereichs, auf Wunsch mit Nebenträgern, in einem Schritt.
- it: Posiziona pilastri alle intersezioni della griglia e travi lungo gli assi su ogni piano di un intervallo di livelli, con travi secondarie facoltative, in un solo passaggio.
- lo: ວາງເສົາຢູ່ຈຸດຕັດຂອງເສັ້ນກຣິດ ແລະ ຄານຕາມເສັ້ນກຣິດໃນທຸກຊັ້ນຂອງຊ່ວງລະດັບ, ພ້ອມຄານຮອງຖ້າຕ້ອງການ, ໃນຂັ້ນຕອນດຽວ.
- pl: Umieszcza słupy na przecięciach osi i belki wzdłuż osi na każdej kondygnacji zakresu poziomów, opcjonalnie z belkami drugorzędnymi, w jednym kroku.
- uk-UA: Розміщує колони на перетинах осей і балки вздовж осей на кожному поверсі діапазону рівнів, за потреби з другорядними балками, за один крок.
- zh-TW: 在一段樓層範圍的每一層，於軸網交點放置柱、沿軸線放置梁，可選擇加入次梁，一次完成。
- zh-CN: 在一段标高范围的每一层，于轴网交点放置柱、沿轴线放置梁，可选加入次梁，一步完成。
- ja: レベル範囲の各階で、通り芯の交点に柱、通り芯に沿って梁を配置し、必要に応じて小梁も一度に配置します。

`msg-grid-frame-created` (placeholders `$columns`, `$beams`, `$secondary` and `$skipped`, identical in every language):
- pt-BR: { $columns } pilares, { $beams } vigas e { $secondary } vigas secundárias colocados; { $skipped } ignorados.
- en: { $columns } columns, { $beams } beams and { $secondary } secondary beams placed; { $skipped } skipped.
- es: { $columns } pilares, { $beams } vigas y { $secondary } vigas secundarias colocados; { $skipped } omitidos.
- fr: { $columns } poteaux, { $beams } poutres et { $secondary } poutres secondaires placés ; { $skipped } ignorés.
- de: { $columns } Stützen, { $beams } Träger und { $secondary } Nebenträger gesetzt; { $skipped } übersprungen.
- it: { $columns } pilastri, { $beams } travi e { $secondary } travi secondarie posizionati; { $skipped } saltati.
- lo: ວາງເສົາ { $columns }, ຄານ { $beams } ແລະ ຄານຮອງ { $secondary }; ຂ້າມ { $skipped }.
- pl: Umieszczono słupy: { $columns }, belki: { $beams }, belki drugorzędne: { $secondary }; pominięto: { $skipped }.
- uk-UA: Розміщено колон: { $columns }, балок: { $beams }, другорядних балок: { $secondary }; пропущено: { $skipped }.
- zh-TW: 已放置 { $columns } 根柱、{ $beams } 根梁及 { $secondary } 根次梁；略過 { $skipped } 個。
- zh-CN: 已放置 { $columns } 根柱、{ $beams } 根梁和 { $secondary } 根次梁；跳过 { $skipped } 个。
- ja: 柱 { $columns } 本、梁 { $beams } 本、小梁 { $secondary } 本を配置しました。{ $skipped } 件をスキップしました。

Material names reuse the existing keys `profile-material-concrete`, `profile-material-steel` and `profile-material-timber`. The steel and timber *Grid* group reuses `group-framing`. The disabled reason reuses `msg-grids-two`.

---

### Task 1: Structural material of a type (cc_core)

**Files:**
- Create: `crates/cc_core/src/structural_material.rs`
- Modify: `crates/cc_core/src/lib.rs`: add `pub mod structural_material;` and `pub use structural_material::StructuralMaterial;` next to the `structural_profile` lines.

**Interfaces:**
- Produces:
  - `cc_core::StructuralMaterial { Concrete (default), Steel, Timber }` with:
    - derives `Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default`;
    - `ALL: [Self; 3]`;
    - `code(self) -> &'static str` (`concrete`/`steel`/`timber`);
    - `from_code(&str) -> Option<Self>`;
    - `key(self) -> &'static str` (the `profile-material-*` keys);
    - `of_profile(ProfileMaterial) -> Self`.
  - `Project::structural_material(&self, &TypeData) -> Option<StructuralMaterial>`.
  - `Project::types_of_material(&self, Category, StructuralMaterial) -> impl Iterator<Item = &ElementType>`.

- [ ] **Step 1: Write the module with its failing tests**

```rust
//! The structural material of a column or beam type: concrete, steel
//! (rolled or cold-formed) or timber. It comes from the type's profile,
//! else from the type's own material (a legacy section that makes no
//! outline).

use crate::structural_profile::ProfileMaterial;
use crate::{Category, ElementType, Project, TypeData};

/// What a column or beam is made of, as the Structure tab groups its tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum StructuralMaterial {
    #[default]
    Concrete,
    Steel,
    Timber,
}

impl StructuralMaterial {
    pub const ALL: [StructuralMaterial; 3] = [Self::Concrete, Self::Steel, Self::Timber];

    pub fn code(self) -> &'static str {
        match self {
            Self::Concrete => "concrete",
            Self::Steel => "steel",
            Self::Timber => "timber",
        }
    }

    pub fn from_code(c: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.code().eq_ignore_ascii_case(c.trim()))
    }

    /// The i18n key of its name (shared with the profile materials).
    pub fn key(self) -> &'static str {
        match self {
            Self::Concrete => "profile-material-concrete",
            Self::Steel => "profile-material-steel",
            Self::Timber => "profile-material-timber",
        }
    }

    /// The structural material of a profile's: cold-formed steel is steel.
    pub fn of_profile(m: ProfileMaterial) -> Self {
        match m {
            ProfileMaterial::Steel | ProfileMaterial::ColdFormed => Self::Steel,
            ProfileMaterial::Timber => Self::Timber,
            ProfileMaterial::Concrete => Self::Concrete,
        }
    }
}

impl Project {
    /// The structural material of a column or beam type: its profile's,
    /// else its material's class (metal is steel, wood timber, the rest
    /// concrete). `None` for other types.
    pub fn structural_material(&self, data: &TypeData) -> Option<StructuralMaterial> {
        let (TypeData::Column { material, .. } | TypeData::Beam { material, .. }) = data else { return None };
        let profile = match self.type_profile_def(data) {
            Some(def) => def.material,
            None => self.profile_material_for(material),
        };
        Some(StructuralMaterial::of_profile(profile))
    }

    /// The types of `category` made of `material`, in id order.
    pub fn types_of_material(
        &self,
        category: Category,
        material: StructuralMaterial,
    ) -> impl Iterator<Item = &ElementType> + '_ {
        self.types_of(category).filter(move |t| self.structural_material(&t.data) == Some(material))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structural_profile::{ProfileDef, ProfileShape};
    use crate::{ColumnSection, MaterialClass, ProfileId, Uid};

    fn with_profile(p: &mut Project, material: ProfileMaterial) -> TypeData {
        let id = ProfileId(p.alloc_id());
        let shape = ProfileShape::Rect { b: 0.2, h: 0.4, chamfer: 0.0 };
        let name = format!("P{}", id.0);
        let def = ProfileDef {
            id,
            uid: Uid::generate(),
            name,
            material,
            shape,
            catalog: None,
            description: String::new(),
        };
        p.profiles.insert(id, def);
        TypeData::Column {
            section: ColumnSection::Rectangular { width: 0.2, depth: 0.4 },
            material: "none".into(),
            profile: Some(id),
            profile_rotation_deg: 0.0,
            profile_mirror: false,
        }
    }

    #[test]
    fn codes_round_trip() {
        for m in StructuralMaterial::ALL {
            assert_eq!(StructuralMaterial::from_code(m.code()), Some(m));
        }
        assert_eq!(StructuralMaterial::from_code(" STEEL "), Some(StructuralMaterial::Steel));
        assert_eq!(StructuralMaterial::from_code("gold"), None);
    }

    #[test]
    fn profiles_decide_the_material() {
        let mut p = Project::from_template();
        for (pm, want) in [
            (ProfileMaterial::Steel, StructuralMaterial::Steel),
            (ProfileMaterial::ColdFormed, StructuralMaterial::Steel),
            (ProfileMaterial::Timber, StructuralMaterial::Timber),
            (ProfileMaterial::Concrete, StructuralMaterial::Concrete),
        ] {
            let data = with_profile(&mut p, pm);
            assert_eq!(p.structural_material(&data), Some(want), "{pm:?}");
        }
        assert_eq!(p.structural_material(&TypeData::Grid { bubble: Default::default() }), None);
    }

    #[test]
    fn a_type_without_profile_reads_its_material_class() {
        let p = Project::from_template();
        let named = |class| p.materials.iter().find(|m| m.identity.class == class).map(|m| m.name.clone());
        let column = |material: String| TypeData::Column {
            section: ColumnSection::Rectangular { width: 0.2, depth: 0.2 },
            material,
            profile: None,
            profile_rotation_deg: 0.0,
            profile_mirror: false,
        };
        let metal = named(MaterialClass::Metal).expect("the template has a metal");
        assert_eq!(p.structural_material(&column(metal)), Some(StructuralMaterial::Steel));
        if let Some(wood) = named(MaterialClass::Wood) {
            assert_eq!(p.structural_material(&column(wood)), Some(StructuralMaterial::Timber));
        }
        assert_eq!(p.structural_material(&column("unknown".into())), Some(StructuralMaterial::Concrete));
    }

    #[test]
    fn template_types_by_material() {
        let p = Project::from_template();
        assert!(p.types_of_material(Category::Column, StructuralMaterial::Concrete).next().is_some());
        assert!(p.types_of_material(Category::Column, StructuralMaterial::Steel).next().is_some());
        assert!(p.types_of_material(Category::Beam, StructuralMaterial::Steel).next().is_some());
        assert!(p.types_of_material(Category::Column, StructuralMaterial::Timber).next().is_none());
    }
}
```

`TypeData::Grid { bubble: .. }` is only an example of a non-structural type. Use whatever non-column/beam `TypeData` variant compiles: look in `crates/cc_core/src/types.rs` for one with few fields, e.g. `TypeData::Room {}` or `TypeData::Shaft {}`.

- [ ] **Step 2: Register the module.** In `crates/cc_core/src/lib.rs`, add `pub mod structural_material;` in alphabetical position and `pub use structural_material::StructuralMaterial;` beside `pub use structural_profile::...`.

- [ ] **Step 3: Run the tests.** `cargo nextest run -p cc_core structural_material`. Expected: 4 pass. If `template_types_by_material` fails on the steel assertions, print the template's column types and their profiles. The template has steel H and tube columns and a steel I beam, converted to steel profiles on creation (`convert_sections_to_profiles`).

- [ ] **Step 4: Clippy, then commit.** `cargo clippy -p cc_core --all-targets -- -D warnings`, then `cargo fmt --all`, then `git add -A && git commit -s -m "Read the structural material of column and beam types"`.

---

### Task 2: `structural_type_for` command (cc_engine)

**Files:**
- Create: `crates/cc_engine/src/commands/structure_material.rs`
- Modify:
  - `crates/cc_engine/src/commands/mod.rs`: `pub mod structure_material;`
  - `crates/cc_engine/src/registry.rs`: `structure_material::StructuralTypeFor,` next to `profiles::ProfileTypeCreate`.
  - `assets/i18n/*.ftl` (12 files): key `tx-structural-type`.
  - `docs/commands.md`: regenerated.
- Test:
  - Create `crates/cc_engine/tests/structure_materials.rs`.
  - Add `mod structure_materials;` to `crates/cc_engine/tests/main.rs`, in alphabetical order.

**Interfaces:**
- Consumes (Task 1): `StructuralMaterial`, `Project::types_of_material`, `Project::structural_material`.
- Produces:
  - `pub struct StructuralTypeFor { pub category: String, pub material: String }`. Command name `structural_type_for`; returns `{ id, name, created }`.
  - `pub(crate) fn structural_type(s: &mut Session, category: Category, material: StructuralMaterial) -> Result<(ElementTypeId, bool)>`. Task 3 uses it.
  - `pub(crate) fn material_arg(code: &str) -> Result<StructuralMaterial>`.

- [ ] **Step 1: Write the failing engine tests** in `crates/cc_engine/tests/structure_materials.rs`:

```rust
//! Column and beam types by structural material, and the multi-storey grid frame.

// Test helpers outside #[test] functions may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cc_core::{ElementTypeId, StructuralMaterial};
use cc_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn run_err(s: &mut Session, name: &str, args: Value) -> cc_engine::EngineError {
    s.execute(name, args).expect_err(name)
}

fn type_for(s: &mut Session, category: &str, material: &str) -> Value {
    run(s, "structural_type_for", json!({"category": category, "material": material}))
}

#[test]
fn an_existing_type_is_returned_without_an_undo_step() {
    let mut s = Session::default();
    let depth = s.doc.undo_depth();
    for (category, material) in [("column", "steel"), ("beam", "steel"), ("column", "concrete")] {
        let out = type_for(&mut s, category, material);
        assert_eq!(out["created"], false, "{category} {material}: the template has one");
        let id = ElementTypeId(out["id"].as_u64().unwrap());
        let data = &s.project().element_type(id).unwrap().data;
        assert_eq!(s.project().structural_material(data), StructuralMaterial::from_code(material));
    }
    assert_eq!(s.doc.undo_depth(), depth);
}

#[test]
fn a_missing_material_gets_its_catalog_default_once() {
    let mut s = Session::default();
    for category in ["column", "beam"] {
        let depth = s.doc.undo_depth();
        let out = type_for(&mut s, category, "timber");
        assert_eq!(out["created"], true);
        assert_eq!(out["name"], "MLC 9x32");
        assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
        let id = ElementTypeId(out["id"].as_u64().unwrap());
        let data = &s.project().element_type(id).unwrap().data;
        assert_eq!(s.project().structural_material(data), Some(StructuralMaterial::Timber));
        let again = type_for(&mut s, category, "timber");
        assert_eq!((again["id"].clone(), again["created"].clone()), (out["id"].clone(), json!(false)));
    }
    assert_eq!(s.project().profiles.values().filter(|p| p.name == "MLC 9x32").count(), 1, "one profile shared");
    run(&mut s, "undo", json!({}));
    assert!(s.project().types_of_material(cc_core::Category::Beam, StructuralMaterial::Timber).next().is_none());
}

#[test]
fn steel_and_concrete_defaults_are_made_when_missing() {
    let mut s = Session::default();
    // Point every column type at concrete so steel is missing.
    let steel: Vec<u64> = s
        .project()
        .types_of_material(cc_core::Category::Column, StructuralMaterial::Steel)
        .map(|t| t.id.0)
        .collect();
    let concrete = type_for(&mut s, "column", "concrete")["id"].as_u64().unwrap();
    for id in steel {
        run(&mut s, "type_delete", json!({"type": id, "replace_with": concrete}));
    }
    let out = type_for(&mut s, "column", "steel");
    assert_eq!((out["created"].clone(), out["name"].clone()), (json!(true), json!("W200x26,6")));
    // Concrete: delete them all but the new steel one, then ask for concrete.
    let concrete_ids: Vec<u64> = s
        .project()
        .types_of_material(cc_core::Category::Column, StructuralMaterial::Concrete)
        .map(|t| t.id.0)
        .collect();
    for id in concrete_ids {
        run(&mut s, "type_delete", json!({"type": id, "replace_with": out["id"]}));
    }
    let made = type_for(&mut s, "column", "concrete");
    assert_eq!(made["created"], true);
    let id = ElementTypeId(made["id"].as_u64().unwrap());
    let def = s.project().type_profile_def(&s.project().element_type(id).unwrap().data).unwrap();
    assert_eq!(def.shape, cc_core::ProfileShape::Rect { b: 0.2, h: 0.4, chamfer: 0.0 });
    assert_eq!(def.material, cc_core::ProfileMaterial::Concrete);
}

#[test]
fn bad_arguments_are_refused() {
    let mut s = Session::default();
    for (category, material) in [("wall", "steel"), ("column", "gold")] {
        let err = run_err(&mut s, "structural_type_for", json!({"category": category, "material": material}));
        assert_eq!(err.code, cc_engine::ErrorCode::InvalidArguments, "{category} {material}");
    }
}
```

`type_delete` with `replace_with` must exist. Check `crates/cc_engine/src/commands/types.rs` for its exact parameter names (`type`, `replace_with`) and adapt the two calls. If deleting is refused because elements use the type, the replacement moves them.

- [ ] **Step 2: Run them to see them fail.** `cargo nextest run -p cc_engine --test main structure_materials`. Expected: FAIL with `unknown command: structural_type_for`.

- [ ] **Step 3: Write the command** in `crates/cc_engine/src/commands/structure_material.rs`:

```rust
//! Column and beam types by structural material (concrete, steel, timber):
//! the first type of a material, else a default made from the profile
//! catalog (steel `W200x26,6`, timber `MLC 9x32`) or a 20 x 40 concrete
//! rectangle, in one undo step.

use super::profiles::{ProfileCreate, ProfileImport, ProfileTypeCreate};
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{Category, ElementTypeId, ProfileMaterial, ProfileShape, StructuralMaterial};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The catalog row of the steel default: (series, designation).
const STEEL_DEFAULT: (&str, &str) = ("W", "W200x26,6");
/// The catalog row of the timber default.
const TIMBER_DEFAULT: (&str, &str) = ("Glulam", "MLC 9x32");
/// The name of the concrete default profile, a 0.20 x 0.40 rectangle.
const CONCRETE_DEFAULT: &str = "20x40";

pub(crate) fn material_arg(code: &str) -> Result<StructuralMaterial> {
    StructuralMaterial::from_code(code).ok_or_else(|| {
        let codes: Vec<&str> = StructuralMaterial::ALL.iter().map(|m| m.code()).collect();
        EngineError::invalid(format!("unknown structural material '{code}' ({})", codes.join(", ")))
    })
}

pub(crate) fn member_category(code: &str) -> Result<Category> {
    match refs::category(code)? {
        c @ (Category::Column | Category::Beam) => Ok(c),
        c => Err(EngineError::invalid(format!("a column or beam, not {}", refs::category_name(c)))),
    }
}

/// The type of `category` new `material` members get: the first one of the
/// material, else a default made in one undo step. Returns it and whether it was made.
pub(crate) fn structural_type(
    s: &mut Session,
    category: Category,
    material: StructuralMaterial,
) -> Result<(ElementTypeId, bool)> {
    if let Some(t) = s.project().types_of_material(category, material).next() {
        return Ok((t.id, false));
    }
    let depth = s.doc.undo_depth();
    match make_default(s, category, material) {
        Ok(id) => {
            s.doc.squash_since(depth, t!("tx-structural-type"));
            Ok((id, true))
        }
        Err(e) => {
            s.undo_to(depth);
            Err(e)
        }
    }
}

fn make_default(s: &mut Session, category: Category, material: StructuralMaterial) -> Result<ElementTypeId> {
    let profile = match material {
        StructuralMaterial::Steel => catalog_profile(s, STEEL_DEFAULT)?,
        StructuralMaterial::Timber => catalog_profile(s, TIMBER_DEFAULT)?,
        StructuralMaterial::Concrete => concrete_profile(s)?,
    };
    let category = if category == Category::Column { "column" } else { "beam" };
    let out =
        ProfileTypeCreate { profile: Ref::Name(profile), category: category.into(), ..Default::default() }.run(s)?;
    out["id"].as_u64().map(ElementTypeId).ok_or_else(|| EngineError::not_found("the new type"))
}

/// The project's profile of a catalog row, imported when missing.
fn catalog_profile(s: &mut Session, (series, designation): (&str, &str)) -> Result<String> {
    if let Some(p) = s.project().profile_named(designation) {
        return Ok(p.name.clone());
    }
    ProfileImport { series: series.into(), designations: vec![designation.into()], on_clash: None }.run(s)?;
    s.project()
        .profile_named(designation)
        .map(|p| p.name.clone())
        .ok_or_else(|| EngineError::not_found(format!("catalog row {designation}")))
}

/// The 0.20 x 0.40 concrete rectangle: the project's `20x40` when it is that, else a new one.
fn concrete_profile(s: &mut Session) -> Result<String> {
    let wanted = ProfileShape::Rect { b: 0.2, h: 0.4, chamfer: 0.0 };
    let existing = s.project().profile_named(CONCRETE_DEFAULT);
    if let Some(p) = existing.filter(|p| p.shape == wanted && p.material == ProfileMaterial::Concrete) {
        return Ok(p.name.clone());
    }
    let name = existing.is_none().then(|| CONCRETE_DEFAULT.to_string());
    let params = BTreeMap::from([("b".to_string(), 0.2), ("h".to_string(), 0.4)]);
    let out = ProfileCreate {
        name,
        material: "concrete".into(),
        shape: Some("rect".into()),
        params: Some(params),
        ..Default::default()
    }
    .run(s)?;
    out["name"].as_str().map(str::to_string).ok_or_else(|| EngineError::not_found("the new profile"))
}

/// The column or beam type new members of a structural material get: the
/// first type of that material, else a default made in one undo step
/// (steel `W200x26,6`, timber `MLC 9x32` from the profile catalog, concrete
/// a 0.20 x 0.40 rectangle named `20x40`). Returns its `id`, `name` and
/// whether it was `created`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct StructuralTypeFor {
    /// `column` or `beam`.
    pub category: String,
    /// `concrete`, `steel` or `timber`.
    pub material: String,
}

impl Command for StructuralTypeFor {
    const NAME: &'static str = "structural_type_for";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let category = member_category(&self.category)?;
        let material = material_arg(&self.material)?;
        let (id, created) = structural_type(s, category, material)?;
        let name = s.project().type_name(id).to_string();
        Ok(json!({ "id": id.0, "name": name, "created": created }))
    }
}
```

Notes:
- If `Session::undo_to` is not `pub` from inside the crate, use the same rollback the session uses on a failed command (`s.undo_to(depth)` exists, `crates/cc_engine/src/session.rs:420`).
- If `Project::type_name` returns `&str` or `String`, adapt the `.to_string()`.
- `ProfileImport`, `ProfileCreate` and `ProfileTypeCreate` must be `pub` in `commands::profiles`; they already are.

- [ ] **Step 4: Register and add the key.**
  - Add `pub mod structure_material;` to `crates/cc_engine/src/commands/mod.rs`.
  - Add `structure_material::StructuralTypeFor,` to `registry!` in `crates/cc_engine/src/registry.rs`, after `profiles::ProfileTypeCreate,`.
  - Append `tx-structural-type` to the 12 `.ftl` files, with the table's text under `# Structure by material`.

- [ ] **Step 5: Run the tests.** `cargo nextest run -p cc_engine --test main structure_materials`. Expected: 4 pass.

- [ ] **Step 6: Regenerate the reference and run its check.** `cargo run -q -p cc_cli -- reference > docs/commands.md`. Then run `cargo nextest run -p cc_cli` and `cargo nextest run -p cc_app i18n`. Expected: pass.

- [ ] **Step 7: Clippy, fmt, commit.** `cargo clippy -p cc_engine --all-targets -- -D warnings`, `cargo fmt --all`, `git add -A && git commit -s -m "Add structural_type_for: a column or beam type of a material"`.

---

### Task 3: `grid_frame_create`: storeys of columns and main beams (cc_engine)

**Files:**
- Create: `crates/cc_engine/src/commands/grid_frame.rs`
- Modify:
  - `crates/cc_engine/src/commands/structure.rs`: make `SAME`, `crossing`, `intersections`, `columns_on` and `beam_exists` `pub(crate)`. Add `pub(crate) fn grids_with_ids(p: &Project, ids: &[u64]) -> Result<Vec<(ElementId, (DVec2, DVec2))>>` (the same lookup as `grid_lines`, keeping each grid's id) and rewrite `grid_lines` on top of it.
  - `crates/cc_engine/src/commands/mod.rs`: `pub mod grid_frame;`
  - `crates/cc_engine/src/registry.rs`: `grid_frame::GridFrameCreate,` after `structure::BeamsBetweenColumns,`.
  - `assets/i18n/*.ftl`: key `tx-grid-frame`.
  - `docs/commands.md`: regenerated.
- Test: `crates/cc_engine/tests/structure_materials.rs` (append).

**Interfaces:**
- Consumes (Task 2): `structure_material::{structural_type, material_arg}`.
- Produces:
  - `pub struct GridFrameCreate { grids: Vec<u64>, base_level: Option<Ref>, top_level: Option<Ref>, column_type: Option<Ref>, beam_type: Option<Ref>, material: Option<String>, secondary: Option<SecondaryArg> }`.
  - `pub struct SecondaryArg { pub parallel_to: u64, pub spacing: f64 }`. Task 4 fills in its behavior; in this task a `secondary` argument is accepted and ignored.
  - `pub const MAX_MEMBERS: usize = 2000;`
  - Return value: `{ columns: [ids], beams: [ids], secondary: [ids], skipped: n, skipped_bays: n }`.

- [ ] **Step 1: Append the failing tests** to `crates/cc_engine/tests/structure_materials.rs`:

```rust
/// Three storeys (levels at 0, 3, 6 and 9 m) over a 3 x 3 grid 6 m apart.
fn three_storeys() -> (Session, Vec<u64>, [u64; 4]) {
    let mut s = Session::default();
    let mut grids = Vec::new();
    for k in 0..3 {
        let x = 6.0 * k as f64;
        grids.push(run(&mut s, "grid_create", json!({"start": [x, -2.0], "end": [x, 14.0]}))["id"].as_u64().unwrap());
        grids.push(run(&mut s, "grid_create", json!({"start": [-2.0, x], "end": [14.0, x]}))["id"].as_u64().unwrap());
    }
    run(&mut s, "level_create", json!({"name": "L2", "elevation": 6.0}));
    run(&mut s, "level_create", json!({"name": "L3", "elevation": 9.0}));
    let mut levels: Vec<(f64, u64)> = s.project().levels.iter().map(|l| (l.elevation, l.id.0)).collect();
    levels.sort_by(|a, b| a.0.total_cmp(&b.0));
    let ids = [levels[0].1, levels[1].1, levels[2].1, levels[3].1];
    (s, grids, ids)
}

fn kind_count(s: &Session, f: impl Fn(&cc_core::ElementKind) -> bool) -> usize {
    s.project().elements.values().filter(|e| f(&e.kind)).count()
}

#[test]
fn grid_frame_fills_every_storey_in_one_step() {
    let (mut s, _, levels) = three_storeys();
    let depth = s.doc.undo_depth();
    let out = run(&mut s, "grid_frame_create", json!({"base_level": levels[0], "top_level": levels[3]}));
    assert_eq!(out["columns"].as_array().unwrap().len(), 27, "9 per storey");
    // Each upper level: 3 grids x 2 spans in each direction = 12 beams.
    assert_eq!(out["beams"].as_array().unwrap().len(), 36);
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    for id in out["columns"].as_array().unwrap() {
        let e = s.project().element(cc_core::ElementId(id.as_u64().unwrap())).unwrap();
        let cc_core::ElementKind::Column(c) = &e.kind else { panic!() };
        let i = levels.iter().position(|l| *l == e.level.0).unwrap();
        assert_eq!(c.top, cc_core::WallTop::Level { level: cc_core::LevelId(levels[i + 1]), offset: 0.0 });
        let data = &s.project().element_type(e.type_id).unwrap().data;
        assert_eq!(s.project().structural_material(data), Some(StructuralMaterial::Steel), "steel by default");
    }
    for id in out["beams"].as_array().unwrap() {
        let e = s.project().element(cc_core::ElementId(id.as_u64().unwrap())).unwrap();
        assert_ne!(e.level.0, levels[0], "beams sit on the upper levels");
        let cc_core::ElementKind::Beam(b) = &e.kind else { panic!() };
        assert!((s.project().beam_z(e, b).1 - s.project().level_elevation(e.level)).abs() < 1e-9, "top at the level");
    }
    // Again: nothing new.
    let again = run(&mut s, "grid_frame_create", json!({"base_level": levels[0], "top_level": levels[3]}));
    assert!(again["columns"].as_array().unwrap().is_empty() && again["beams"].as_array().unwrap().is_empty());
    assert_eq!(again["skipped"], 27 + 36);
    run(&mut s, "undo", json!({}));
    run(&mut s, "undo", json!({}));
    assert_eq!(kind_count(&s, |k| matches!(k, cc_core::ElementKind::Column(_))), 0);
}

#[test]
fn grid_frame_takes_given_types_and_refuses_bad_ranges() {
    let (mut s, _, levels) = three_storeys();
    let concrete = type_for(&mut s, "column", "concrete")["id"].clone();
    let out = run(
        &mut s,
        "grid_frame_create",
        json!({"base_level": levels[1], "top_level": levels[2], "column_type": concrete, "material": "timber"}),
    );
    let col = cc_core::ElementId(out["columns"][0].as_u64().unwrap());
    assert_eq!(s.project().element(col).unwrap().type_id.0, concrete.as_u64().unwrap(), "the given type wins");
    let beam = cc_core::ElementId(out["beams"][0].as_u64().unwrap());
    let data = &s.project().element_type(s.project().element(beam).unwrap().type_id).unwrap().data;
    assert_eq!(s.project().structural_material(data), Some(StructuralMaterial::Timber));
    let depth = s.doc.undo_depth();
    let err = run_err(&mut s, "grid_frame_create", json!({"base_level": levels[2], "top_level": levels[1]}));
    assert_eq!(err.code, cc_engine::ErrorCode::InvalidArguments);
    assert_eq!(s.doc.undo_depth(), depth, "a refused run leaves no undo step");
}

#[test]
fn grid_frame_is_bounded() {
    let mut s = Session::default();
    // 40 x 40 grids: 1600 columns and 3120 beams for one storey, over the limit.
    for k in 0..40 {
        let x = k as f64;
        run(&mut s, "grid_create", json!({"start": [x, -1.0], "end": [x, 41.0]}));
        run(&mut s, "grid_create", json!({"start": [-1.0, x], "end": [41.0, x]}));
    }
    let depth = s.doc.undo_depth();
    let err = run_err(&mut s, "grid_frame_create", json!({}));
    assert_eq!(err.code, cc_engine::ErrorCode::InvalidArguments);
    assert_eq!(s.doc.undo_depth(), depth, "refused before any edit, the type default included");
}
```

Check these against the real API:
- `Project::beam_z(e, b)` returns `(bottom, top)`. If it returns something else, read `crates/cc_core/src/project.rs` and assert the top equals the level elevation.
- `level_create` in the template returns `id`.
- The template has levels at 0 and 3 m.

- [ ] **Step 2: Run them to see them fail.** `cargo nextest run -p cc_engine --test main grid_frame`. Expected: FAIL with an unknown command.

- [ ] **Step 3: Expose the helpers in `structure.rs`:**

```rust
/// The grids named by `ids` with their lines, or every grid when none are given.
pub(crate) fn grids_with_ids(p: &Project, ids: &[u64]) -> Result<Vec<(ElementId, (DVec2, DVec2))>> {
    let line = |id: ElementId| match p.element(id).map(|e| &e.kind) {
        Some(ElementKind::Grid(g)) => Ok((id, (g.start, g.end))),
        _ => Err(EngineError::invalid(format!("element {} is not a grid", id.0))),
    };
    if ids.is_empty() {
        return Ok(p
            .elements
            .values()
            .filter_map(|e| match &e.kind {
                ElementKind::Grid(g) => Some((e.id, (g.start, g.end))),
                _ => None,
            })
            .collect());
    }
    ids.iter().map(|&id| refs::element(p, id).and_then(line)).collect()
}

/// The grid lines named by `ids`, or every grid when none are given.
fn grid_lines(p: &Project, ids: &[u64]) -> Result<Vec<(DVec2, DVec2)>> {
    Ok(grids_with_ids(p, ids)?.into_iter().map(|(_, l)| l).collect())
}
```

Mark `SAME`, `crossing`, `intersections`, `columns_on` and `beam_exists` as `pub(crate)`.

- [ ] **Step 4: Write `grid_frame.rs`:**

```rust
//! A multi-storey grid frame: on every storey of a range of levels, columns
//! at the grid intersections (tops attached to the level above) and beams
//! along the grids on the upper level, plus optional secondary beams, in one
//! undo step.

use super::structure::{SAME, beam_exists, columns_on, crossing, grids_with_ids, intersections};
use super::structure_material::{material_arg, structural_type};
use super::{add_element, edit, ids_json};
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::{Beam, Category, Column, ElementId, ElementKind, ElementTypeId, LevelId, StructuralMaterial, WallTop};
use glam::DVec2;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Most members one run creates.
pub const MAX_MEMBERS: usize = 2000;

/// Secondary beams in each bay, parallel to a grid.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct SecondaryArg {
    /// The grid the secondary beams run parallel to (one of `grids`).
    pub parallel_to: u64,
    /// The largest distance between beams, meters (at least 0.3).
    pub spacing: f64,
}

/// Frame a range of levels on the grids: on every storey (each pair of
/// consecutive levels from `base_level` to `top_level`), a column at each
/// grid intersection of the lower level with its top attached to the upper
/// one, and beams on the upper level along each grid between neighboring
/// columns. Columns and beams already modeled are skipped. With `secondary`,
/// each bay between two grids parallel to `parallel_to` and two crossing
/// grids gets beams parallel to it, at most `spacing` apart. One undo step;
/// returns the new `columns`, `beams` and `secondary` beams and how many
/// were `skipped` (and bays `skipped_bays`).
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct GridFrameCreate {
    /// Grid ids (default: every grid).
    #[serde(default)]
    pub grids: Vec<u64>,
    /// The lowest level (default: the lowest level of the project).
    #[serde(default)]
    pub base_level: Option<Ref>,
    /// The highest level (default: the level above the base).
    #[serde(default)]
    pub top_level: Option<Ref>,
    /// Column type id or name (default: the first of `material`, made when missing).
    #[serde(default)]
    pub column_type: Option<Ref>,
    /// Beam type id or name (default: the first of `material`, made when missing).
    #[serde(default)]
    pub beam_type: Option<Ref>,
    /// `concrete`, `steel` (default) or `timber`: the default types' material.
    #[serde(default)]
    pub material: Option<String>,
    /// Secondary beams in each bay.
    #[serde(default)]
    pub secondary: Option<SecondaryArg>,
}

/// What a run will create, worked out before any edit.
#[derive(Default)]
struct Layout {
    columns: Vec<(LevelId, Column)>,
    beams: Vec<(LevelId, DVec2, DVec2)>,
    secondary: Vec<(LevelId, DVec2, DVec2)>,
    skipped: usize,
    skipped_bays: usize,
}

impl Command for GridFrameCreate {
    const NAME: &'static str = "grid_frame_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let material = match &self.material {
            Some(m) => material_arg(m)?,
            None => StructuralMaterial::Steel,
        };
        // Everything that can be refused is checked before any edit.
        let p = s.project();
        let storeys = storeys(p, self.base_level.as_ref(), self.top_level.as_ref())?;
        let grids = grids_with_ids(p, &self.grids)?;
        if grids.len() < 2 {
            return Err(EngineError::invalid(t!("msg-grids-two")));
        }
        let lines: Vec<(DVec2, DVec2)> = grids.iter().map(|(_, l)| *l).collect();
        let points = intersections(&lines);
        if points.is_empty() {
            return Err(EngineError::invalid("the grids do not cross"));
        }
        if let Some(sec) = &self.secondary {
            check_secondary(&grids, sec)?;
        }
        let layout = layout(p, &storeys, &grids, &points, self.secondary.as_ref());
        let total = layout.columns.len() + layout.beams.len() + layout.secondary.len();
        if total > MAX_MEMBERS {
            return Err(EngineError::invalid(format!(
                "{total} members are more than {MAX_MEMBERS} in one run; frame fewer levels or grids"
            )));
        }
        let column_given = self.column_type.as_ref().map(|r| refs::type_of(p, Category::Column, Some(r))).transpose()?;
        let beam_given = self.beam_type.as_ref().map(|r| refs::type_of(p, Category::Beam, Some(r))).transpose()?;
        let depth = s.doc.undo_depth();
        let result = (|| -> Result<Value> {
            let column_type = match column_given {
                Some(t) => t,
                None => structural_type(s, Category::Column, material)?.0,
            };
            let beam_type = match beam_given {
                Some(t) => t,
                None => structural_type(s, Category::Beam, material)?.0,
            };
            create(s, layout, column_type, beam_type)
        })();
        match result {
            Ok(v) => {
                s.doc.squash_since(depth, t!("tx-grid-frame"));
                Ok(v)
            }
            Err(e) => {
                s.undo_to(depth);
                Err(e)
            }
        }
    }
}

/// The storeys from `base` to `top`: consecutive levels by elevation.
fn storeys(p: &cc_core::Project, base: Option<&Ref>, top: Option<&Ref>) -> Result<Vec<(LevelId, LevelId)>> {
    let mut levels: Vec<(f64, LevelId)> = p.levels.iter().map(|l| (l.elevation, l.id)).collect();
    levels.sort_by(|a, b| a.0.total_cmp(&b.0));
    let index = |id: LevelId| levels.iter().position(|(_, l)| *l == id);
    let base = match base {
        Some(r) => refs::level(p, r)?,
        None => levels.first().map(|l| l.1).ok_or_else(|| EngineError::rejected("the project has no levels"))?,
    };
    let b = index(base).ok_or_else(|| EngineError::not_found("base level"))?;
    let t = match top {
        Some(r) => index(refs::level(p, r)?).ok_or_else(|| EngineError::not_found("top level"))?,
        None => b + 1,
    };
    if t <= b || t >= levels.len() {
        return Err(EngineError::invalid("the top level must be above the base level"));
    }
    Ok(levels[b..=t].windows(2).map(|w| (w[0].1, w[1].1)).collect())
}

fn check_secondary(grids: &[(ElementId, (DVec2, DVec2))], sec: &SecondaryArg) -> Result<()> {
    if !(sec.spacing.is_finite() && sec.spacing >= 0.3) {
        return Err(EngineError::invalid("secondary spacing must be at least 0.3 m"));
    }
    if !grids.iter().any(|(id, _)| id.0 == sec.parallel_to) {
        return Err(EngineError::invalid(format!("grid {} is not one of the grids", sec.parallel_to)));
    }
    Ok(())
}

/// Columns and main beams of every storey, skipping what exists.
fn layout(
    p: &cc_core::Project,
    storeys: &[(LevelId, LevelId)],
    grids: &[(ElementId, (DVec2, DVec2))],
    points: &[DVec2],
    secondary: Option<&SecondaryArg>,
) -> Layout {
    let mut out = Layout::default();
    for &(lower, upper) in storeys {
        let existing = columns_on(p, lower);
        let mut on_level: Vec<DVec2> = existing.iter().map(|(_, c)| *c).collect();
        for &q in points {
            if on_level.iter().any(|c| c.distance(q) < SAME) {
                out.skipped += 1;
                continue;
            }
            on_level.push(q);
            out.columns.push((
                lower,
                Column {
                    position: q,
                    rotation_deg: 0.0,
                    base_offset: 0.0,
                    height: (p.level_elevation(upper) - p.level_elevation(lower)).max(0.1),
                    top: WallTop::Level { level: upper, offset: 0.0 },
                    structural: true,
                },
            ));
        }
        for (_, (a, b)) in grids {
            for (x, y) in spans_along(*a, *b, &on_level) {
                let planned = out.beams.iter().any(|&(l, s, e)| l == upper && same_span((s, e), (x, y)));
                if planned || beam_exists(p, upper, x, y) {
                    out.skipped += 1;
                } else {
                    out.beams.push((upper, x, y));
                }
            }
        }
        if let Some(sec) = secondary {
            secondary_beams(p, upper, grids, sec, &mut out);
        }
    }
    out
}

fn same_span((a, b): (DVec2, DVec2), (x, y): (DVec2, DVec2)) -> bool {
    (a.distance(x) < SAME && b.distance(y) < SAME) || (a.distance(y) < SAME && b.distance(x) < SAME)
}

/// Spans between neighboring points of `on` that lie on the line `a`-`b`.
fn spans_along(a: DVec2, b: DVec2, on: &[DVec2]) -> Vec<(DVec2, DVec2)> {
    let Some(u) = (b - a).try_normalize() else { return Vec::new() };
    let len = a.distance(b);
    let mut along: Vec<(f64, DVec2)> = on
        .iter()
        .map(|&c| ((c - a).dot(u), c))
        .filter(|&(t, c)| (c - a).perp_dot(u).abs() < SAME && t > -SAME && t < len + SAME)
        .collect();
    along.sort_by(|x, y| x.0.total_cmp(&y.0));
    along.dedup_by(|x, y| x.1.distance(y.1) < SAME);
    along.windows(2).map(|w| (w[0].1, w[1].1)).filter(|(x, y)| x.distance(*y) >= SAME).collect()
}

/// Secondary beams of `upper` (Task 4 fills this in).
fn secondary_beams(
    _p: &cc_core::Project,
    _upper: LevelId,
    _grids: &[(ElementId, (DVec2, DVec2))],
    _sec: &SecondaryArg,
    _out: &mut Layout,
) {
}

fn create(s: &mut Session, layout: Layout, column_type: ElementTypeId, beam_type: ElementTypeId) -> Result<Value> {
    let Layout { columns, beams, secondary, skipped, skipped_bays } = layout;
    if columns.is_empty() && beams.is_empty() && secondary.is_empty() {
        return Ok(json!({"columns": [], "beams": [], "secondary": [], "skipped": skipped, "skipped_bays": skipped_bays}));
    }
    let (c, b, sec) = edit(s, t!("tx-grid-frame"), |e| {
        let c: Vec<ElementId> =
            columns.into_iter().map(|(l, col)| add_element(e, l, column_type, ElementKind::Column(col))).collect();
        let mut beam = |(l, x, y): (LevelId, DVec2, DVec2)| {
            add_element(e, l, beam_type, ElementKind::Beam(Beam::new(x, y, 0.0)))
        };
        let b: Vec<ElementId> = beams.into_iter().map(&mut beam).collect();
        let sec: Vec<ElementId> = secondary.into_iter().map(&mut beam).collect();
        Ok((c, b, sec))
    })?;
    Ok(json!({
        "columns": ids_json(&c),
        "beams": ids_json(&b),
        "secondary": ids_json(&sec),
        "skipped": skipped,
        "skipped_bays": skipped_bays,
    }))
}
```

Notes:
- `Beam::new(start, end, top_offset)` is used by `structure.rs`. Top offset 0 puts the beam's top at its own level.
- If borrowing `e` in two closures does not compile, write the beams in a plain `for` loop.
- `crossing` is imported for Task 4. If clippy flags it as unused here, import it in Task 4 instead.

- [ ] **Step 5: Register, add the key, run.**
  - Add `pub mod grid_frame;` to `crates/cc_engine/src/commands/mod.rs`.
  - Add `grid_frame::GridFrameCreate,` to `registry!`.
  - Append `tx-grid-frame` to the 12 `.ftl` files.
  - Run `cargo nextest run -p cc_engine --test main structure_materials`. Expected: all pass.
  - If the bounded test fails because the default type was made before the check, make sure every check runs before `structural_type`, as in the code above.

- [ ] **Step 6: Reference, clippy, fmt, commit.**
  - `cargo run -q -p cc_cli -- reference > docs/commands.md`
  - `cargo clippy -p cc_engine --all-targets -- -D warnings`
  - `cargo fmt --all`
  - `git add -A && git commit -s -m "Add grid_frame_create: columns and beams over a range of levels"`

---

### Task 4: Secondary beams in `grid_frame_create`

**Files:**
- Modify: `crates/cc_engine/src/commands/grid_frame.rs` (`secondary_beams`).
- Test: `crates/cc_engine/tests/structure_materials.rs` (append).

**Interfaces:**
- Consumes (Task 3): `Layout`, `SecondaryArg`, `crossing`, `SAME`, `beam_exists`.

- [ ] **Step 1: Append the failing tests:**

```rust
#[test]
fn secondary_beams_divide_each_bay() {
    let (mut s, grids, levels) = three_storeys();
    // grids[0] is the vertical grid x = 0; bays are 6 m wide, spacing 2.5 m: ceil(6 / 2.5) - 1 = 2 per bay.
    let out = run(
        &mut s,
        "grid_frame_create",
        json!({"base_level": levels[0], "top_level": levels[1], "secondary": {"parallel_to": grids[0], "spacing": 2.5}}),
    );
    // 4 bays x 2 beams on the one upper level.
    let sec = out["secondary"].as_array().unwrap();
    assert_eq!(sec.len(), 8);
    for id in sec {
        let e = s.project().element(cc_core::ElementId(id.as_u64().unwrap())).unwrap();
        let cc_core::ElementKind::Beam(b) = &e.kind else { panic!() };
        assert!((b.start.x - b.end.x).abs() < 1e-9, "parallel to the x = 0 grid");
        assert!((b.start.y - b.end.y).abs() - 6.0 < 1e-9 && (b.start.y - b.end.y).abs() > 6.0 - 1e-9);
        let frac = b.start.x.rem_euclid(6.0);
        assert!((frac - 2.0).abs() < 1e-9 || (frac - 4.0).abs() < 1e-9, "evenly spaced: {}", b.start.x);
    }
    // An exact multiple: 6 / 3 = 2 gives 1 beam per bay.
    let again = run(
        &mut s,
        "grid_frame_create",
        json!({"base_level": levels[1], "top_level": levels[2], "secondary": {"parallel_to": grids[0], "spacing": 3.0}}),
    );
    assert_eq!(again["secondary"].as_array().unwrap().len(), 4);
}

#[test]
fn skewed_bays_are_skipped_and_bad_secondary_refused() {
    let mut s = Session::default();
    let a = run(&mut s, "grid_create", json!({"start": [0.0, -1.0], "end": [0.0, 7.0]}))["id"].as_u64().unwrap();
    run(&mut s, "grid_create", json!({"start": [6.0, -1.0], "end": [6.0, 7.0]}));
    run(&mut s, "grid_create", json!({"start": [-1.0, 0.0], "end": [7.0, 0.0]}));
    // The fourth grid leans: the bay's crossing grids are not parallel.
    run(&mut s, "grid_create", json!({"start": [-1.0, 5.0], "end": [7.0, 7.0]}));
    let out = run(&mut s, "grid_frame_create", json!({"secondary": {"parallel_to": a, "spacing": 2.0}}));
    assert_eq!(out["secondary"].as_array().unwrap().len(), 0);
    assert_eq!(out["skipped_bays"], 1);
    for sec in [json!({"parallel_to": a, "spacing": 0.1}), json!({"parallel_to": 999_999, "spacing": 2.0})] {
        let err = run_err(&mut s, "grid_frame_create", json!({"secondary": sec}));
        assert_eq!(err.code, cc_engine::ErrorCode::InvalidArguments);
    }
}
```

- [ ] **Step 2: Run them to see them fail.** `cargo nextest run -p cc_engine --test main secondary`. Expected: FAIL (0 secondary beams).

- [ ] **Step 3: Implement `secondary_beams`:**

```rust
/// Grids within this angle are parallel.
const PARALLEL_DEG: f64 = 1.0;

fn parallel(u: DVec2, v: DVec2) -> bool {
    u.perp_dot(v).abs() <= PARALLEL_DEG.to_radians().sin()
}

/// Secondary beams of `upper`: in each bay between two neighboring grids
/// parallel to `parallel_to` and two neighboring crossing grids, beams
/// parallel to it, evenly spaced at most `spacing` apart, from one crossing
/// grid to the other. Bays whose crossing grids are not parallel to each
/// other, or that lack a corner, are skipped.
fn secondary_beams(
    p: &cc_core::Project,
    upper: LevelId,
    grids: &[(ElementId, (DVec2, DVec2))],
    sec: &SecondaryArg,
    out: &mut Layout,
) {
    let Some(&(_, (a0, a1))) = grids.iter().find(|(id, _)| id.0 == sec.parallel_to) else { return };
    let Some(d) = (a1 - a0).try_normalize() else { return };
    let n = d.perp();
    let dir = |(s, e): (DVec2, DVec2)| (e - s).try_normalize();
    // Parallel grids by their offset across `d`; crossing grids by where they meet the reference line.
    let mut parallels: Vec<(f64, (DVec2, DVec2))> = Vec::new();
    let mut crossings: Vec<(f64, (DVec2, DVec2))> = Vec::new();
    for &(_, line) in grids {
        let Some(u) = dir(line) else { continue };
        if parallel(u, d) {
            parallels.push(((line.0 - a0).dot(n), line));
        } else {
            let along = (line.0 - a0).dot(d) - (line.0 - a0).dot(n) * u.dot(d) / u.dot(n);
            crossings.push((along, line));
        }
    }
    parallels.sort_by(|x, y| x.0.total_cmp(&y.0));
    crossings.sort_by(|x, y| x.0.total_cmp(&y.0));
    for pw in parallels.windows(2) {
        let ((o0, p0), (o1, p1)) = (pw[0], pw[1]);
        let width = (o1 - o0).abs();
        if width < SAME {
            continue;
        }
        for cw in crossings.windows(2) {
            let (c0, c1) = (cw[0].1, cw[1].1);
            let corners = (crossing(p0, c0), crossing(p1, c0), crossing(p0, c1), crossing(p1, c1));
            let (Some(q00), Some(q10), Some(q01), Some(q11)) = corners else {
                out.skipped_bays += 1;
                continue;
            };
            let (Some(u0), Some(u1)) = (dir(c0), dir(c1)) else { continue };
            if !parallel(u0, u1) {
                out.skipped_bays += 1;
                continue;
            }
            let count = ((width / sec.spacing) - 1e-9).ceil().max(1.0) as usize - 1;
            for k in 1..=count {
                let f = k as f64 / (count + 1) as f64;
                let (x, y) = (q00.lerp(q10, f), q01.lerp(q11, f));
                if beam_exists(p, upper, x, y) {
                    out.skipped += 1;
                } else {
                    out.secondary.push((upper, x, y));
                }
            }
        }
    }
}
```

Check the geometry by hand with the test: grids x = 0, 6, 12 are parallel to `grids[0]` (vertical), and the crossing grids are y = 0, 6, 12.
- Bays are [0,6] x [0,6] and so on: 4 bays.
- The width is 6, so with spacing 2.5 the count is ceil(2.4) - 1 = 2. The beams sit at x = 2 and x = 4, from y = c0 to y = c1, 6 m long.

`along` sorts the crossing grids by where each meets the reference line. Its formula projects `line.0` onto `d` and corrects along `u`. If it reads wrong, replace it with the `x` of `crossing(extended reference, line)`. Any monotonic key along `d` works.

The leaning grid in the second test makes both corners exist but the crossing grids are not parallel, so the bay is skipped. With only one bay possible, `skipped_bays` is 1.

- [ ] **Step 4: Run all the module's tests.** `cargo nextest run -p cc_engine --test main structure_materials`. Expected: all pass.

- [ ] **Step 5: Clippy, fmt, reference, commit.**
  - `cargo clippy -p cc_engine --all-targets -- -D warnings`
  - `cargo fmt --all`
  - `cargo run -q -p cc_cli -- reference > docs/commands.md`
  - `git add -A && git commit -s -m "Add secondary beams to grid_frame_create"`

---

### Task 5: Material filter on column and beam tools (cc_app)

**Files:**
- Modify:
  - `crates/cc_app/src/tools.rs`:
    - `Tool::Column`/`Tool::Beam` gain `material: Option<StructuralMaterial>`;
    - update every pattern: `type_id()` (around line 256), `with_type` (around line 314, keep the material), `for_category` (around line 361, `material: None`) and the canvas dispatch (around line 1567, `{ type_id, .. }`);
    - add `pub fn material(&self) -> Option<StructuralMaterial>`;
    - `tool_types` gains a `material: Option<StructuralMaterial>` parameter that keeps only `project.structural_material(&t.data) == Some(m)` when set, and its in-file tests pass `None`.
  - `crates/cc_app/src/preferences.rs`: `Preferences::structure_material: StructuralMaterial`, key `STRUCTURE_MATERIAL_KEY = "structure_material"`, read with `StructuralMaterial::from_code`, written with `.code()`.
  - `crates/cc_app/src/commands.rs`:
    - `UiCommand::StructurePlace(Category, StructuralMaterial)`, with label, icon and description taken from `Place(category)`, no shortcut, no engine name;
    - `unavailable` maps it to the reason `Place(category)` gives;
    - `run` calls `actions::structure_place(state, category, material)`.
  - `crates/cc_app/src/actions.rs`: `structure_place` and `material_type_ref`.
  - `crates/cc_app/src/ribbon.rs`: the options bar passes `state.tool.material()` to `tool_types`.
- Test: `crates/cc_app/src/ui_tests.rs`.

**Interfaces:**
- Consumes (Tasks 1–2): `StructuralMaterial`, the command `cc_engine::commands::structure_material::StructuralTypeFor`.
- Produces:
  - `UiCommand::StructurePlace(Category, StructuralMaterial)`.
  - `actions::structure_place(state, Category, StructuralMaterial)`.
  - `actions::material_type_ref(state: &mut AppState, category: Category) -> Option<cc_engine::Ref>`: the selected material's type through `StructuralTypeFor`.
  - `Preferences::structure_material`.

- [ ] **Step 1: Write the failing UI test** in `ui_tests.rs`:

```rust
/// Column in Steel starts on a steel type, and the options bar lists only
/// steel column types; Timber makes its default type, undone in one step.
#[test]
fn structure_column_tool_keeps_to_its_material() {
    use cc_core::StructuralMaterial as M;
    let mut state = sample_state();
    let ctx = egui::Context::default();
    crate::commands::run(&mut state, &ctx, UiCommand::StructurePlace(Category::Column, M::Steel));
    let crate::tools::Tool::Column { type_id, material } = state.tool.clone() else { panic!("{:?}", state.tool) };
    assert_eq!(material, Some(M::Steel));
    let project = state.project();
    assert_eq!(project.structural_material(&project.element_type(type_id).unwrap().data), Some(M::Steel));
    let listed = crate::tools::tool_types(project, Category::Column, type_id, Some(M::Steel));
    assert!(!listed.is_empty());
    assert!(listed.iter().all(|(id, _)| project.structural_material(&project.element_type(*id).unwrap().data)
        == Some(M::Steel)));
    assert!(listed.len() < project.types_of(Category::Column).count(), "concrete types are left out");
    // Timber: the project has none, so one is made.
    let depth = state.session.doc.undo_depth();
    crate::commands::run(&mut state, &ctx, UiCommand::StructurePlace(Category::Beam, M::Timber));
    let crate::tools::Tool::Beam { type_id, .. } = state.tool.clone() else { panic!() };
    assert_eq!(state.project().type_name(type_id), "MLC 9x32");
    assert_eq!(state.session.doc.undo_depth(), depth + 1);
}
```

Adapt `crate::commands::run(state, &ctx, cmd)` to its real signature (`crates/cc_app/src/commands.rs:1770`), and `type_name` to its return type.

- [ ] **Step 2: Run it to see it fail.** `cargo nextest run -p cc_app structure_column_tool`. Expected: compile error, `StructurePlace` is missing.

- [ ] **Step 3: Implement.**

`tools.rs` variants:

```rust
    Column {
        type_id: ElementTypeId,
        /// Started from the Structure tab: its type list keeps to this material.
        material: Option<cc_core::StructuralMaterial>,
    },
    Beam {
        type_id: ElementTypeId,
        material: Option<cc_core::StructuralMaterial>,
    },
```

`with_type` keeps the filter:

```rust
            Tool::Column { material, .. } => Tool::Column { type_id: id, material },
            Tool::Beam { material, .. } => Tool::Beam { type_id: id, material },
```

`Tool::material`:

```rust
    /// The structural material a column or beam tool's types keep to.
    pub fn material(&self) -> Option<cc_core::StructuralMaterial> {
        match self {
            Tool::Column { material, .. } | Tool::Beam { material, .. } => *material,
            _ => None,
        }
    }
```

`tool_types`:

```rust
pub fn tool_types(
    project: &Project,
    category: Category,
    type_id: ElementTypeId,
    material: Option<cc_core::StructuralMaterial>,
) -> Vec<(ElementTypeId, String)> {
    let group = (category == Category::Component).then(|| component_group(project, type_id));
    project
        .types_of(category)
        .filter(|t| group.is_none_or(|g| component_group(project, t.id) == g))
        .filter(|t| material.is_none_or(|m| project.structural_material(&t.data) == Some(m)))
        .map(|t| (t.id, t.name.clone()))
        .collect()
}
```

`actions.rs`:

```rust
/// The type of `category` in the Structure tab's material (made when the
/// project has none, as an undo step).
pub fn material_type_ref(state: &mut AppState, category: Category) -> Option<cc_engine::Ref> {
    let material = state.prefs.structure_material;
    let command = cc_engine::commands::structure_material::StructuralTypeFor {
        category: if category == Category::Column { "column" } else { "beam" }.into(),
        material: material.code().into(),
    };
    run_command(state, command).and_then(|o| o["id"].as_u64()).map(cc_engine::Ref::Id)
}

/// Start the column or beam tool of the Structure tab on a type of `material`.
pub fn structure_place(state: &mut AppState, category: Category, material: cc_core::StructuralMaterial) {
    state.prefs.structure_material = material;
    let Some(cc_engine::Ref::Id(id)) = material_type_ref(state, category) else { return };
    let type_id = cc_core::ElementTypeId(id);
    let material = Some(material);
    let tool = if category == Category::Column {
        Tool::Column { type_id, material }
    } else {
        Tool::Beam { type_id, material }
    };
    state.set_tool(tool);
}
```

`commands.rs`:
- Add the variant after `Place(Category)`.
- In `icon`, `label` and `description`, add `StructurePlace(category, _) => <the same as Place(category)>`.
- In `unavailable`, add `StructurePlace(category, _) => return UiCommand::Place(category).unavailable(state),` before the generic arms. The match is on `self`, so place it where it compiles.
- In `run`, add `StructurePlace(category, material) => actions::structure_place(state, category, material),`.
- `StructurePlace` stays out of the palette's `ALL` list (Place(Column) and Place(Beam) are there already).

`preferences.rs`:
- Add the field and default (`StructuralMaterial::default()`, Concrete).
- In `read`: `structure_material: get(STRUCTURE_MATERIAL_KEY).and_then(|v| cc_core::StructuralMaterial::from_code(&v)).unwrap_or(d.structure_material),`
- In `write`: `set(STRUCTURE_MATERIAL_KEY, self.structure_material.code().to_string());`

`ribbon.rs` options bar: `crate::tools::tool_types(state.project(), category, type_id, state.tool.material())`.

- [ ] **Step 4: Compile and run.** First `cargo check --tests -p cc_app`; fix every pattern the compiler flags. Then `cargo nextest run -p cc_app structure_column_tool tools::` and `cargo nextest run -p cc_app preferences`. Expected: pass.

- [ ] **Step 5: Clippy, fmt, commit.** `cargo clippy -p cc_app --all-targets -- -D warnings`, `cargo fmt --all`, then `git add -A && git commit -s -m "Keep the Structure tab's column and beam tools to a material"`.

---

### Task 6: Material selector and groups on the Structure tab (cc_app)

**Files:**
- Modify:
  - `crates/cc_app/src/ribbon.rs`: `structure_tab` and a new `material_selector`.
  - `crates/cc_app/src/actions.rs`: `columns_at_grids`, `beams_on_grids`, `beams_between_columns` and `braces_x` pass `type_ref: material_type_ref(state, <category>)`.
  - `assets/i18n/*.ftl`: `group-structure-material`.
- Test: `crates/cc_app/src/ui_tests.rs`.

**Interfaces:**
- Consumes (Task 5): `UiCommand::StructurePlace`, `actions::material_type_ref`, `Preferences::structure_material`.
- Uses `UiCommand::GridFrame` from Task 7. To keep this task compiling, add `GridFrame` to the steel and timber groups only in Task 7, and leave a one-line comment marking the place.

- [ ] **Step 1: Write the failing test:**

```rust
/// The Structure tab's material selector swaps the material's groups:
/// reinforcement in Concrete, bracing and the grid tools in Steel.
#[test]
fn structure_material_selector_swaps_groups() {
    let mut state = sample_state();
    state.ribbon_tab = RibbonTab::Structure;
    state.prefs.structure_material = cc_core::StructuralMaterial::Concrete;
    let mut harness = ribbon_harness(state, 1600.0);
    let rebar = t!("action-rebar-typical");
    let braces = t!("action-braces-x");
    assert!(harness.query_by_label(&rebar).is_some(), "concrete shows reinforcement");
    assert!(harness.query_by_label(&braces).is_none());
    harness.get_by_label(&crate::i18n::tr("profile-material-steel")).click();
    harness.run();
    assert_eq!(harness.state().prefs.structure_material, cc_core::StructuralMaterial::Steel);
    assert!(harness.query_by_label(&rebar).is_none(), "steel hides reinforcement");
    assert!(harness.query_by_label(&braces).is_some());
    assert!(harness.query_by_label(&t!("action-columns-at-grids")).is_some());
}
```

Check the label keys of BracesX and TypicalRebar in `commands.rs` `label()` (`action-braces-x`, `action-rebar-typical`). The query might match a tooltip rather than the button; if so, use `query_by_role_and_label(Role::Button, ...)`.

- [ ] **Step 2: Run it to see it fail.** `cargo nextest run -p cc_app structure_material_selector`.

- [ ] **Step 3: Implement the selector and the groups:**

```rust
/// The Structure tab's material: three stacked toggles.
fn material_selector(ui: &mut Ui, state: &mut AppState) {
    ui.vertical(|ui| {
        for m in cc_core::StructuralMaterial::ALL {
            let on = state.prefs.structure_material == m;
            let label = crate::i18n::tr(m.key());
            let response = ui.selectable_label(on, label.clone());
            response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, label.clone()));
            if response.clicked() {
                state.prefs.structure_material = m;
            }
        }
    });
}

fn structure_tab(ui: &mut Ui, state: &mut AppState) {
    use UiCommand::*;
    use cc_core::StructuralMaterial as M;
    let m = state.prefs.structure_material;
    group(ui, t!("group-structure-material"), |ui| material_selector(ui, state));
    let (column, beam) = (StructurePlace(Category::Column, m), StructurePlace(Category::Beam, m));
    match m {
        M::Concrete => {
            group(ui, t!("group-structure"), |ui| {
                command_buttons(ui, state, &[column, beam, place(Category::Floor)]);
            });
            group(ui, t!("group-reinforcement"), |ui| stacks(ui, state, &[&[TypicalRebar, RebarView, SteelSchedule]]));
        }
        M::Steel => {
            group(ui, t!("group-structure"), |ui| command_buttons(ui, state, &[column, beam, BracesX]));
            // Task 7 adds GridFrame to the second stack.
            group(ui, t!("group-framing"), |ui| {
                stacks(ui, state, &[&[ColumnsAtGrids, BeamsOnGrids, BeamsBetweenColumns], &[Truss]]);
            });
        }
        M::Timber => {
            group(ui, t!("group-structure"), |ui| command_buttons(ui, state, &[column, beam]));
            // Task 7 adds GridFrame here.
            group(ui, t!("group-framing"), |ui| {
                stacks(ui, state, &[&[ColumnsAtGrids, BeamsOnGrids, BeamsBetweenColumns]]);
            });
        }
    }
    group(ui, t!("group-profiles"), |ui| command_button(ui, state, Profiles));
    group(ui, t!("group-foundation"), |ui| {
        stacks(ui, state, &[&[FootingsUnderColumns, StripFootingsUnderWalls, FoundationSlab]]);
    });
    group(ui, t!("group-analytical"), |ui| {
        stacks(ui, state, &[&[SlabJoinSupports, AnalyticalView, AnalyticalExport]]);
    });
    group(ui, t!("group-documentation"), |ui| {
        stacks(ui, state, &[&[StructuralPlan, NumberStructure, StructuralSchedules]]);
    });
    group(ui, t!("group-datum"), |ui| command_buttons(ui, state, &[place(Category::Grid), AlignGridEnds]));
    group(ui, t!("group-selection"), |ui| command_button(ui, state, Select));
}
```

Check the `WidgetInfo::selected` signature in egui 0.36 (`selected(typ, enabled, selected, label)`) and adapt. If `selectable_label` already reports its selected state with the right label, drop the `widget_info` call.

`actions.rs`:
- `columns_at_grids`: `type_ref: material_type_ref(state, Category::Column),`.
- `beams_on_grids`, `beams_between_columns` and `braces_x`: `type_ref: material_type_ref(state, Category::Beam)`. Each struct already has a `type_ref` field.
- Take the type before building the command, because it needs `&mut state`.

- [ ] **Step 4: Run the UI tests.** First add `group-structure-material` to the 12 `.ftl` files. Then run:
  - `cargo nextest run -p cc_app structure_material_selector ribbon`;
  - the existing ribbon tests (`cargo nextest run -p cc_app ribbon`), which must still pass. Some use `RibbonTab::Structure`, which now opens on Concrete; a test that expected a steel-only button there must set `prefs.structure_material`.

  Expected: pass.

- [ ] **Step 5: Clippy, fmt, commit.** `git commit -s -m "Split the Structure tab by material"`.

---

### Task 7: Grid frame dialog (cc_app)

**Files:**
- Create:
  - `crates/cc_app/src/grid_frame_ui.rs`
  - `assets/icons/ui/grid-frame.svg`
- Modify:
  - `crates/cc_app/src/main.rs`: `mod grid_frame_ui;`
  - `crates/cc_app/src/state.rs`: `Dialogs::grid_frame: Option<crate::grid_frame_ui::GridFrameDialog>`.
  - `crates/cc_app/src/dialogs.rs`: call `crate::grid_frame_ui::show(ctx, state);`.
  - `crates/cc_app/src/commands.rs`: `UiCommand::GridFrame`:
    - icon `Icon::GridFrame`;
    - label `action-grid-frame`;
    - description `cmd-desc-grid-frame`;
    - engine name `grid_frame_create`;
    - unavailable when the project has fewer than two grids, reason `msg-grids-two`;
    - `run` opens the dialog;
    - add it to the `ALL` list next to `ColumnsAtGrids`.
  - `crates/cc_app/src/icons.rs`: `Icon::GridFrame`. Bump the `ALL` array length by 1 and add the `include_str!`.
  - `crates/cc_app/src/ribbon.rs`: `GridFrame` in the steel and timber framing groups (the places Task 6 marked).
  - `assets/i18n/*.ftl`: `action-grid-frame`, `cmd-desc-grid-frame`, `dlg-grid-frame`, `label-base-level`, `label-top-level`, `label-column-type`, `label-beam-type`, `label-secondary-beams`, `label-parallel-to`, `label-spacing` and `msg-grid-frame-created`.
- Test: `crates/cc_app/src/ui_tests.rs`.

**Interfaces:**
- Consumes:
  - `GridFrameCreate`, `SecondaryArg` (Tasks 3–4).
  - `Project::types_of_material` (Task 1).
  - `Preferences::structure_material` (Task 5).
- Produces: `GridFrameDialog { base: LevelId, top: LevelId, column_type: Option<ElementTypeId>, beam_type: Option<ElementTypeId>, secondary: bool, parallel_to: Option<ElementId>, spacing: f64, error: Option<String> }`.

- [ ] **Step 1: The icon** `assets/icons/ui/grid-frame.svg` (24 grid, white strokes, width 1.75, soft fill 20%):

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M4 21V5M12 21V5M20 21V5" /><path d="M3 5h18M3 13h18" /><path d="M4 21h16" stroke-width="1.25" stroke-dasharray="2 2"/><path d="M5 6h6v6H5zM13 6h6v6h-6z" fill="#fff" fill-opacity=".2" stroke="none"/></svg>
```

- [ ] **Step 2: Write the failing UI test:**

```rust
/// *Grid Frame* creates columns and beams on the grids for the chosen
/// levels in one undo step; Cancel leaves the model as it was.
#[test]
fn grid_frame_dialog_creates_and_cancels() {
    let mut state = sample_state();
    for (start, end) in [([0.0, -1.0], [0.0, 7.0]), ([6.0, -1.0], [6.0, 7.0]), ([-1.0, 0.0], [7.0, 0.0])] {
        let command = cc_engine::commands::create::GridCreate {
            start: start.into(),
            end: end.into(),
            ..Default::default()
        };
        crate::actions::run_command(&mut state, command).expect("grid");
    }
    state.ribbon_tab = RibbonTab::Structure;
    state.prefs.structure_material = cc_core::StructuralMaterial::Steel;
    let depth = state.session.doc.undo_depth();
    let columns = |h: &Harness<'static, AppState>| h.state().project().elements_of(Category::Column).count();
    let mut harness = app_harness(state);
    let before = columns(&harness);
    harness.get_by_role_and_label(Role::Button, &t!("action-grid-frame")).click();
    harness.run();
    assert!(harness.state().dialogs.grid_frame.is_some());
    harness.get_by_label(&t!("action-cancel")).click();
    harness.run();
    assert!(harness.state().dialogs.grid_frame.is_none());
    assert_eq!(harness.state().session.doc.undo_depth(), depth, "Cancel changes nothing");
    harness.get_by_role_and_label(Role::Button, &t!("action-grid-frame")).click();
    harness.run();
    harness.get_by_label(&t!("action-ok")).click();
    harness.run();
    assert!(harness.state().dialogs.grid_frame.is_none());
    assert_eq!(columns(&harness), before + 4, "four intersections on the first storey");
    assert_eq!(harness.state().session.doc.undo_depth(), depth + 1);
}
```

Read the real field names of `GridCreate` in `crates/cc_engine/src/commands/create.rs` (around line 690; `Point` may be `[f64; 2]`) and adapt the construction. If `elements_of` does not exist, count with `elements.values().filter(|e| e.category() == Category::Column)`.

The dialog opens with the base at the active or lowest level and the top at the next level. The template has two levels, so OK makes one storey: 4 columns at the 4 intersections of the 2 x 2 grid crossings (x = 0, 6 by y = 0) and the beams between them. The grids `x = 0` and `x = 6` cross `y = 0` at 2 points only; adjust the expected count to the real intersections. With the three grids above that is 2 columns, so either expect `before + 2` or add a fourth grid `y = 6` for 4.

- [ ] **Step 3: Write `grid_frame_ui.rs`:**

```rust
//! *Structure → Grid Frame*: the levels, the types (of the tab's material)
//! and optional secondary beams for `grid_frame_create`.

use crate::dialog::Dialog;
use crate::state::AppState;
use crate::t;
use crate::theme::PALETTE;
use cc_core::{Category, ElementId, ElementKind, ElementTypeId, LevelId};
use cc_engine::Ref;
use cc_engine::commands::grid_frame::{GridFrameCreate, SecondaryArg};
use egui::{Context, RichText};

#[derive(Debug, Clone)]
pub struct GridFrameDialog {
    pub base: LevelId,
    pub top: LevelId,
    pub column_type: Option<ElementTypeId>,
    pub beam_type: Option<ElementTypeId>,
    pub secondary: bool,
    pub parallel_to: Option<ElementId>,
    pub spacing: f64,
    pub error: Option<String>,
}

/// Levels by elevation.
fn levels(state: &AppState) -> Vec<(LevelId, String)> {
    let mut levels: Vec<_> = state.project().levels.iter().map(|l| (l.elevation, l.id, l.name.clone())).collect();
    levels.sort_by(|a, b| a.0.total_cmp(&b.0));
    levels.into_iter().map(|(_, id, name)| (id, name)).collect()
}

fn grids(state: &AppState) -> Vec<(ElementId, String)> {
    state
        .project()
        .elements
        .values()
        .filter_map(|e| match &e.kind {
            ElementKind::Grid(g) => Some((e.id, g.name.clone())),
            _ => None,
        })
        .collect()
}

/// Open the dialog: from the active level (else the lowest) to the next one.
pub fn open(state: &mut AppState) {
    let levels = levels(state);
    let start = state
        .active_level()
        .and_then(|a| levels.iter().position(|(id, _)| *id == a))
        .unwrap_or(0);
    let (Some(base), Some(top)) = (levels.get(start), levels.get(start + 1).or(levels.get(start))) else { return };
    let m = state.prefs.structure_material;
    let p = state.project();
    state.dialogs.grid_frame = Some(GridFrameDialog {
        base: base.0,
        top: top.0,
        column_type: p.types_of_material(Category::Column, m).next().map(|t| t.id),
        beam_type: p.types_of_material(Category::Beam, m).next().map(|t| t.id),
        secondary: false,
        parallel_to: grids(state).first().map(|g| g.0),
        spacing: 2.0,
        error: None,
    });
}

fn combo<T: PartialEq + Copy>(ui: &mut egui::Ui, id: &str, value: &mut T, items: &[(T, String)]) {
    let shown = items.iter().find(|(v, _)| v == value).map(|(_, n)| n.clone()).unwrap_or_default();
    egui::ComboBox::from_id_salt(id).selected_text(shown).width(200.0).show_ui(ui, |ui| {
        for (v, name) in items {
            ui.selectable_value(value, *v, name);
        }
    });
}

pub fn show(ctx: &Context, state: &mut AppState) {
    let Some(mut d) = state.dialogs.grid_frame.clone() else { return };
    let m = state.prefs.structure_material;
    let levels = levels(state);
    let grids = grids(state);
    let types = |c| -> Vec<(Option<ElementTypeId>, String)> {
        let p = state.project();
        p.types_of_material(c, m).map(|t| (Some(t.id), t.name.clone())).collect()
    };
    let (column_types, beam_types) = (types(Category::Column), types(Category::Beam));
    let mut ok = None;
    let response = Dialog::new("grid-frame", t!("dlg-grid-frame")).width(420.0).show(
        ctx,
        |ui| {
            egui::Grid::new("grid-frame-fields").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                let label = |ui: &mut egui::Ui, k: String| ui.label(RichText::new(k).color(PALETTE.text_muted));
                label(ui, t!("label-base-level"));
                combo(ui, "gf-base", &mut d.base, &levels);
                ui.end_row();
                label(ui, t!("label-top-level"));
                combo(ui, "gf-top", &mut d.top, &levels);
                ui.end_row();
                label(ui, t!("label-column-type"));
                combo(ui, "gf-column", &mut d.column_type, &column_types);
                ui.end_row();
                label(ui, t!("label-beam-type"));
                combo(ui, "gf-beam", &mut d.beam_type, &beam_types);
                ui.end_row();
            });
            ui.add_space(6.0);
            ui.checkbox(&mut d.secondary, t!("label-secondary-beams"));
            ui.add_enabled_ui(d.secondary, |ui| {
                egui::Grid::new("grid-frame-secondary").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                    ui.label(RichText::new(t!("label-parallel-to")).color(PALETTE.text_muted));
                    let items: Vec<(Option<ElementId>, String)> =
                        grids.iter().map(|(id, n)| (Some(*id), n.clone())).collect();
                    combo(ui, "gf-parallel", &mut d.parallel_to, &items);
                    ui.end_row();
                    ui.label(RichText::new(t!("label-spacing")).color(PALETTE.text_muted));
                    ui.add(egui::DragValue::new(&mut d.spacing).range(0.3..=50.0).speed(0.05).suffix(" m"));
                    ui.end_row();
                });
            });
            if let Some(e) = &d.error {
                ui.add_space(6.0);
                ui.label(RichText::new(e).color(PALETTE.error));
            }
        },
        |f| {
            ok = Some(f.primary(t!("action-ok")));
            f.secondary(t!("action-cancel"));
        },
    );
    if response.clicked.is_some() && response.clicked == ok {
        let command = GridFrameCreate {
            base_level: Some(Ref::Id(d.base.0)),
            top_level: Some(Ref::Id(d.top.0)),
            column_type: d.column_type.map(|t| Ref::Id(t.0)),
            beam_type: d.beam_type.map(|t| Ref::Id(t.0)),
            material: Some(m.code().into()),
            secondary: d
                .secondary
                .then_some(d.parallel_to)
                .flatten()
                .map(|g| SecondaryArg { parallel_to: g.0, spacing: d.spacing }),
            ..Default::default()
        };
        match state.session.run(command) {
            Ok(out) => {
                let count = |k: &str| out[k].as_array().map_or(0, Vec::len);
                state.message = t!(
                    "msg-grid-frame-created",
                    "columns" => count("columns"),
                    "beams" => count("beams"),
                    "secondary" => count("secondary"),
                    "skipped" => out["skipped"].as_u64().unwrap_or(0)
                );
                state.dialogs.grid_frame = None;
            }
            Err(e) => {
                d.error = Some(e.message);
                state.dialogs.grid_frame = Some(d);
            }
        }
        return;
    }
    if response.clicked.is_some() || response.dismissed {
        state.dialogs.grid_frame = None;
    } else {
        state.dialogs.grid_frame = Some(d);
    }
}
```

Adapt to the real API:
- `PALETTE.error`: if there is no such token, use the theme's danger or error color from `theme.rs`.
- `g.name` on `Grid`: read `cc_core::Grid`.
- The `t!` macro's argument syntax: copy from `msg-columns-placed` in `actions.rs`.
- `state.session.run` vs `actions::run_command`: `run_command` sends errors to the status bar; the dialog wants the message, so use `session.run`.
- After a run, `AppState` may need `state.sync()` or a geometry refresh. Check how `actions::run_command` finishes and do the same, or call `run_command` and keep the dialog open on `None`.

Wire it up:
- `state.rs`: `pub grid_frame: Option<crate::grid_frame_ui::GridFrameDialog>` in `Dialogs`, default `None`.
- `dialogs.rs`: `crate::grid_frame_ui::show(ctx, state);`.
- `commands.rs` `run`: `GridFrame => crate::grid_frame_ui::open(state),`.
- `unavailable`: `GridFrame if grid_count(project) < 2 => Some(t!("msg-grids-two")),`, where `grid_count` counts `ElementKind::Grid` elements.
- `icons.rs`: add the variant and the `include_str!("../../../assets/icons/ui/grid-frame.svg")`, and bump `ALL`.
- `ribbon.rs`:
  - steel: `&[&[ColumnsAtGrids, BeamsOnGrids, BeamsBetweenColumns], &[GridFrame, Truss]]`;
  - timber: `&[&[ColumnsAtGrids, BeamsOnGrids, BeamsBetweenColumns], &[GridFrame]]`.

- [ ] **Step 4: Keys and tests.** Add the eleven keys to the 12 `.ftl` files. Run `cargo nextest run -p cc_app grid_frame_dialog i18n icons`. Expected: pass.

- [ ] **Step 5: Clippy, fmt, commit.** `git commit -s -m "Add the Grid Frame dialog"`.

---

### Task 8: Documentation and full check

**Files:**
- Modify:
  - `docs/interface.md`, `docs/interface.pt-BR.md`: in the Structure tab section, describe the material selector, the material groups, types created from the catalog, and *Grid Frame* with its fields.
  - `AGENTS.md`, in the cc_engine or Structural profiles paragraph:
    - `cc_core::structural_material` (`StructuralMaterial`, `Project::structural_material`, `types_of_material`);
    - `cc_engine::commands::structure_material` (`structural_type_for`, defaults);
    - `cc_engine::commands::grid_frame` (`grid_frame_create`, `MAX_MEMBERS`, secondary beams);
    - the app's `grid_frame_ui.rs`, `Tool::Column/Beam::material` and `Preferences::structure_material`.
  - `CHANGELOG.md`: one entry under `## [Unreleased]` → `### Added` describing the Structure tab by material and Grid Frame.
  - `docs/automation.md`, `docs/automacao.md`: a short example: `{"command": "grid_frame_create", "args": {"base_level": "Térreo", "top_level": "Cobertura", "material": "steel", "secondary": {"parallel_to": 12, "spacing": 2.5}}}` (prose in English or Portuguese, matching each file).

- [ ] **Step 1: Write the docs** above. Keep them concise and in the style of the neighboring entries.
- [ ] **Step 2: Full check.**
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo nextest run --workspace`
  - `cargo test --workspace --doc`

  Expected: all green.
- [ ] **Step 3: Commit.** `git commit -s -m "Document the Structure tab by material and Grid Frame"`.
