# Wall Framing (Light Steel and Wood) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a parametric `WallFraming` host element, attached to one straight wall, that lays out light steel (cold-formed) or wood studs, tracks or plates, king and jack studs, headers, sills, cripples and blocking as ordinary columns and beams, regenerated whenever the framing, its wall or the wall's doors and windows change. It comes with `wall_framing_create`, and with the *Light Steel Frame* and *Wood Frame* buttons, a dialog and palette rows in the app.

**Architecture:**
- `cc_core::member_host` (from `feat/parametric-truss`) gains two default trait methods: `inputs(&self, &Project) -> Vec<ElementId>` (other elements whose change regenerates the host) and `member_level(&self, &Project) -> Option<LevelId>` (the level members go on when it is not the host's). The generic `sync_member_hosts` regenerates a host when an edit touched one of its inputs, an input's type, or an element that an input hosted before the edit. It also gains `Project::member_host_of`.
- `cc_core::wall_framing` holds the `WallFraming` model (`FramingSystem`, `FramingRoleKind`, `FramingRole`, `FramingMember`), its bounds and defaults, and the deterministic `layout(&Project)` in the wall's frame. `WallFraming` implements `MemberHost` (its inputs are the wall and the wall's doors and windows). Removal goes with the wall (`EditCtx::remove_element`), copies go with the wall (`copy_set`/`paste_elements`), and repair allows one framing per wall.
- `cc_engine::commands::wall_framing::WallFramingCreate` (`wall_framing_create`) creates one framing per wall in one undo step, with catalog default types through `structure_material::type_or_row`/`catalog_type`. `element_get` shows the derived member lists, and `element_set` goes through the shared host check and also refuses a change of `wall`.
- `cc_app`: `UiCommand::LightSteelFrame` (Steel framing group) and `UiCommand::WoodFrame` (Timber framing group) open `wall_framing_ui.rs`'s dialog. That file also holds the palette rows (with `member_host_ui::member_rows` and *Reset Members*) and the *Framing* row of a member, which selects its framing. A new icon `wall-framing.svg` is added.

**Tech Stack:** Rust 2024 workspace. egui/eframe app with egui_kittest UI tests. Fluent i18n in 12 languages. cargo-nextest.

**Spec:** `docs/superpowers/specs/2026-11-13-wall-framing-design.md`

## Global Constraints

- Work only in the worktree `D:\constructcad-framing` (branch `feat/wall-framing`, stacked on `feat/parametric-truss`). Never touch `D:\constructcad`, `D:\constructcad-steel` or `D:\constructcad-truss`; another agent is still implementing `feat/parametric-truss` there.
  - Before Task 1, once every task of `D:\constructcad-truss\docs\superpowers\plans\2026-11-13-parametric-truss.md` is committed on `feat/parametric-truss`, run `git -C D:\constructcad-truss worktree add D:\constructcad-framing -b feat/wall-framing feat/parametric-truss`.
  - Then copy the spec to `docs/superpowers/specs/2026-11-13-wall-framing-design.md` and this plan to `docs/superpowers/plans/2026-11-13-wall-framing.md` in the new worktree. Both are committed with Task 1.
- **The member-host contract is the truss plan's Task 1.** Before Task 1, read the committed `crates/cc_core/src/member_host.rs` in full. This plan quotes it as of the truss branch: `RoleKey(u64)` with `new(kind: u8, group: u16, index: u16)`/`parts`; `trait MemberHost { member_types, member_layout(&self, &Project), member_list, set_member_list, detached_roles, set_detached_roles, role_fits, check_parameters, repair_parameters }`; `ElementKind::member_host[_mut]`; `Project::host_members_of`; `EditCtx::sync_member_hosts` (with `regenerate_members(host, added, follow)`); `Project::repair_member_hosts`; `pub(crate) within, reset, fix_type`. If a name or signature differs, adapt this plan's code to the real one and keep its behavior. Do not change the truss or steel building behavior.
- **Code from the truss plan is consumed as committed:**
  - `cc_app::member_host_ui::{finished, length, member_rows}`;
  - `structure_material::type_or_row` (`pub(crate)`). If it is still private in `steel_building.rs`, move it as the truss plan's Task 4 Step 3 says;
  - `Category::Truss`, `ElementKind::Truss`, `TypeData::Truss {}`, `Icon::Truss`. Every match that lists `SteelBuilding`/`Truss` also gets the framing arm.
- Never push, never run `gh`, never merge. Commit with `git commit -s`. No Claude attribution or Co-Authored-By.
- Non-test code never panics: no `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!` or `unimplemented!` (clippy denies them). No `unsafe`. Guard non-finite numbers. Bound every loop.
- Every new interface or transaction text is an i18n key added to all 12 files in `assets/i18n/`: `pt-BR`, `en`, `es`, `fr`, `de`, `it`, `lo`, `pl`, `uk-UA`, `zh-TW`, `zh-CN`, `ja`. Append them under a comment `# Wall framing`.
- Every engine command parameter has a doc comment. Register `wall_framing::WallFramingCreate` in `crates/cc_engine/src/registry.rs`. Regenerate the reference with `cargo run -q -p cc_cli -- reference > docs/commands.md`; a test checks it.
- Integration tests are one binary: `crates/cc_engine/tests/wall_framing.rs` needs `mod wall_framing;` in `crates/cc_engine/tests/main.rs`, between `mod wall_attach;` and `mod wall_location_joins;`.
- Tests run with cargo-nextest:
  - `cargo nextest run -p <crate> <filter>`, adding `--test main` for engine integration tests;
  - `cargo check --tests -p <crate>` for type errors;
  - `cargo test --workspace --doc` for doctests.
- Formatting is rustfmt with `max_width = 120` (`cargo fmt --all`). Clippy: `cargo clippy --workspace --all-targets -- -D warnings`.
- Code, comments and identifiers are in English.
- Names are new and do not clash: `Category::WallFraming`, `TypeData::WallFraming {}`, `ElementKind::WallFraming(Box<WallFraming>)`, module `cc_core::wall_framing`, `FramingSystem`, `FramingRoleKind`, `FramingRole`, `FramingMember`, `Icon::WallFraming`, `UiCommand::{LightSteelFrame, WoodFrame}`. `cc_core::roofing::RoofFraming` and `cc_geom::roof_framing` are unrelated and stay untouched.
- Values, verbatim from the spec:
  - systems `LightSteel` (code `light_steel`, the default) and `Wood` (code `wood`);
  - `spacing` 0.2–1.2 m on center from the wall start, default 0.4 for LSF and 0.6 for wood;
  - `double_top` is wood only: default on for wood, off for LSF;
  - `blocking` defaults to off;
  - `MAX_MEMBERS = 4000`;
  - role kinds `BottomTrack`, `TopTrack`, `Stud`, `KingStud`, `JackStud`, `Header`, `Sill`, `Cripple`, `Blocking`, with fields `opening` (the opening's order along the wall) and `index`, packed as `RoleKey`;
  - one framing per wall: a second one is refused; curved walls are refused;
  - `wall_framing_create` returns `{ framings: [{ id, wall, members }] }`, and `wall` cannot be changed by `element_set`.
- Default types, all made through `catalog_type` (via `type_or_row`):

  | system | studs (column type of) | tracks/plates | headers/sills |
  |---|---|---|---|
  | LightSteel | `Ue 90x40x12x0,95` (NBR6355-Ue) | `U 92x38x0,95` (NBR6355-U) | `Ue 140x40x12x0,95` (NBR6355-Ue) |
  | Wood | `Peça 6x8` (Timber-Sawn) | `Peça 6x8` (Timber-Sawn) | `Peça 6x16` (Timber-Sawn) |
- Decisions this plan makes where the spec is open or inconsistent. Each one is also listed at the end.
  - `inputs` takes the project: `fn inputs(&self, project: &Project) -> Vec<ElementId>`, since the openings are found in the project.
  - The sync regenerates a host when the edit changed, added or removed an input, edited an input's type, or touched an element that one of its inputs hosted before the edit (a removed or re-hosted door or window).
  - The trait gains a second default method, `member_level` (a framing's members follow the wall's level).
  - **Openings** keep their clear width:
    - the jack studs stand at the opening's edges, at `u0 − s` and `u1 + s`, and the king studs one stud width further out, at `u0 − 3s` and `u1 + 3s`;
    - the header runs from `u0 − w` to `u1 + w`, over the jacks and between the kings, and the sill runs from `u0` to `u1`, between the jacks;
    - a regular stud whose body overlaps `[u0 − 2w, u1 + 2w]` is dropped.

    The spec put the king studs at the edges and the jacks one stud width inside the opening, which would narrow a door by two stud widths.
  - **Stud rotation** equals the wall direction, with no quarter turn. A column section's depth is its local y, so this runs the depth across the wall as the spec intends; the spec's "+90°" assumed the other axis.
  - **End studs** are flush with the wall ends (centers `s` and `L − s`). The end stud's index is `END_STUD = u16::MAX`, so its key does not depend on the length. Of the regular studs, the one too close to the previous stud or to the end stud is dropped; the end stud stays.
  - **Blocking** uses the track type. Its top is at the mid-height between the tracks plus `t/2`. It runs face to face between neighboring full-height studs (regular and king), never across an opening and never shorter than `MIN_PIECE = 0.05` m. It is keyed by its left stud: `(0, k)` after stud `k`, and `(j + 1, 0)` / `(j + 1, 1)` after opening `j`'s end-side or start-side king.
  - **Cripple indices** are `2k` above the header and `2k + 1` below the sill. King and jack indices are 0 on the start side and 1 on the end side. Top plates are indexed from 0 for the topmost.
  - All framing beams have `join_start`/`join_end` off, so they keep their laid-out lengths.
  - The header's underside is capped at `top − n·t − header depth`. Jacks and cripples shorter than `MIN_PIECE` are skipped. A sill is made only when it clears the bottom track.
  - `double_top` on light steel is ignored, not refused.
  - A wall bent after framing keeps its framing with no members, and gets them back when straightened. Creation refuses curved walls.
  - Openings are ordered by `(offset, id)`. One without a readable size still takes its index.
  - A copied wall takes its framing along. A framing copied alone is skipped. Repair drops framings whose wall is gone and later duplicates on a wall.
  - App:
    - OK frames the selected walls, or the active level's walls when no wall is selected, keeping only straight walls that are not framed yet, and says so when none is left;
    - the palette shows the system read-only; the dialog's system choice resets the spacing, double top and types;
    - a single new icon serves both buttons.

## i18n keys

Keys added by each task (append under `# Wall framing` in all 12 files):

| key | pt-BR | en | es | fr | de | it | lo | pl | uk-UA | zh-TW | zh-CN | ja |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| category-wall-framing (T1) | Ossaturas de parede | Wall framings | Entramados de muro | Ossatures de mur | Wandständerwerke | Telai di parete | ໂຄງຝາ | Szkielety ścian | Каркаси стін | 牆骨架 | 墙骨架 | 壁下地フレーム |
| tpl-type-wall-framing (T1) | Ossatura de parede | Wall framing | Entramado de muro | Ossature de mur | Wandständerwerk | Telaio di parete | ໂຄງຝາ | Szkielet ściany | Каркас стіни | 牆骨架 | 墙骨架 | 壁下地フレーム |
| framing-light-steel (T1) | Light Steel Frame | Light Steel Frame | Entramado de acero ligero | Ossature acier légère | Stahlleichtbau | Telaio in acciaio leggero | ໂຄງເຫຼັກເບົາ | Lekki szkielet stalowy | ЛСТК-каркас | 輕鋼構架 | 轻钢龙骨 | 軽量鉄骨下地 |
| framing-wood (T1) | Wood Frame | Wood Frame | Entramado de madera | Ossature bois | Holzrahmenbau | Telaio in legno | ໂຄງໄມ້ | Szkielet drewniany | Дерев'яний каркас | 木構架 | 木骨架 | 木造枠組 |
| tx-wall-framing (T3) | Estruturar paredes | Frame walls | Entramar muros | Ossaturer des murs | Wandständerwerk erstellen | Creare telai di parete | ສ້າງໂຄງຝາ | Utwórz szkielety ścian | Створити каркаси стін | 建立牆骨架 | 创建墙骨架 | 壁下地フレームを作成 |
| action-light-steel-frame (T4) | Light Steel Frame | Light Steel Frame | Entramado de acero ligero | Ossature acier légère | Stahlleichtbau | Telaio in acciaio leggero | ໂຄງເຫຼັກເບົາ | Lekki szkielet stalowy | ЛСТК-каркас | 輕鋼構架 | 轻钢龙骨 | 軽量鉄骨下地 |
| action-wood-frame (T4) | Wood Frame | Wood Frame | Entramado de madera | Ossature bois | Holzrahmenbau | Telaio in legno | ໂຄງໄມ້ | Szkielet drewniany | Дерев'яний каркас | 木構架 | 木骨架 | 木造枠組 |
| dlg-wall-framing (T4) | Ossatura de parede | Wall Framing | Entramado de muro | Ossature de mur | Wandständerwerk | Telaio di parete | ໂຄງຝາ | Szkielet ściany | Каркас стіни | 牆骨架 | 墙骨架 | 壁下地フレーム |
| section-wall-framing (T4) | Ossatura | Framing | Entramado | Ossature | Ständerwerk | Telaio | ໂຄງ | Szkielet | Каркас | 骨架 | 骨架 | 下地フレーム |
| prop-wall-framing (T4) | Ossatura | Framing | Entramado | Ossature | Ständerwerk | Telaio | ໂຄງ | Szkielet | Каркас | 骨架 | 骨架 | 下地フレーム |
| prop-framing-system (T4) | Sistema | System | Sistema | Système | System | Sistema | ລະບົບ | System | Система | 系統 | 系统 | 工法 |
| prop-stud-spacing (T4) | Espaçamento dos montantes | Stud spacing | Separación de montantes | Entraxe des montants | Ständerabstand | Interasse dei montanti | ໄລຍະຫ່າງເສົາຕັ້ງ | Rozstaw słupków | Крок стояків | 立柱間距 | 立柱间距 | 間柱間隔 |
| label-stud-type (T4) | Tipo dos montantes | Stud type | Tipo de montantes | Type de montants | Ständertyp | Tipo di montanti | ປະເພດເສົາຕັ້ງ | Typ słupków | Тип стояків | 立柱類型 | 立柱类型 | 間柱タイプ |
| label-track-type (T4) | Tipo das guias e soleiras | Track and plate type | Tipo de canales y soleras | Type de rails et lisses | Typ der Schienen und Schwellen | Tipo di guide e correnti | ປະເພດລາງ ແລະ ແຜ່ນ | Typ prowadnic i podwalin | Тип напрямних і обв'язок | 軌道與板材類型 | 天地龙骨类型 | ランナー・土台タイプ |
| label-header-type (T4) | Tipo das vergas e contravergas | Header and sill type | Tipo de dinteles y antepechos | Type de linteaux et appuis | Typ der Stürze und Brüstungsriegel | Tipo di architravi e davanzali | ປະເພດຄານຂອບເທິງ ແລະ ລຸ່ມ | Typ nadproży i podokienników | Тип перемичок і підвіконних ригелів | 過梁與窗台梁類型 | 过梁与窗台梁类型 | まぐさ・窓台タイプ |
| prop-double-top (T4) | Frechal duplo | Double top plate | Solera superior doble | Double sablière | Doppeltes Rähm | Doppio corrente superiore | ແຜ່ນເທິງຄູ່ | Podwójny oczep | Подвійна верхня обв'язка | 雙頂板 | 双顶梁 | ダブル頭つなぎ |
| prop-blocking (T4) | Bloqueadores | Blocking | Bloqueos | Entretoises | Zwischenriegel | Rompitratta | ໄມ້ຂວາງ | Rygle | Розпірки | 橫撐 | 横撑 | 胴つなぎ |
| action-select-framing (T4) | Selecionar | Select | Seleccionar | Sélectionner | Auswählen | Seleziona | ເລືອກ | Zaznacz | Виділити | 選取 | 选择 | 選択 |

`msg-no-walls-to-frame` (T4, no placeholder):
- pt-BR: Nenhuma parede reta sem ossatura: selecione paredes ou abra uma planta do nível delas.
- en: No straight unframed walls: select walls, or open a plan of their level.
- es: Ningún muro recto sin entramado: seleccione muros o abra una planta de su nivel.
- fr: Aucun mur droit sans ossature : sélectionnez des murs ou ouvrez un plan de leur niveau.
- de: Keine geraden Wände ohne Ständerwerk: Wählen Sie Wände aus oder öffnen Sie einen Grundriss ihrer Ebene.
- it: Nessun muro diritto senza telaio: selezionare dei muri o aprire una pianta del loro livello.
- lo: ບໍ່ມີຝາຊື່ທີ່ຍັງບໍ່ມີໂຄງ: ເລືອກຝາ ຫຼື ເປີດແຜນຜັງຂອງຊັ້ນຂອງມັນ.
- pl: Brak prostych ścian bez szkieletu: zaznacz ściany lub otwórz rzut ich poziomu.
- uk-UA: Немає прямих стін без каркаса: виділіть стіни або відкрийте план їхнього рівня.
- zh-TW: 沒有尚未加骨架的直牆：請選取牆，或開啟其樓層的平面圖。
- zh-CN: 没有尚未加骨架的直墙：请选择墙，或打开其楼层的平面图。
- ja: 下地のない直線の壁がありません。壁を選択するか、そのレベルの平面図を開いてください。

`msg-walls-framed` (T4, placeholders `$walls`, `$count`):
- pt-BR: { $walls } paredes estruturadas com { $count } peças.
- en: { $walls } walls framed with { $count } members.
- es: { $walls } muros entramados con { $count } piezas.
- fr: { $walls } murs ossaturés avec { $count } éléments.
- de: { $walls } Wände mit { $count } Bauteilen ausgefacht.
- it: { $walls } muri intelaiati con { $count } elementi.
- lo: ສ້າງໂຄງໃຫ້ { $walls } ຝາ ດ້ວຍ { $count } ຊິ້ນ.
- pl: Utworzono szkielety { $walls } ścian z { $count } elementów.
- uk-UA: Каркаси { $walls } стін створено з { $count } елементів.
- zh-TW: 已為 { $walls } 面牆建立骨架，共 { $count } 個構件。
- zh-CN: 已为 { $walls } 面墙创建骨架，共 { $count } 个构件。
- ja: { $walls } 枚の壁に { $count } 本の部材で下地を作成しました。

`cmd-desc-light-steel-frame` (T4):
- pt-BR: Estrutura as paredes selecionadas (ou as do nível ativo) em light steel frame: montantes formados a frio a cada 0,4 m entre guias, montantes duplos, vergas, contravergas e montantes curtos em portas e janelas, acompanhando a parede e suas aberturas.
- en: Frame the selected walls (or those of the active level) in light steel: cold-formed studs every 0.4 m between tracks, king and jack studs, headers, sills and cripples at doors and windows, kept in step with the wall and its openings.
- es: Entrama los muros seleccionados (o los del nivel activo) en acero ligero: montantes conformados en frío cada 0,4 m entre canales, montantes dobles, dinteles, antepechos y montantes cortos en puertas y ventanas, que siguen al muro y sus aberturas.
- fr: Ossature les murs sélectionnés (ou ceux du niveau actif) en acier léger : montants formés à froid tous les 0,4 m entre rails, montants doubles, linteaux, appuis et montants courts aux portes et fenêtres, tenus à jour avec le mur et ses ouvertures.
- de: Versieht die ausgewählten Wände (oder die der aktiven Ebene) mit Stahlleichtbau-Ständerwerk: kaltgeformte Ständer alle 0,4 m zwischen Schienen, Doppelständer, Stürze, Brüstungsriegel und kurze Ständer an Türen und Fenstern, die der Wand und ihren Öffnungen folgen.
- it: Intelaia i muri selezionati (o quelli del livello attivo) in acciaio leggero: montanti formati a freddo ogni 0,4 m tra le guide, montanti doppi, architravi, davanzali e montanti corti a porte e finestre, aggiornati con il muro e le sue aperture.
- lo: ສ້າງໂຄງເຫຼັກເບົາໃຫ້ຝາທີ່ເລືອກ (ຫຼື ຝາຂອງຊັ້ນປັດຈຸບັນ): ເສົາຕັ້ງເຫຼັກຂຶ້ນຮູບເຢັນທຸກ 0,4 m ລະຫວ່າງລາງ, ເສົາຄູ່, ຄານຂອບເທິງ, ຄານຂອບລຸ່ມ ແລະ ເສົາສັ້ນທີ່ປະຕູ ແລະ ປ່ອງຢ້ຽມ, ປ່ຽນຕາມຝາ ແລະ ຊ່ອງເປີດ.
- pl: Tworzy lekki szkielet stalowy zaznaczonych ścian (lub ścian aktywnego poziomu): słupki z kształtowników giętych co 0,4 m między prowadnicami, słupki podwójne, nadproża, podokienniki i krótkie słupki przy drzwiach i oknach, aktualizowane razem ze ścianą i jej otworami.
- uk-UA: Створює легкий сталевий каркас виділених стін (або стін активного рівня): холодногнуті стояки кожні 0,4 м між напрямними, подвійні стояки, перемички, підвіконні ригелі й короткі стояки біля дверей і вікон, що оновлюються разом зі стіною та її отворами.
- zh-TW: 為所選的牆（或目前樓層的牆）建立輕鋼構架：天地軌道之間每 0.4 m 一支冷軋立柱，門窗處有雙立柱、過梁、窗台梁與短柱，隨牆與開口更新。
- zh-CN: 为所选的墙（或当前楼层的墙）创建轻钢龙骨：天地龙骨之间每 0.4 m 一根冷弯立柱，门窗处有双立柱、过梁、窗台梁和短柱，随墙及洞口更新。
- ja: 選択した壁（または現在のレベルの壁）に軽量鉄骨下地を作成します。ランナー間に 0.4 m ごとの冷間成形間柱、開口部には二重の柱、まぐさ、窓台、短い間柱を配置し、壁と開口部に合わせて更新します。

`cmd-desc-wood-frame` (T4):
- pt-BR: Estrutura as paredes selecionadas (ou as do nível ativo) em wood frame: montantes a cada 0,6 m entre soleira e frechal duplo, montantes duplos, vergas, contravergas e montantes curtos em portas e janelas, acompanhando a parede e suas aberturas.
- en: Frame the selected walls (or those of the active level) in timber: studs every 0.6 m between a sole plate and a double top plate, king and jack studs, headers, sills and cripples at doors and windows, kept in step with the wall and its openings.
- es: Entrama los muros seleccionados (o los del nivel activo) en madera: montantes cada 0,6 m entre solera inferior y solera superior doble, montantes dobles, dinteles, antepechos y montantes cortos en puertas y ventanas, que siguen al muro y sus aberturas.
- fr: Ossature les murs sélectionnés (ou ceux du niveau actif) en bois : montants tous les 0,6 m entre lisse basse et double sablière, montants doubles, linteaux, appuis et montants courts aux portes et fenêtres, tenus à jour avec le mur et ses ouvertures.
- de: Versieht die ausgewählten Wände (oder die der aktiven Ebene) mit Holzrahmenbau-Ständerwerk: Ständer alle 0,6 m zwischen Schwelle und doppeltem Rähm, Doppelständer, Stürze, Brüstungsriegel und kurze Ständer an Türen und Fenstern, die der Wand und ihren Öffnungen folgen.
- it: Intelaia i muri selezionati (o quelli del livello attivo) in legno: montanti ogni 0,6 m tra corrente inferiore e doppio corrente superiore, montanti doppi, architravi, davanzali e montanti corti a porte e finestre, aggiornati con il muro e le sue aperture.
- lo: ສ້າງໂຄງໄມ້ໃຫ້ຝາທີ່ເລືອກ (ຫຼື ຝາຂອງຊັ້ນປັດຈຸບັນ): ເສົາຕັ້ງທຸກ 0,6 m ລະຫວ່າງແຜ່ນລຸ່ມ ແລະ ແຜ່ນເທິງຄູ່, ເສົາຄູ່, ຄານຂອບເທິງ, ຄານຂອບລຸ່ມ ແລະ ເສົາສັ້ນທີ່ປະຕູ ແລະ ປ່ອງຢ້ຽມ, ປ່ຽນຕາມຝາ ແລະ ຊ່ອງເປີດ.
- pl: Tworzy szkielet drewniany zaznaczonych ścian (lub ścian aktywnego poziomu): słupki co 0,6 m między podwaliną a podwójnym oczepem, słupki podwójne, nadproża, podokienniki i krótkie słupki przy drzwiach i oknach, aktualizowane razem ze ścianą i jej otworami.
- uk-UA: Створює дерев'яний каркас виділених стін (або стін активного рівня): стояки кожні 0,6 м між нижньою та подвійною верхньою обв'язкою, подвійні стояки, перемички, підвіконні ригелі й короткі стояки біля дверей і вікон, що оновлюються разом зі стіною та її отворами.
- zh-TW: 為所選的牆（或目前樓層的牆）建立木構架：底板與雙頂板之間每 0.6 m 一支立柱，門窗處有雙立柱、過梁、窗台梁與短柱，隨牆與開口更新。
- zh-CN: 为所选的墙（或当前楼层的墙）创建木骨架：底梁与双顶梁之间每 0.6 m 一根立柱，门窗处有双立柱、过梁、窗台梁和短柱，随墙及洞口更新。
- ja: 選択した壁（または現在のレベルの壁）に木造枠組下地を作成します。土台とダブル頭つなぎの間に 0.6 m ごとの間柱、開口部には二重の柱、まぐさ、窓台、短い間柱を配置し、壁と開口部に合わせて更新します。

Existing keys reused (do not add): `prop-members`, `prop-detached-members`, `action-reset-members`, `opt-catalog-default`, `section-type`, `action-ok`, `action-cancel`.

---

### Task 1: The wall framing model and its layout (cc_core)

**Files:**
- Create: `crates/cc_core/src/wall_framing.rs`
- Modify:
  - `crates/cc_core/src/lib.rs`: `pub mod wall_framing;` right before `pub mod wall_ops;`.
  - `crates/cc_core/src/element.rs`:
    - the variant `/// Light steel or wood framing of a wall: it owns the studs and the beams of its tracks, headers and sills.` `WallFraming(Box<crate::wall_framing::WallFraming>),` after `Truss(..)`;
    - the `Element::category` arm `ElementKind::WallFraming(_) => Category::WallFraming,`;
    - the `Element::translate` arm `// Placed by its wall.` `ElementKind::WallFraming(_) => {}`.
  - `crates/cc_core/src/transform.rs`: the `Element::transform` arm `ElementKind::WallFraming(_) => {}` (placed by its wall).
  - `crates/cc_core/src/types.rs`:
    - `Category::WallFraming` (doc `/// Light steel and wood framing of walls: hosts of studs, tracks, headers and sills.`) right after `Category::Truss`;
    - `Category::ALL` grows by one (`[Category; 37]` on top of the truss branch's 36), with `Category::WallFraming` after `Category::Truss`. It goes in neither `MODEL` nor `SCHEDULABLE`;
    - `Category::key`: `Category::WallFraming => "category-wall-framing",`;
    - `TypeData::WallFraming {}` (doc `/// Wall framings (no properties of their own; types name them).`) after `Truss {}`;
    - every `TypeData` or-pattern listing `TypeData::Truss {}` also lists `| TypeData::WallFraming {}`, and the category match gets `TypeData::WallFraming {} => Category::WallFraming,`.
  - `crates/cc_core/src/repair.rs`:
    - the `element_is_valid` arm `// Its numbers are reset by repair_member_hosts; its wall is checked with the hosts.` `ElementKind::WallFraming(_) => true,`;
    - `| TypeData::WallFraming {}` next to `| TypeData::Truss {}`.
  - `crates/cc_core/src/schedule.rs` (the arms that list `Category::SteelBuilding`/`Truss`): `Category::WallFraming` with them.
  - Other crates' exhaustive matches (run `cargo check --workspace --tests` to list them; known ones):
    - `crates/cc_views/src/legend.rs` and `crates/cc_ifc/src/export.rs`: `| TypeData::WallFraming {}` next to `TypeData::Truss {}`;
    - `crates/cc_app/src/commands.rs`, the two category functions: `Category::WallFraming => t!("category-wall-framing"),` in the name one, and in the description one `Category::WallFraming => t!("category-wall-framing"),` for now (Task 4 changes it to `cmd-desc-light-steel-frame`);
    - `crates/cc_app/src/tools.rs`: add `| Category::WallFraming` to the `return None` arm;
    - `crates/cc_app/src/ribbon.rs` `category_icon`: `Category::WallFraming => Icon::SteelBuilding,` for now (Task 4 adds `Icon::WallFraming`);
    - `crates/cc_app/src/properties.rs`: `ElementKind::WallFraming(_) => {}` for now (Task 4 fills it);
    - `crates/cc_engine/src/commands/query.rs`: nothing yet (Task 3).
  - `assets/i18n/*.ftl` (12): `category-wall-framing`, `tpl-type-wall-framing`, `framing-light-steel`, `framing-wood` from the table.
- Test: `crates/cc_core/src/wall_framing.rs` (`mod tests`).

**Interfaces:**
- Consumes:
  - `member_host::{reset, within}` (truss plan, Task 1);
  - `steel_building::section_box(&Project, ElementTypeId) -> (f64, f64)` (width across x, depth up y; `MISSING_SECTION` when unreadable);
  - `Project::{wall_axis, wall_base_z, wall_top_z, level_elevation, hosted_by, element, element_type}`;
  - `ElementType::opening_size`, `Wall::arc`, `Beam::new`, `Column`.
- Produces (module `cc_core::wall_framing`):
  - consts `MAX_MEMBERS: usize = 4000`, `MIN_PIECE: f64 = 0.05`, `END_STUD: u16 = u16::MAX`, `SPACING: RangeInclusive<f64> = 0.2..=1.2`, `LIGHT_STEEL_SPACING = 0.4`, `WOOD_SPACING = 0.6`.
  - `enum FramingSystem { LightSteel (default), Wood }` with `ALL`, `code`, `from_code`, `key`, `default_spacing`, `default_double_top`.
  - `enum FramingRoleKind { BottomTrack, TopTrack, Stud, KingStud, JackStud, Header, Sill, Cripple, Blocking }` with `ALL: [_; 9]` and `is_stud()`.
  - `struct FramingRole { pub kind: FramingRoleKind, pub opening: u16, pub index: u16 }` (`Ord`) with `fits(&ElementKind) -> bool`; `struct FramingMember { pub role: FramingRole, pub id: ElementId }`.
  - `struct WallFraming { pub wall, pub system, pub stud_type, pub track_type, pub header_type, pub spacing, pub double_top, pub blocking, pub members: Vec<FramingMember>, pub detached: Vec<FramingRole> }` with:
    - `new(wall: ElementId, system: FramingSystem, stud: ElementTypeId, track: ElementTypeId, header: ElementTypeId) -> Self`;
    - `top_plates() -> u16`, `check() -> Result<(), String>`, `check_types(&Project) -> Result<(), String>`, `check_wall(&Project) -> Result<(), String>`;
    - `reset_out_of_range() -> usize`, `type_ids() -> impl Iterator<Item = ElementTypeId>`, `wall_length(&Project) -> f64`;
    - `layout(&Project) -> Vec<(FramingRole, ElementTypeId, ElementKind)>`.
  - `host_type(id: ElementTypeId, name: String) -> ElementType`.
  - `ElementKind::WallFraming(Box<WallFraming>)`, `Category::WallFraming`, `TypeData::WallFraming {}`.
  - Test helpers `pub(crate)`: `tests::{Site, site_with, add_opening, framing}`.

- [ ] **Step 1: Write the failing tests.** Create `crates/cc_core/src/wall_framing.rs` with only the module doc, the `use` lines and this test module. Then add the module line to `lib.rs` so it compiles to a failure:

```rust
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{ColumnSection, Document, Element, Hosted, LevelId, Wall};

    fn add_type(e: &mut crate::EditCtx, data: TypeData) -> ElementTypeId {
        let id = ElementTypeId(e.alloc_id());
        e.add_type(ElementType { id, name: format!("T{}", id.0), data, info: Default::default(), uid: Default::default() })
    }

    pub(crate) fn element(id: ElementId, level: LevelId, type_id: ElementTypeId, kind: ElementKind) -> Element {
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

    pub(crate) struct Site {
        pub doc: Document,
        pub wall: ElementId,
        pub host: ElementId,
        /// A 1.2 x 1.2 window type, sill 0.9.
        pub window: ElementTypeId,
        /// A 0.9 x 2.1 door type.
        pub door: ElementTypeId,
    }

    /// A wall 3 m high from (0, 0) to (`length`, 0) on the ground level,
    /// framed (light steel, 0.4 m) with 0.04 x 0.09 studs, 0.04 deep tracks
    /// and 0.14 deep headers; `f` changes the framing before it is added.
    pub(crate) fn site_with(length: f64, f: impl FnOnce(&mut WallFraming)) -> Site {
        let mut doc = Document::new(Project::from_template());
        let (wall, host, window, door) = doc.edit("site", |e| {
            let level = e.project().levels[0].id;
            let material = e.project().materials[0].name.clone();
            let wall_type = e.project().default_type(Category::Wall).expect("a wall type");
            let beam = |width: f64, height: f64| TypeData::Beam {
                width,
                height,
                material: material.clone(),
                shape: Default::default(),
                profile: None,
                profile_rotation_deg: 0.0,
                profile_mirror: false,
            };
            let track = add_type(e, beam(0.09, 0.04));
            let header = add_type(e, beam(0.09, 0.14));
            let stud = add_type(
                e,
                TypeData::Column {
                    section: ColumnSection::Rectangular { width: 0.04, depth: 0.09 },
                    material: material.clone(),
                    profile: None,
                    profile_rotation_deg: 0.0,
                    profile_mirror: false,
                },
            );
            let window = add_type(
                e,
                TypeData::Window { width: 1.2, height: 1.2, default_sill: 0.9, material: material.clone() },
            );
            let door = add_type(e, TypeData::Door { width: 0.9, height: 2.1, material: material.clone() });
            let host_type = add_type(e, TypeData::WallFraming {});
            let wall = ElementId(e.alloc_id());
            let w = Wall { start: DVec2::ZERO, end: DVec2::new(length, 0.0), height: 3.0, ..Default::default() };
            e.add_element(element(wall, level, wall_type, ElementKind::Wall(w)));
            let mut framing = WallFraming::new(wall, FramingSystem::LightSteel, stud, track, header);
            f(&mut framing);
            let host = ElementId(e.alloc_id());
            e.add_element(element(host, level, host_type, ElementKind::WallFraming(Box::new(framing))));
            (wall, host, window, door)
        });
        Site { doc, wall, host, window, door }
    }

    /// Add a window (`type_id == site.window`, sill 0.9) or a door centered
    /// `offset` along the site's wall.
    pub(crate) fn add_opening(site: &mut Site, type_id: ElementTypeId, offset: f64) -> ElementId {
        let (wall, window) = (site.wall, type_id == site.window);
        site.doc.edit("opening", |e| {
            let level = e.project().levels[0].id;
            let sill_height = if window { 0.9 } else { 0.0 };
            let hosted = Hosted { host: wall, offset, sill_height, flip_facing: false, flip_hand: false };
            let kind = if window { ElementKind::Window(hosted) } else { ElementKind::Door(hosted) };
            let id = ElementId(e.alloc_id());
            e.add_element(element(id, level, type_id, kind))
        })
    }

    pub(crate) fn framing(doc: &Document, host: ElementId) -> WallFraming {
        match doc.project.element(host).map(|e| &e.kind) {
            Some(ElementKind::WallFraming(f)) => (**f).clone(),
            _ => panic!("not a wall framing"),
        }
    }

    fn layout(site: &Site) -> Vec<(FramingRole, ElementTypeId, ElementKind)> {
        framing(&site.doc, site.host).layout(&site.doc.project)
    }

    /// Members per kind, in `FramingRoleKind::ALL` order: [bottom tracks,
    /// top tracks, studs, king studs, jack studs, headers, sills, cripples, blocking].
    fn counts(site: &Site) -> [usize; 9] {
        let l = layout(site);
        FramingRoleKind::ALL.map(|k| l.iter().filter(|(r, _, _)| r.kind == k).count())
    }

    fn find(site: &Site, kind: FramingRoleKind, opening: u16, index: u16) -> ElementKind {
        layout(site)
            .into_iter()
            .find(|(r, _, _)| *r == FramingRole { kind, opening, index })
            .map(|(_, _, k)| k)
            .unwrap_or_else(|| panic!("no {kind:?} {opening} {index}"))
    }

    fn column(k: ElementKind) -> Column {
        match k {
            ElementKind::Column(c) => c,
            other => panic!("not a column: {other:?}"),
        }
    }

    fn beam(k: ElementKind) -> Beam {
        match k {
            ElementKind::Beam(b) => b,
            other => panic!("not a beam: {other:?}"),
        }
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn a_bare_wall_gets_studs_at_the_spacing_between_tracks() {
        let site = site_with(3.6, |_| {});
        // 0, 0.4, ..., 3.2 and the end stud.
        assert_eq!(counts(&site), [1, 1, 10, 0, 0, 0, 0, 0, 0]);
        let first = column(find(&site, FramingRoleKind::Stud, 0, 0));
        assert!(near(first.position.x, 0.02) && near(first.position.y, 0.0), "flush with the wall start");
        assert!(near(first.base_offset, 0.04) && near(first.height, 2.92), "between the tracks");
        assert!(near(first.rotation_deg, 0.0), "turned with the wall");
        assert!(near(column(find(&site, FramingRoleKind::Stud, 0, 1)).position.x, 0.4));
        assert!(near(column(find(&site, FramingRoleKind::Stud, 0, END_STUD)).position.x, 3.58));
        let bottom = beam(find(&site, FramingRoleKind::BottomTrack, 0, 0));
        assert!(near(bottom.start.x, 0.0) && near(bottom.end.x, 3.6) && near(bottom.top_offset, 0.04));
        assert!(!bottom.join_start && !bottom.join_end, "tracks keep their length");
        assert!(near(beam(find(&site, FramingRoleKind::TopTrack, 0, 0)).top_offset, 3.0));
    }

    #[test]
    fn a_window_gets_kings_jacks_a_header_a_sill_and_cripples() {
        let mut site = site_with(3.6, |_| {});
        let window = site.window;
        add_opening(&mut site, window, 1.8);
        // The studs at 1.2, 1.6, 2.0 and 2.4 give way to the opening's framing.
        assert_eq!(counts(&site), [1, 1, 6, 2, 2, 1, 1, 4, 0]);
        for k in 3..=6 {
            assert!(layout(&site).iter().all(|(r, _, _)| *r != FramingRole { kind: FramingRoleKind::Stud, opening: 0, index: k }));
        }
        assert!(near(column(find(&site, FramingRoleKind::KingStud, 0, 0)).position.x, 1.14));
        assert!(near(column(find(&site, FramingRoleKind::KingStud, 0, 1)).position.x, 2.46));
        let jack = column(find(&site, FramingRoleKind::JackStud, 0, 1));
        assert!(near(jack.position.x, 2.42) && near(jack.base_offset, 0.04) && near(jack.height, 2.06));
        let header = beam(find(&site, FramingRoleKind::Header, 0, 0));
        assert!(near(header.start.x, 1.16) && near(header.end.x, 2.44) && near(header.top_offset, 2.24));
        let sill = beam(find(&site, FramingRoleKind::Sill, 0, 0));
        assert!(near(sill.start.x, 1.2) && near(sill.end.x, 2.4) && near(sill.top_offset, 0.9));
        // At 1.6 (k = 4): above the header (index 8) and below the sill (index 9).
        let above = column(find(&site, FramingRoleKind::Cripple, 0, 8));
        assert!(near(above.position.x, 1.6) && near(above.base_offset, 2.24) && near(above.height, 0.72));
        let below = column(find(&site, FramingRoleKind::Cripple, 0, 9));
        assert!(near(below.base_offset, 0.04) && near(below.height, 0.72));
    }

    #[test]
    fn a_door_has_no_sill_and_no_cripples_below() {
        let mut site = site_with(3.6, |_| {});
        let door = site.door;
        add_opening(&mut site, door, 1.0);
        assert_eq!(counts(&site), [1, 1, 8, 2, 2, 1, 0, 2, 0]);
        assert!(near(column(find(&site, FramingRoleKind::JackStud, 0, 0)).height, 2.06));
    }

    #[test]
    fn wood_gets_a_double_top_plate_and_light_steel_does_not() {
        let site = site_with(3.6, |f| (f.system, f.double_top) = (FramingSystem::Wood, true));
        assert_eq!(counts(&site)[1], 2);
        assert!(near(beam(find(&site, FramingRoleKind::TopTrack, 0, 1)).top_offset, 2.96));
        assert!(near(column(find(&site, FramingRoleKind::Stud, 0, 0)).height, 2.88));
        let site = site_with(3.6, |f| f.double_top = true);
        assert_eq!(counts(&site)[1], 1, "double top is wood only");
        assert!(FramingSystem::Wood.default_double_top() && !FramingSystem::LightSteel.default_double_top());
    }

    #[test]
    fn blocking_runs_between_full_height_studs_but_not_across_openings() {
        let mut site = site_with(3.6, |f| f.blocking = true);
        assert_eq!(counts(&site)[8], 9);
        let first = beam(find(&site, FramingRoleKind::Blocking, 0, 0));
        assert!(near(first.start.x, 0.04) && near(first.end.x, 0.38) && near(first.top_offset, 1.52));
        let window = site.window;
        add_opening(&mut site, window, 1.8);
        assert_eq!(counts(&site)[8], 6);
        // After the end-side king of opening 0.
        let after_king = beam(find(&site, FramingRoleKind::Blocking, 1, 0));
        assert!(near(after_king.start.x, 2.48));
    }

    #[test]
    fn an_opening_at_the_wall_start_is_clipped() {
        let mut site = site_with(3.6, |_| {});
        let window = site.window;
        add_opening(&mut site, window, 0.3);
        let c = counts(&site);
        assert_eq!((c[3], c[4]), (1, 1), "only the end-side king and jack");
        assert!(near(beam(find(&site, FramingRoleKind::Header, 0, 0)).start.x, 0.0));
    }

    #[test]
    fn curved_short_and_out_of_range_get_nothing() {
        let mut site = site_with(3.6, |_| {});
        let wall = site.wall;
        site.doc.edit("bend", |e| {
            if let Some(ElementKind::Wall(w)) = e.element_mut(wall).map(|x| &mut x.kind) {
                w.bulge = 0.2;
            }
        });
        assert!(layout(&site).is_empty(), "curved");
        assert!(layout(&site_with(0.05, |_| {})).is_empty(), "shorter than two studs");
        let mut f = framing(&site.doc, site.host);
        for bad in [0.1, 1.3, f64::NAN] {
            f.spacing = bad;
            assert!(f.check().is_err() && f.layout(&site.doc.project).is_empty(), "{bad}");
        }
        assert_eq!(f.reset_out_of_range(), 1);
        assert!(near(f.spacing, LIGHT_STEEL_SPACING));
        (f.system, f.spacing) = (FramingSystem::Wood, 5.0);
        f.reset_out_of_range();
        assert!(near(f.spacing, WOOD_SPACING));
    }

    #[test]
    fn layouts_stop_at_the_limit() {
        let site = site_with(2000.0, |f| f.spacing = 0.2);
        assert_eq!(layout(&site).len(), MAX_MEMBERS);
    }

    #[test]
    fn systems_and_roles() {
        for s in FramingSystem::ALL {
            assert_eq!(FramingSystem::from_code(s.code()), Some(s));
        }
        assert_eq!(FramingSystem::from_code(" WOOD "), Some(FramingSystem::Wood));
        assert_eq!(FramingSystem::from_code("steel"), None);
        let stud = FramingRole { kind: FramingRoleKind::Stud, opening: 0, index: 1 };
        assert!(stud.fits(&ElementKind::Column(column(find(&site_with(3.6, |_| {}), FramingRoleKind::Stud, 0, 1)))));
        let header = FramingRole { kind: FramingRoleKind::Header, opening: 0, index: 0 };
        assert!(!header.fits(&ElementKind::Column(column(find(&site_with(3.6, |_| {}), FramingRoleKind::Stud, 0, 1)))));
    }
}
```

- [ ] **Step 2: Run them to see them fail.** Run `cargo nextest run -p cc_core wall_framing`. Expected: compile errors (`WallFraming`, `FramingSystem`, `TypeData::WallFraming`, … are missing).

- [ ] **Step 3: Write the module** above the tests:

```rust
//! Light steel and wood framing of walls: a host element attached to one
//! straight wall that owns ordinary columns (studs, king and jack studs,
//! cripples) and beams (tracks or plates, headers, sills, blocking) laid
//! out from its parameters, its wall and the doors and windows the wall
//! hosts. They are kept in step like a steel building's members (see
//! [`crate::member_host`]), and any change of the wall or its openings
//! regenerates them.
//!
//! The wall frame: `u` along the wall's body centerline
//! ([`Project::wall_axis`]) from its start to its length `L`, heights above
//! the wall's level.

use crate::member_host::{reset, within};
use crate::steel_building::section_box;
use crate::{Beam, Category, Column, ElementId, ElementKind, ElementType, ElementTypeId, Project, TypeData, WallTop};
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;

/// Most members one framing makes; the layout stops there.
pub const MAX_MEMBERS: usize = 4000;
/// A jack, cripple or blocking piece shorter than this is left out, meters.
pub const MIN_PIECE: f64 = 0.05;
/// The index of the stud at the wall's end (its key does not depend on the length).
pub const END_STUD: u16 = u16::MAX;
pub const SPACING: RangeInclusive<f64> = 0.2..=1.2;
pub const LIGHT_STEEL_SPACING: f64 = 0.4;
pub const WOOD_SPACING: f64 = 0.6;

/// How the wall is framed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum FramingSystem {
    /// Cold-formed steel studs between tracks.
    #[default]
    LightSteel,
    /// Sawn timber studs between plates.
    Wood,
}

impl FramingSystem {
    pub const ALL: [FramingSystem; 2] = [FramingSystem::LightSteel, FramingSystem::Wood];

    pub fn code(self) -> &'static str {
        match self {
            FramingSystem::LightSteel => "light_steel",
            FramingSystem::Wood => "wood",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.code().eq_ignore_ascii_case(code.trim()))
    }

    /// The i18n key of its name.
    pub fn key(self) -> &'static str {
        match self {
            FramingSystem::LightSteel => "framing-light-steel",
            FramingSystem::Wood => "framing-wood",
        }
    }

    pub fn default_spacing(self) -> f64 {
        match self {
            FramingSystem::LightSteel => LIGHT_STEEL_SPACING,
            FramingSystem::Wood => WOOD_SPACING,
        }
    }

    /// Wood frames get a double top plate by default.
    pub fn default_double_top(self) -> bool {
        self == FramingSystem::Wood
    }
}

/// What a member is in its framing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FramingRoleKind {
    BottomTrack,
    TopTrack,
    Stud,
    KingStud,
    JackStud,
    Header,
    Sill,
    Cripple,
    Blocking,
}

impl FramingRoleKind {
    pub const ALL: [FramingRoleKind; 9] = [
        FramingRoleKind::BottomTrack,
        FramingRoleKind::TopTrack,
        FramingRoleKind::Stud,
        FramingRoleKind::KingStud,
        FramingRoleKind::JackStud,
        FramingRoleKind::Header,
        FramingRoleKind::Sill,
        FramingRoleKind::Cripple,
        FramingRoleKind::Blocking,
    ];

    /// Studs are columns; tracks, headers, sills and blocking are beams.
    pub fn is_stud(self) -> bool {
        matches!(
            self,
            FramingRoleKind::Stud | FramingRoleKind::KingStud | FramingRoleKind::JackStud | FramingRoleKind::Cripple
        )
    }
}

/// A member's place in its framing:
/// - tracks: index 0 (the top plates from the topmost down);
/// - studs: index `k` for the stud at `k · spacing`, 0 at the wall start, [`END_STUD`] at its end;
/// - king and jack studs, headers, sills: `opening` is the opening's order along the wall, index 0 on the start side, 1 on the end side;
/// - cripples: index `2k` above the header and `2k + 1` below the sill;
/// - blocking: keyed by its left stud, `(0, k)` after stud `k` and `(j + 1, 0)`/`(j + 1, 1)` after opening `j`'s end-side/start-side king.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FramingRole {
    pub kind: FramingRoleKind,
    pub opening: u16,
    pub index: u16,
}

impl FramingRole {
    pub fn fits(self, kind: &ElementKind) -> bool {
        if self.kind.is_stud() { matches!(kind, ElementKind::Column(_)) } else { matches!(kind, ElementKind::Beam(_)) }
    }
}

/// A member the framing manages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FramingMember {
    pub role: FramingRole,
    pub id: ElementId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WallFraming {
    /// The framed wall (straight; one framing per wall).
    pub wall: ElementId,
    #[serde(default)]
    pub system: FramingSystem,
    /// Column type of the studs, king and jack studs and cripples.
    pub stud_type: ElementTypeId,
    /// Beam type of the bottom and top tracks or plates, and of the blocking.
    pub track_type: ElementTypeId,
    /// Beam type of the headers and sills.
    pub header_type: ElementTypeId,
    /// Stud spacing on center from the wall start, meters.
    pub spacing: f64,
    /// An extra top plate (wood only; ignored for light steel).
    #[serde(default)]
    pub double_top: bool,
    /// A row of blocking at mid-height between full-height studs.
    #[serde(default)]
    pub blocking: bool,
    #[serde(default)]
    pub members: Vec<FramingMember>,
    #[serde(default)]
    pub detached: Vec<FramingRole>,
}

/// The type a project gets with its first wall framing.
pub fn host_type(id: ElementTypeId, name: String) -> ElementType {
    ElementType { id, name, data: TypeData::WallFraming {}, info: Default::default(), uid: Default::default() }
}

/// Members collected by a layout, stopping at [`MAX_MEMBERS`].
struct Members(Vec<(FramingRole, ElementTypeId, ElementKind)>);

impl Members {
    fn push(&mut self, kind: FramingRoleKind, opening: u16, index: u16, type_id: ElementTypeId, element: ElementKind) {
        if self.0.len() < MAX_MEMBERS {
            self.0.push((FramingRole { kind, opening, index }, type_id, element));
        }
    }
}

/// The wall frame: the centerline's start and direction, its length, and
/// the wall's base and top above the wall's level.
#[derive(Debug, Clone, Copy)]
struct Frame {
    origin: DVec2,
    dir: DVec2,
    length: f64,
    base: f64,
    top: f64,
}

/// A door or window in the wall frame: its order along the wall, its span
/// clipped to the wall, and its sill and head above the wall's level.
#[derive(Debug, Clone, Copy)]
struct Opening {
    index: u16,
    u0: f64,
    u1: f64,
    sill: f64,
    head: f64,
    window: bool,
}

impl WallFraming {
    /// A framing of `wall` with the system's default spacing and top plates, no blocking.
    pub fn new(
        wall: ElementId,
        system: FramingSystem,
        stud_type: ElementTypeId,
        track_type: ElementTypeId,
        header_type: ElementTypeId,
    ) -> Self {
        Self {
            wall,
            system,
            stud_type,
            track_type,
            header_type,
            spacing: system.default_spacing(),
            double_top: system.default_double_top(),
            blocking: false,
            members: Vec::new(),
            detached: Vec::new(),
        }
    }

    /// Top tracks or plates: two for a wood frame with `double_top`, else one.
    pub fn top_plates(&self) -> u16 {
        if self.system == FramingSystem::Wood && self.double_top { 2 } else { 1 }
    }

    /// Whether the spacing is in range.
    pub fn check(&self) -> Result<(), String> {
        within("spacing", self.spacing, &SPACING)
    }

    /// Whether the stud type is a column type and the track and header types beam types.
    pub fn check_types(&self, project: &Project) -> Result<(), String> {
        let is = |t: ElementTypeId, c: Category| project.element_type(t).is_some_and(|x| x.category() == c);
        if !is(self.stud_type, Category::Column) {
            return Err(format!("type {} is not a column type", self.stud_type.0));
        }
        match [self.track_type, self.header_type].into_iter().find(|t| !is(*t, Category::Beam)) {
            Some(t) => Err(format!("type {} is not a beam type", t.0)),
            None => Ok(()),
        }
    }

    /// Whether its wall is a wall of the project.
    pub fn check_wall(&self, project: &Project) -> Result<(), String> {
        match project.element(self.wall) {
            Some(e) if e.as_wall().is_some() => Ok(()),
            _ => Err(format!("element {} is not a wall", self.wall.0)),
        }
    }

    /// Put an out-of-range spacing back to the system's default; returns how many (0 or 1).
    pub fn reset_out_of_range(&mut self) -> usize {
        reset(&mut self.spacing, &SPACING, self.system.default_spacing())
    }

    /// The member types it uses.
    pub fn type_ids(&self) -> impl Iterator<Item = ElementTypeId> + '_ {
        [self.stud_type, self.track_type, self.header_type].into_iter()
    }

    /// Its wall's centerline length (0 when the wall is gone).
    pub fn wall_length(&self, project: &Project) -> f64 {
        project.element(self.wall).and_then(|e| e.as_wall().map(|w| project.wall_length(e, w))).unwrap_or(0.0)
    }

    /// The wall frame and the wall's openings; `None` when the wall is gone,
    /// curved or degenerate.
    fn frame(&self, project: &Project) -> Option<(Frame, Vec<Opening>)> {
        let e = project.element(self.wall)?;
        let w = e.as_wall()?;
        if w.arc().is_some() {
            return None;
        }
        let (a, b) = project.wall_axis(e, w);
        let length = a.distance(b);
        let dir = (b - a).try_normalize()?;
        let floor = project.level_elevation(e.level);
        let (base, top) = (project.wall_base_z(e, w) - floor, project.wall_top_z(e, w) - floor);
        if !(length.is_finite() && base.is_finite() && top.is_finite()) {
            return None;
        }
        let mut hosted: Vec<(&crate::Element, &crate::Hosted)> =
            project.hosted_by(self.wall).filter_map(|o| o.as_hosted().map(|h| (o, h))).collect();
        hosted.sort_by(|x, y| x.1.offset.total_cmp(&y.1.offset).then(x.0.id.cmp(&y.0.id)));
        let mut openings = Vec::new();
        for (j, (o, h)) in hosted.into_iter().enumerate() {
            let Ok(index) = u16::try_from(j) else { break };
            let Some((width, height)) = project.element_type(o.type_id).and_then(|t| t.opening_size()) else {
                continue;
            };
            if !(width.is_finite() && height.is_finite() && h.offset.is_finite() && width > 0.0 && height > 0.0) {
                continue;
            }
            let window = matches!(o.kind, ElementKind::Window(_));
            let sill = if window && h.sill_height.is_finite() { h.sill_height.max(0.0) } else { 0.0 };
            let u0 = (h.offset - width / 2.0).clamp(0.0, length);
            let u1 = (h.offset + width / 2.0).clamp(0.0, length);
            if u1 - u0 > 1e-6 {
                openings.push(Opening { index, u0, u1, sill: base + sill, head: base + sill + height, window });
            }
        }
        Some((Frame { origin: a, dir, length, base, top }, openings))
    }

    /// The regular stud positions `(index, u)`: the start stud flush with the
    /// wall start, stud `k` at `k · spacing` while it clears the stud before
    /// it and the end stud by a stud width, and the end stud flush with the end.
    fn positions(&self, length: f64, w: f64) -> Vec<(u16, f64)> {
        let s = w / 2.0;
        let end = length - s;
        let mut out = vec![(0, s)];
        for k in 1..END_STUD {
            let u = f64::from(k) * self.spacing;
            if u > end - w + 1e-9 {
                break;
            }
            if out.last().is_some_and(|&(_, last)| u - last >= w - 1e-9) {
                out.push((k, u));
            }
        }
        out.push((END_STUD, end));
        out
    }

    /// Its members: role, type and element, at most [`MAX_MEMBERS`]. Empty
    /// when the spacing is out of range or the wall is gone, curved, shorter
    /// than two studs or too low for its tracks.
    pub fn layout(&self, project: &Project) -> Vec<(FramingRole, ElementTypeId, ElementKind)> {
        use FramingRoleKind as K;
        let mut out = Members(Vec::new());
        if self.check().is_err() {
            return out.0;
        }
        let Some((f, openings)) = self.frame(project) else { return out.0 };
        let (w, _) = section_box(project, self.stud_type);
        let (_, t) = section_box(project, self.track_type);
        let (_, hd) = section_box(project, self.header_type);
        let s = w / 2.0;
        let plates = self.top_plates();
        // Studs stand on the bottom track and stop under the top ones.
        let (bottom, top) = (f.base + t, f.top - f64::from(plates) * t);
        if f.length < 2.0 * w || top - bottom < MIN_PIECE {
            return out.0;
        }
        // The section's width (local x) along the wall, its depth (local y) across it.
        let angle = f.dir.to_angle().to_degrees();
        let at = |u: f64| f.origin + f.dir * u;
        let stud = |u: f64, z0: f64, z1: f64| {
            ElementKind::Column(Column {
                position: at(u),
                rotation_deg: angle,
                base_offset: z0,
                height: z1 - z0,
                top: WallTop::Unconnected,
                structural: true,
            })
        };
        let beam = |u0: f64, u1: f64, top_z: f64| {
            let mut b = Beam::new(at(u0), at(u1), top_z);
            (b.join_start, b.join_end) = (false, false);
            ElementKind::Beam(b)
        };
        let (studs, tracks, headers) = (self.stud_type, self.track_type, self.header_type);
        out.push(K::BottomTrack, 0, 0, tracks, beam(0.0, f.length, bottom));
        for i in 0..plates {
            out.push(K::TopTrack, 0, i, tracks, beam(0.0, f.length, f.top - f64::from(i) * t));
        }
        let positions = self.positions(f.length, w);
        // What an opening's framing takes: its jacks and kings on both sides.
        let taken =
            |u: f64| openings.iter().any(|o| u + s > o.u0 - 2.0 * w + 1e-9 && u - s < o.u1 + 2.0 * w - 1e-9);
        // Full-height studs, with the key of the blocking that starts at each.
        let mut full: Vec<(f64, Option<(u16, u16)>)> = Vec::new();
        for &(k, u) in &positions {
            if !taken(u) {
                out.push(K::Stud, 0, k, studs, stud(u, bottom, top));
                full.push((u, Some((0, k))));
            }
        }
        let on_wall = |u: f64| u >= s - 1e-9 && u <= f.length - s + 1e-9;
        for o in &openings {
            let j = o.index;
            let elsewhere = |u: f64| openings.iter().any(|x| x.index != j && u > x.u0 && u < x.u1);
            // The header's underside: the opening's head, kept under the top tracks.
            let head = o.head.min(top - hd);
            for (side, sign, edge) in [(0u16, -1.0, o.u0), (1, 1.0, o.u1)] {
                let king = edge + sign * 3.0 * s;
                if on_wall(king) && !elsewhere(king) {
                    out.push(K::KingStud, j, side, studs, stud(king, bottom, top));
                    full.push((king, j.checked_add(1).map(|g| (g, 1 - side))));
                }
                let jack = edge + sign * s;
                if on_wall(jack) && !elsewhere(jack) && head - bottom >= MIN_PIECE {
                    out.push(K::JackStud, j, side, studs, stud(jack, bottom, head));
                }
            }
            if head - bottom >= MIN_PIECE {
                out.push(K::Header, j, 0, headers, beam((o.u0 - w).max(0.0), (o.u1 + w).min(f.length), head + hd));
            }
            // A window's sill, when it clears the bottom track.
            let sill = (o.window && o.sill - hd >= bottom - 1e-9).then_some(o.sill);
            if let Some(z) = sill {
                out.push(K::Sill, j, 0, headers, beam(o.u0, o.u1, z));
            }
            for &(k, u) in positions.iter().filter(|(k, _)| *k != END_STUD) {
                if u < o.u0 + s - 1e-9 || u > o.u1 - s + 1e-9 || elsewhere(u) {
                    continue;
                }
                let (Ok(above), Ok(below)) = (u16::try_from(2 * u32::from(k)), u16::try_from(2 * u32::from(k) + 1))
                else {
                    continue;
                };
                if top - (head + hd) >= MIN_PIECE {
                    out.push(K::Cripple, j, above, studs, stud(u, head + hd, top));
                }
                if let Some(z) = sill
                    && z - hd - bottom >= MIN_PIECE
                {
                    out.push(K::Cripple, j, below, studs, stud(u, bottom, z - hd));
                }
            }
        }
        if self.blocking {
            full.sort_by(|a, b| a.0.total_cmp(&b.0));
            // Centered at mid-height between the tracks.
            let z = (bottom + top) / 2.0 + t / 2.0;
            for pair in full.windows(2) {
                let ((a, key), (b, _)) = (pair[0], pair[1]);
                let (a0, b0) = (a + s, b - s);
                let across = openings.iter().any(|o| o.u0 < b0 && o.u1 > a0);
                if let Some((group, index)) = key
                    && b0 - a0 >= MIN_PIECE
                    && !across
                {
                    out.push(K::Blocking, group, index, tracks, beam(a0, b0, z));
                }
            }
        }
        out.0
    }
}
```

Notes for the implementer:
- `crate::Hosted` and `crate::Element` are re-exported at the crate root (doors and windows are `ElementKind::Door(Hosted)`). If `Hosted` is not, use `crate::element::Hosted` after checking `rg -n "pub use element" crates/cc_core/src/lib.rs`.
- `Project::wall_length` exists (`project.rs`). `DVec2::to_angle` and `try_normalize` are glam methods.
- The test numbers follow from the layout with `w = 0.04`, `t = 0.04` and header depth `0.14`. If a count is off, print `layout(&site)` and check the rules above before changing a number. Each number is derived in the plan's decisions (studs taken where their body overlaps `[u0 − 2w, u1 + 2w]`, cripples at positions within `[u0 + s, u1 − s]`).

- [ ] **Step 4: Wire the variant** as listed under *Files*: element, transform, types, repair, schedule, the other crates' arms and the four i18n keys from the table. `cargo check --workspace --tests` lists every exhaustive match.

- [ ] **Step 5: Run the tests.** Run `cargo nextest run -p cc_core wall_framing`. Expected: pass.

- [ ] **Step 6: Clippy, fmt, commit.** Run `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all`, then:

```bash
git add -A
git commit -s -m "Add the wall framing element and its layout"
```

---

### Task 2: Wall framings kept in step with their walls (cc_core)

**Files:**
- Modify:
  - `crates/cc_core/src/member_host.rs`: the trait gains `inputs` and `member_level`, the sync gets the input trigger and `member_level`, the dispatch gets the `WallFraming` arms, and `Project::member_host_of` is added.
  - `crates/cc_core/src/wall_framing.rs`: `FramingRole::{key, from_key}`, `impl MemberHost for WallFraming`, and `Project::{framings_of, wall_framing_of, repair_wall_framings}`. The imports become `use crate::member_host::{MemberHost, RoleKey, fix_type, reset, within};` plus `LevelId`. Tests are added.
  - `crates/cc_core/src/document.rs` `EditCtx::remove_element`: `pending.extend(self.project.framings_of(id));` next to `host_members_of`.
  - `crates/cc_core/src/transform.rs`: the framing goes along in `copy_set`, is accepted in `accepts`, and gets its wall remapped in `paste_elements`.
  - `crates/cc_core/src/repair.rs`: the `host_ok` arm and the `repair_wall_framings` call.
- Test: `crates/cc_core/src/wall_framing.rs` (`mod tests`), plus the unchanged `member_host`, `steel_building` and `truss` tests.

**Interfaces:**
- Consumes: Task 1's `wall_framing::*` and `tests::{site_with, add_opening, framing, element}`; `EditCtx::{transform_elements, copy_elements, remove_element, element_mut, type_mut}`; `Document::{edit, undo}`; `Project::{repair, host_members_of, hosted_by}`.
- Produces:
  - `MemberHost::inputs(&self, project: &Project) -> Vec<ElementId>` (default empty);
  - `MemberHost::member_level(&self, project: &Project) -> Option<LevelId>` (default `None`);
  - `Project::member_host_of(&self, member: ElementId) -> Option<ElementId>`;
  - `Project::framings_of(&self, wall: ElementId) -> impl Iterator<Item = ElementId> + '_`;
  - `Project::wall_framing_of(&self, wall: ElementId) -> Option<ElementId>`;
  - `Project::repair_wall_framings(&mut self) -> usize` (`pub(crate)`);
  - `FramingRole::key(self) -> RoleKey`, `FramingRole::from_key(RoleKey) -> Option<FramingRole>`;
  - `ElementKind::member_host()` returns the framing.

- [ ] **Step 1: Write the failing tests** at the end of `wall_framing::tests`:

```rust
    fn member(f: &WallFraming, kind: FramingRoleKind, opening: u16, index: u16) -> ElementId {
        let role = FramingRole { kind, opening, index };
        f.members.iter().find(|m| m.role == role).map(|m| m.id).unwrap_or_else(|| panic!("no {role:?}"))
    }

    fn set(doc: &mut Document, host: ElementId, f: impl FnOnce(&mut WallFraming)) {
        doc.edit("set", |e| {
            if let Some(ElementKind::WallFraming(x)) = e.element_mut(host).map(|x| &mut x.kind) {
                f(x);
            }
        });
    }

    fn of_kind(f: &WallFraming, kind: FramingRoleKind) -> usize {
        f.members.iter().filter(|m| m.role.kind == kind).count()
    }

    fn position(doc: &Document, id: ElementId) -> DVec2 {
        match doc.project.element(id).map(|e| &e.kind) {
            Some(ElementKind::Column(c)) => c.position,
            _ => panic!("not a column"),
        }
    }

    #[test]
    fn role_keys_round_trip_in_order() {
        let roles = [
            FramingRole { kind: FramingRoleKind::BottomTrack, opening: 0, index: 0 },
            FramingRole { kind: FramingRoleKind::Stud, opening: 0, index: END_STUD },
            FramingRole { kind: FramingRoleKind::KingStud, opening: 2, index: 1 },
            FramingRole { kind: FramingRoleKind::Blocking, opening: 3, index: 0 },
        ];
        for w in roles.windows(2) {
            assert!(w[0] < w[1] && w[0].key() < w[1].key());
        }
        for r in roles {
            assert_eq!(FramingRole::from_key(r.key()), Some(r));
        }
        assert_eq!(FramingRole::from_key(RoleKey::new(9, 0, 0)), None);
    }

    #[test]
    fn a_new_framing_gets_its_members() {
        let site = site_with(3.6, |_| {});
        let f = framing(&site.doc, site.host);
        assert_eq!(f.members.len(), 12);
        let level = site.doc.project.element(site.wall).map(|e| e.level);
        for m in &f.members {
            let e = site.doc.project.element(m.id).expect("the member exists");
            assert!(m.role.fits(&e.kind));
            assert_eq!(Some(e.level), level);
        }
        assert_eq!(site.doc.project.member_host_of(f.members[0].id), Some(site.host));
        assert_eq!(site.doc.project.wall_framing_of(site.wall), Some(site.host));
        assert_eq!(site.doc.project.clone().repair(), 0, "a consistent model");
    }

    #[test]
    fn moving_the_wall_moves_the_studs_and_keeps_their_ids() {
        let mut site = site_with(3.6, |_| {});
        let before = framing(&site.doc, site.host);
        let stud = member(&before, FramingRoleKind::Stud, 0, 1);
        let wall = site.wall;
        site.doc.edit("move", |e| e.transform_elements(&[wall], &PlanTransform::translate(DVec2::new(0.0, 2.0))));
        let after = framing(&site.doc, site.host);
        assert_eq!(member(&after, FramingRoleKind::Stud, 0, 1), stud, "same id");
        assert!(position(&site.doc, stud).distance(DVec2::new(0.4, 2.0)) < 1e-9);
        assert!(after.detached.is_empty(), "moved by its wall, not detached");
        assert!(site.doc.undo());
        assert_eq!(framing(&site.doc, site.host), before);
    }

    #[test]
    fn a_window_regenerates_the_framing_and_undo_restores_it() {
        let mut site = site_with(3.6, |_| {});
        let before = framing(&site.doc, site.host);
        let start = site.doc.project.elements.clone();
        let kept = member(&before, FramingRoleKind::Stud, 0, 1);
        let dropped = member(&before, FramingRoleKind::Stud, 0, 4);
        let window = site.window;
        add_opening(&mut site, window, 1.8);
        let f = framing(&site.doc, site.host);
        assert_eq!(
            [FramingRoleKind::Stud, FramingRoleKind::KingStud, FramingRoleKind::JackStud]
                .map(|k| of_kind(&f, k)),
            [6, 2, 2]
        );
        assert_eq!(
            [FramingRoleKind::Header, FramingRoleKind::Sill, FramingRoleKind::Cripple].map(|k| of_kind(&f, k)),
            [1, 1, 4]
        );
        assert_eq!(member(&f, FramingRoleKind::Stud, 0, 1), kept);
        assert!(site.doc.project.element(dropped).is_none(), "the stud inside the opening is gone");
        assert!(site.doc.undo());
        assert_eq!(site.doc.project.elements, start);
    }

    #[test]
    fn moving_one_opening_keeps_the_keys_of_the_other() {
        let mut site = site_with(6.0, |_| {});
        let window = site.window;
        add_opening(&mut site, window, 1.2);
        let second = add_opening(&mut site, window, 4.0);
        let f = framing(&site.doc, site.host);
        let (first_header, second_header) =
            (member(&f, FramingRoleKind::Header, 0, 0), member(&f, FramingRoleKind::Header, 1, 0));
        site.doc.edit("slide", |e| {
            if let Some(h) = e.element_mut(second).and_then(|x| x.as_hosted_mut()) {
                h.offset = 4.4;
            }
        });
        let f = framing(&site.doc, site.host);
        assert_eq!(member(&f, FramingRoleKind::Header, 0, 0), first_header);
        assert_eq!(member(&f, FramingRoleKind::Header, 1, 0), second_header);
        let Some(ElementKind::Beam(b)) = site.doc.project.element(second_header).map(|e| &e.kind) else {
            panic!("a beam")
        };
        assert!((b.start.x - 3.76).abs() < 1e-9, "moved with its window");
    }

    #[test]
    fn deleting_the_window_brings_the_studs_back() {
        let mut site = site_with(3.6, |_| {});
        let window = site.window;
        let w = add_opening(&mut site, window, 1.8);
        site.doc.edit("delete", |e| e.remove_element(w));
        let f = framing(&site.doc, site.host);
        assert_eq!((of_kind(&f, FramingRoleKind::Stud), of_kind(&f, FramingRoleKind::KingStud)), (10, 0));
    }

    #[test]
    fn a_window_type_edit_regenerates_the_header() {
        let mut site = site_with(3.6, |_| {});
        let window = site.window;
        add_opening(&mut site, window, 1.8);
        let header = member(&framing(&site.doc, site.host), FramingRoleKind::Header, 0, 0);
        site.doc.edit("wider", |e| {
            if let Some(TypeData::Window { width, .. }) = e.type_mut(window).map(|t| &mut t.data) {
                *width = 1.6;
            }
        });
        let Some(ElementKind::Beam(b)) = site.doc.project.element(header).map(|e| &e.kind) else { panic!("a beam") };
        assert!((b.start.x - 0.96).abs() < 1e-9, "1.0 - 0.04");
    }

    #[test]
    fn deleting_the_wall_deletes_the_framing_and_its_members() {
        let mut site = site_with(3.6, |_| {});
        let ids = site.doc.project.host_members_of(site.host);
        assert_eq!(ids.len(), 12);
        let wall = site.wall;
        site.doc.edit("delete", |e| e.remove_element(wall));
        assert!(site.doc.project.element(site.host).is_none());
        assert!(ids.iter().all(|id| site.doc.project.element(*id).is_none()));
        assert!(site.doc.undo());
        assert!(ids.iter().all(|id| site.doc.project.element(*id).is_some()));
        assert!(site.doc.project.element(site.host).is_some());
    }

    #[test]
    fn a_detached_stud_is_left_alone() {
        let mut site = site_with(3.6, |_| {});
        let stud = member(&framing(&site.doc, site.host), FramingRoleKind::Stud, 0, 2);
        site.doc.edit("move", |e| e.transform_elements(&[stud], &PlanTransform::translate(DVec2::new(0.0, 1.0))));
        let role = FramingRole { kind: FramingRoleKind::Stud, opening: 0, index: 2 };
        assert!(framing(&site.doc, site.host).detached.contains(&role));
        set(&mut site.doc, site.host, |f| f.spacing = 0.4 + 1e-6);
        assert!(position(&site.doc, stud).distance(DVec2::new(0.8, 1.0)) < 1e-9, "where the user left it");
        assert_eq!(framing(&site.doc, site.host).members.len(), 11, "and not made again");
    }

    #[test]
    fn the_members_follow_the_wall_to_another_level() {
        let mut site = site_with(3.6, |_| {});
        let upper = site.doc.project.levels[1].id;
        let wall = site.wall;
        site.doc.edit("level", |e| {
            if let Some(w) = e.element_mut(wall) {
                w.level = upper;
            }
        });
        let ids = site.doc.project.host_members_of(site.host);
        assert!(ids.iter().all(|id| site.doc.project.element(*id).is_some_and(|e| e.level == upper)));
    }

    #[test]
    fn a_copied_wall_takes_its_framing_along_and_a_framing_alone_is_not_copied() {
        let mut site = site_with(3.6, |_| {});
        let (wall, host) = (site.wall, site.host);
        let originals = site.doc.project.host_members_of(host);
        let copies = site.doc.edit("copy", |e| e.copy_elements(&[wall], &[PlanTransform::translate(DVec2::new(0.0, 5.0))]));
        let new_wall = copies.iter().copied().find(|id| site.doc.project.element(*id).is_some_and(|e| e.as_wall().is_some()));
        let new_wall = new_wall.expect("the wall's copy");
        let copy = site.doc.project.wall_framing_of(new_wall).expect("its framing's copy");
        assert_ne!(copy, host);
        let f = framing(&site.doc, copy);
        assert_eq!(f.members.len(), 12);
        assert!(f.members.iter().all(|m| !originals.contains(&m.id)));
        assert_eq!(framing(&site.doc, host).members.len(), 12, "the original keeps its own");
        let alone = site.doc.edit("copy", |e| e.copy_elements(&[host], &[PlanTransform::translate(DVec2::new(0.0, 9.0))]));
        assert!(alone.is_empty(), "its wall has one already");
    }

    #[test]
    fn repair_keeps_one_framing_per_wall_and_drops_those_without_a_wall() {
        let site = site_with(3.6, |_| {});
        let mut p = site.doc.project.clone();
        let mut twin = p.element(site.host).cloned().expect("the framing");
        twin.id = ElementId(p.alloc_id());
        if let ElementKind::WallFraming(f) = &mut twin.kind {
            (f.members, f.detached) = (Vec::new(), Vec::new());
        }
        p.elements.insert(twin.id, twin.clone());
        assert_eq!(p.repair(), 1, "the later twin goes");
        assert!(p.element(twin.id).is_none() && p.element(site.host).is_some());
        assert_eq!(p.repair(), 0);
        let mut p = site.doc.project.clone();
        p.elements.remove(&site.wall);
        assert_eq!(p.repair(), 1, "the framing of a missing wall goes (its members stay as plain elements)");
        assert!(p.element(site.host).is_none());
        assert_eq!(p.repair(), 0);
    }
```

Add `use crate::PlanTransform;` and `use crate::member_host::RoleKey;` to the test module's imports if `super::*` does not bring them in. `Project::alloc_id` exists (`project.rs`).

- [ ] **Step 2: Run them to see them fail.** Run `cargo nextest run -p cc_core wall_framing`. Expected: compile errors (`key`, `from_key`, `member_host_of`, `wall_framing_of` are missing), then failures (no members are made).

- [ ] **Step 3: Extend the trait** in `crates/cc_core/src/member_host.rs`. Add `LevelId` to the `use crate::{..}` line, and add at the end of `trait MemberHost`:

```rust
    /// Other elements whose change regenerates the host (a wall framing's
    /// wall and the wall's doors and windows). An edit regenerates the host
    /// when it changes, adds or removes one of them, edits one's type, or
    /// touches an element one of them hosted before the edit (a removed or
    /// re-hosted door or window). None by default.
    fn inputs(&self, _project: &Project) -> Vec<ElementId> {
        Vec::new()
    }

    /// The level its members go on when it is not the host's own (a wall
    /// framing's members are on its wall's level).
    fn member_level(&self, _project: &Project) -> Option<LevelId> {
        None
    }
```

Add the dispatch arms `ElementKind::WallFraming(f) => Some(f.as_ref() as &dyn MemberHost),` (and `f.as_mut() as &mut dyn MemberHost` in `member_host_mut`). Add to the first `impl Project` block:

```rust
    /// The host that manages `member`, if any.
    pub fn member_host_of(&self, member: ElementId) -> Option<ElementId> {
        self.elements
            .values()
            .find(|e| e.member_host().is_some_and(|h| h.member_list().iter().any(|(_, id)| *id == member)))
            .map(|e| e.id)
    }
```

- [ ] **Step 4: The input trigger** in `sync_member_hosts`. Extend the doc comment's point 2 with ", or touched one of its inputs (see [`MemberHost::inputs`])". Then:
  - before the `for (id, before) in self.element_changes()` loop, add `let mut touched: Vec<(ElementId, Option<ElementId>)> = Vec::new();`;
  - as the loop's first line, add `touched.push((id, before.and_then(|b| b.as_hosted()).map(|h| h.host)));`;
  - in the host loop, replace the `host_changed` line with:

```rust
            let inputs = h.inputs(p);
            // A changed wall or opening, an edited opening type, or a door or window that left the wall.
            let input_changed = !inputs.is_empty()
                && (touched.iter().any(|(id, was)| inputs.contains(id) || was.is_some_and(|w| inputs.contains(&w)))
                    || inputs.iter().filter_map(|i| p.element(*i)).any(|x| types.contains(&x.type_id)));
            let host_changed =
                changed.contains(&e.id) || input_changed || h.member_types().iter().any(|t| types.contains(t));
```

  In `regenerate_members`, replace

```rust
        let (level, phase_created, phase_demolished, design_option) =
            (e.level, e.phase_created, e.phase_demolished, e.design_option);
```

  with

```rust
        let level = h.member_level(p).unwrap_or(e.level);
        let (phase_created, phase_demolished, design_option) = (e.phase_created, e.phase_demolished, e.design_option);
```

  (keep whatever else the committed function does with these values).

- [ ] **Step 5: The framing as a host** in `wall_framing.rs`. Add to `impl FramingRole`:

```rust
    /// Its key for the shared member-host code (same order as the role).
    pub fn key(self) -> RoleKey {
        RoleKey::new(self.kind as u8, self.opening, self.index)
    }

    pub fn from_key(key: RoleKey) -> Option<Self> {
        let (kind, opening, index) = key.parts();
        Some(Self { kind: *FramingRoleKind::ALL.get(usize::from(kind))?, opening, index })
    }
```

and:

```rust
impl MemberHost for WallFraming {
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
        self.members =
            members.iter().filter_map(|&(k, id)| Some(FramingMember { role: FramingRole::from_key(k)?, id })).collect();
    }

    fn detached_roles(&self) -> Vec<RoleKey> {
        self.detached.iter().map(|r| r.key()).collect()
    }

    fn set_detached_roles(&mut self, roles: &[RoleKey]) {
        self.detached = roles.iter().filter_map(|k| FramingRole::from_key(*k)).collect();
    }

    fn role_fits(&self, role: RoleKey, kind: &ElementKind) -> bool {
        FramingRole::from_key(role).is_some_and(|r| r.fits(kind))
    }

    fn check_parameters(&self, project: &Project) -> Result<(), String> {
        self.check()?;
        self.check_types(project)?;
        self.check_wall(project)
    }

    fn repair_parameters(&mut self, project: &Project) -> Option<usize> {
        self.check_wall(project).ok()?;
        let mut n = self.reset_out_of_range();
        n += fix_type(project, &mut self.stud_type, Category::Column)?;
        n += fix_type(project, &mut self.track_type, Category::Beam)?;
        n += fix_type(project, &mut self.header_type, Category::Beam)?;
        Some(n)
    }

    fn inputs(&self, project: &Project) -> Vec<ElementId> {
        let mut ids = vec![self.wall];
        ids.extend(project.hosted_by(self.wall).map(|e| e.id));
        ids
    }

    fn member_level(&self, project: &Project) -> Option<LevelId> {
        project.element(self.wall).map(|e| e.level)
    }
}

impl Project {
    /// The framings of `wall` (one in a consistent model).
    pub fn framings_of(&self, wall: ElementId) -> impl Iterator<Item = ElementId> + '_ {
        self.elements
            .values()
            .filter(move |e| matches!(&e.kind, ElementKind::WallFraming(f) if f.wall == wall))
            .map(|e| e.id)
    }

    /// The framing of `wall`, if it has one.
    pub fn wall_framing_of(&self, wall: ElementId) -> Option<ElementId> {
        self.framings_of(wall).next()
    }

    /// One framing per wall: the later ones (higher ids) go, their members
    /// staying as plain elements. Returns how many went.
    pub(crate) fn repair_wall_framings(&mut self) -> usize {
        let mut seen = std::collections::BTreeSet::new();
        let extra: Vec<ElementId> = self
            .elements
            .values()
            .filter_map(|e| match &e.kind {
                ElementKind::WallFraming(f) => (!seen.insert(f.wall)).then_some(e.id),
                _ => None,
            })
            .collect();
        for id in &extra {
            self.elements.remove(id);
        }
        extra.len()
    }
}
```

- [ ] **Step 6: Removal, copies, repair.**
  - In `document.rs` `remove_element`, after `pending.extend(self.project.host_members_of(id));`, add `// A wall's framing goes with it (and the framing's members with the framing).` and `pending.extend(self.project.framings_of(id));`.
  - In `transform.rs` `copy_set`, before the "member host makes its own members" lines:

```rust
        // A copied wall takes its framing along.
        out.extend(
            self.elements
                .values()
                .filter(|e| !set.contains(&e.id))
                .filter(|e| matches!(&e.kind, ElementKind::WallFraming(f) if set.contains(&f.wall)))
                .cloned(),
        );
```

    Then make the managed set cover every host in `out` (the framing just added included):
    `let managed: std::collections::BTreeSet<ElementId> = out.iter().flat_map(|e| self.host_members_of(e.id)).collect();`.
  - In `accepts`, add the arm:

```rust
            // Only with its wall: the wall it came from has a framing already.
            ElementKind::WallFraming(f) => set.iter().any(|w| w.id == f.wall && w.as_wall().is_some()),
```

  - In `paste_elements`, after the copied-valves loop:

```rust
            // Copied framings frame the copies of their walls.
            for id in &created[first..] {
                let Some(ElementKind::WallFraming(f)) = self.element_mut(*id).map(|e| &mut e.kind) else { continue };
                if let Some(n) = remap.get(&f.wall) {
                    f.wall = *n;
                }
            }
```

  - In `repair.rs`, in the `host_ok` match:

```rust
                        ElementKind::WallFraming(f) => self.element(f.wall).is_some_and(|w| w.as_wall().is_some()),
```

    Right before `repairs += self.repair_member_hosts();`, add `// One framing per wall.` and `repairs += self.repair_wall_framings();`.

- [ ] **Step 7: Run the tests.** Run `cargo nextest run -p cc_core wall_framing member_host steel_building truss`. Expected: pass, with the steel and truss tests unchanged. Known checks:
  - the type-edit test needs `changed_types` to see a `type_mut` edit (the truss Task 3 note). If it does not, check that `type_mut` touches the type;
  - in the copy test, `copy_elements` returns the new ids. If the framing's copy is missing, check `accepts` (the wall must be in `set`, which `copy_set` returns with the framing);
  - if the missing-wall repair counts more than 1, print `p.repair()`'s pieces. Something else may reference the wall; adjust the expected number only after confirming why.

- [ ] **Step 8: Clippy, fmt, commit.** Run `cargo clippy -p cc_core --all-targets -- -D warnings` and `cargo fmt --all`, then:

```bash
git add -A
git commit -s -m "Keep wall framings in step with their walls and openings"
```

---

### Task 3: `wall_framing_create`, `element_get`/`element_set` (cc_engine)

**Files:**
- Create:
  - `crates/cc_engine/src/commands/wall_framing.rs`;
  - `crates/cc_engine/tests/wall_framing.rs`, with `mod wall_framing;` in `crates/cc_engine/tests/main.rs` between `mod wall_attach;` and `mod wall_location_joins;`.
- Modify:
  - `crates/cc_engine/src/commands/mod.rs`: `pub mod wall_framing;` (alphabetical, after `pub mod views;`).
  - `crates/cc_engine/src/registry.rs`: `wall_framing::WallFramingCreate,` after `steel_building::SteelBuildingCreate,`.
  - `crates/cc_engine/src/commands/query.rs`: the arm `ElementKind::WallFraming(f) => super::wall_framing::derived(p, f, &mut derived),` next to the `SteelBuilding` one.
  - `crates/cc_engine/src/commands/edit.rs` (`patch_kind`'s `match &kind`): the arm that refuses a new `wall` (Step 4).
  - `assets/i18n/*.ftl` (12): `tx-wall-framing`.
  - `docs/commands.md`: regenerated.

**Interfaces:**
- Consumes:
  - `cc_core::wall_framing::*` (Tasks 1–2);
  - `structure_material::type_or_row(s, Option<ElementTypeId>, Category, (&str, &str)) -> Result<ElementTypeId>` (truss Task 4);
  - `refs::{element, type_of, Ref}`, `super::{add_element, edit}`;
  - `Session::{project, doc, undo_to}`, `Document::{undo_depth, squash_since}`, `Project::{host_members_of, wall_framing_of, default_type}`.
- Produces:
  - `pub struct WallFramingCreate { walls: Vec<u64>, system, spacing, stud_type, track_type, header_type, double_top, blocking }` (all `Option` except `walls`), with `NAME = "wall_framing_create"`;
  - `pub struct FramingRows { pub stud, pub track, pub header: (&'static str, &'static str) }` and `pub fn framing_rows(FramingSystem) -> FramingRows`;
  - `pub fn new_wall_framing_type(ElementTypeId) -> ElementType`;
  - `pub(crate) fn derived(&Project, &WallFraming, &mut Map)`.
  - `element_get` `derived` holds:
    - `members { studs, king_studs, jack_studs, cripples, bottom_tracks, top_tracks, headers, sills, blocking }`;
    - `detached`, `length` and `at_limit`.

- [ ] **Step 1: Write the failing engine tests** in `crates/cc_engine/tests/wall_framing.rs`:

```rust
//! Light steel and wood framing of walls: `wall_framing_create` makes one
//! host per wall, which keeps its studs, tracks, headers and sills in step
//! with its parameters, its wall and the wall's doors and windows.

use cc_core::wall_framing::{FramingRoleKind as K, WallFraming};
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

fn wall(s: &mut Session, start: [f64; 2], end: [f64; 2]) -> u64 {
    id(&run(s, "wall_create", json!({"start": start, "end": end, "height": 3.0})))
}

fn framing(s: &Session, host: u64) -> WallFraming {
    match s.project().element(ElementId(host)).map(|e| &e.kind) {
        Some(ElementKind::WallFraming(f)) => (**f).clone(),
        _ => panic!("not a wall framing"),
    }
}

fn count(f: &WallFraming, kind: K) -> usize {
    f.members.iter().filter(|m| m.role.kind == kind).count()
}

fn framings(s: &Session) -> usize {
    s.project().elements.values().filter(|e| matches!(e.kind, ElementKind::WallFraming(_))).count()
}

fn first_framing(out: &Value) -> u64 {
    id(&out["framings"][0])
}

#[test]
fn light_steel_defaults_make_the_catalog_types_once() {
    let mut s = Session::default();
    let a = wall(&mut s, [0.0, 0.0], [3.6, 0.0]);
    let b = wall(&mut s, [0.0, 5.0], [3.6, 5.0]);
    let depth = s.doc.undo_depth();
    let out = run(&mut s, "wall_framing_create", json!({"walls": [a, b]}));
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    let list = out["framings"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["wall"], json!(a));
    assert_eq!(list[0]["members"].as_array().unwrap().len(), 12, "10 studs and two tracks");
    let f = framing(&s, first_framing(&out));
    assert_eq!((count(&f, K::Stud), count(&f, K::TopTrack)), (10, 1));
    assert!((f.spacing - 0.4).abs() < 1e-9 && !f.double_top && !f.blocking);
    assert_eq!(types_named(&s, "Ue 90x40x12x0,95"), 1, "the studs' column type");
    assert_eq!(types_named(&s, "U 92x38x0,95"), 1, "the tracks' beam type");
    assert_eq!(types_named(&s, "Ue 140x40x12x0,95"), 1, "the headers' beam type");
    run(&mut s, "undo", json!({}));
    assert_eq!(framings(&s), 0);
    assert_eq!(types_named(&s, "Ue 90x40x12x0,95"), 0, "undone with the framings");
}

#[test]
fn wood_defaults_double_the_top_plate() {
    let mut s = Session::default();
    let a = wall(&mut s, [0.0, 0.0], [3.6, 0.0]);
    let out = run(&mut s, "wall_framing_create", json!({"walls": [a], "system": "wood", "blocking": true}));
    let f = framing(&s, first_framing(&out));
    assert!((f.spacing - 0.6).abs() < 1e-9 && f.double_top);
    assert_eq!((count(&f, K::TopTrack), count(&f, K::Stud), count(&f, K::Blocking)), (2, 7, 6));
    assert_eq!(types_named(&s, "Peça 6x8"), 2, "the studs' column type and the plates' beam type");
    assert_eq!(types_named(&s, "Peça 6x16"), 1, "the headers' beam type");
}

#[test]
fn studs_turn_their_depth_across_the_wall() {
    let mut s = Session::default();
    let a = wall(&mut s, [0.0, 0.0], [0.0, 3.6]);
    let f = framing(&s, first_framing(&run(&mut s, "wall_framing_create", json!({"walls": [a]}))));
    let stud = f.members.iter().find(|m| m.role.kind == K::Stud).unwrap().id;
    let e = s.project().element(stud).unwrap();
    let ElementKind::Column(c) = &e.kind else { panic!("a column") };
    assert!((c.rotation_deg - 90.0).abs() < 1e-9, "turned with the wall");
    // The section's width (local x) runs along the wall, its depth (local y) across it.
    let (width, depth) = cc_core::steel_building::section_box(s.project(), e.type_id);
    assert!(width < depth, "{width} x {depth}");
    assert!(c.position.x.abs() < 1e-9 && (c.position.y - width / 2.0).abs() < 1e-9, "flush with the wall start");
}

#[test]
fn refused_before_anything_is_made() {
    let mut s = Session::default();
    let a = wall(&mut s, [0.0, 0.0], [3.6, 0.0]);
    let curved = id(&run(&mut s, "wall_create", json!({"start": [0, 5], "end": [4, 5], "bulge": 0.3, "height": 3.0})));
    let depth = s.doc.undo_depth();
    for args in [
        json!({"walls": []}),
        json!({"walls": [curved]}),
        json!({"walls": [a, a]}),
        json!({"walls": [a], "spacing": 0.1}),
        json!({"walls": [a], "spacing": 1.3}),
        json!({"walls": [a], "system": "steel"}),
        json!({"walls": [a], "stud_type": "no such type"}),
        json!({"walls": [999_999]}),
    ] {
        assert!(s.execute("wall_framing_create", args.clone()).is_err(), "{args}");
    }
    assert_eq!(s.doc.undo_depth(), depth);
    assert_eq!(framings(&s), 0);
    assert_eq!(types_named(&s, "Ue 90x40x12x0,95"), 0);
    run(&mut s, "wall_framing_create", json!({"walls": [a]}));
    let depth = s.doc.undo_depth();
    assert!(s.execute("wall_framing_create", json!({"walls": [a]})).is_err(), "one framing per wall");
    assert_eq!(s.doc.undo_depth(), depth);
    assert_eq!(framings(&s), 1);
}

#[test]
fn element_set_of_spacing_regenerates_in_one_step() {
    let mut s = Session::default();
    let a = wall(&mut s, [0.0, 0.0], [3.6, 0.0]);
    let host = first_framing(&run(&mut s, "wall_framing_create", json!({"walls": [a]})));
    let before = framing(&s, host);
    let start = before.members.iter().find(|m| m.role.kind == K::Stud).unwrap().id;
    let depth = s.doc.undo_depth();
    run(&mut s, "element_set", json!({"id": host, "params": {"spacing": 0.6}}));
    assert_eq!(s.doc.undo_depth(), depth + 1);
    let after = framing(&s, host);
    assert_eq!(count(&after, K::Stud), 7);
    assert_eq!(after.members.iter().find(|m| m.role.kind == K::Stud).unwrap().id, start, "same id");
    let d = run(&mut s, "element_get", json!({"id": host}))["derived"].clone();
    assert_eq!(d["members"]["studs"].as_array().unwrap().len(), 7);
    assert_eq!(d["detached"], json!(0));
    for bad in [json!({"spacing": 2.0}), json!({"members": []}), json!({"wall": 0}), json!({"stud_type": 0})] {
        assert!(s.execute("element_set", json!({"id": host, "params": bad})).is_err(), "{bad}");
    }
    assert_eq!(s.doc.undo_depth(), depth + 1);
    run(&mut s, "undo", json!({}));
    assert_eq!(framing(&s, host), before);
}

#[test]
fn a_window_frames_its_opening_and_deleting_the_wall_removes_everything() {
    let mut s = Session::default();
    let a = wall(&mut s, [0.0, 0.0], [3.6, 0.0]);
    let host = first_framing(&run(&mut s, "wall_framing_create", json!({"walls": [a]})));
    run(&mut s, "window_create", json!({"wall": a, "offset": 1.8}));
    let f = framing(&s, host);
    assert_eq!((count(&f, K::KingStud), count(&f, K::JackStud), count(&f, K::Header)), (2, 2, 1));
    assert!(count(&f, K::Stud) < 10, "studs inside the opening went");
    let members: Vec<ElementId> = f.members.iter().map(|m| m.id).collect();
    run(&mut s, "elements_delete", json!({"ids": [a]}));
    assert_eq!(framings(&s), 0);
    assert!(members.iter().all(|m| s.project().element(*m).is_none()));
    run(&mut s, "undo", json!({}));
    assert_eq!(framings(&s), 1);
    assert!(members.iter().all(|m| s.project().element(*m).is_some()));
}

#[test]
fn ifc_export_leaves_the_framing_out() {
    let mut s = Session::default();
    let a = wall(&mut s, [0.0, 0.0], [3.6, 0.0]);
    let host = first_framing(&run(&mut s, "wall_framing_create", json!({"walls": [a]})));
    let text = cc_ifc::export(s.project(), &cc_ifc::ExportOptions::default());
    let uid = cc_ifc::guid::encode(s.project().element(ElementId(host)).unwrap().uid.0);
    assert!(!text.contains(&uid), "the framing itself is not exported");
}
```

- [ ] **Step 2: Run them to see them fail.** Run `cargo nextest run -p cc_engine --test main wall_framing`. Expected: failures (`wall_framing_create` is an unknown command).

- [ ] **Step 3: Write the command** in `crates/cc_engine/src/commands/wall_framing.rs`:

```rust
//! Light steel and wood framing of walls (see `cc_core::wall_framing`):
//! the host element and the catalog types its members get by default.

use super::structure_material::type_or_row;
use super::{add_element, edit};
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::wall_framing::{self as wf, FramingRoleKind, FramingSystem, WallFraming};
use cc_core::{Category, ElementId, ElementKind, ElementType, ElementTypeId, LevelId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The catalog rows a system's members get by default: (series, designation).
#[derive(Debug, Clone, Copy)]
pub struct FramingRows {
    /// A column type of this row.
    pub stud: (&'static str, &'static str),
    /// Beam types of these rows.
    pub track: (&'static str, &'static str),
    pub header: (&'static str, &'static str),
}

pub fn framing_rows(system: FramingSystem) -> FramingRows {
    match system {
        FramingSystem::LightSteel => FramingRows {
            stud: ("NBR6355-Ue", "Ue 90x40x12x0,95"),
            track: ("NBR6355-U", "U 92x38x0,95"),
            header: ("NBR6355-Ue", "Ue 140x40x12x0,95"),
        },
        FramingSystem::Wood => FramingRows {
            stud: ("Timber-Sawn", "Peça 6x8"),
            track: ("Timber-Sawn", "Peça 6x8"),
            header: ("Timber-Sawn", "Peça 6x16"),
        },
    }
}

/// The type a project gets with its first wall framing.
pub fn new_wall_framing_type(id: ElementTypeId) -> ElementType {
    wf::host_type(id, t!("tpl-type-wall-framing"))
}

/// Frame straight walls in light steel or wood, one framing per wall. Studs
/// stand every `spacing` from the wall start (plus one flush with each end)
/// between a bottom track or plate and one top track (two plates for wood).
/// Each door or window gets king and jack studs, a header, a sill (windows)
/// and cripples. Blocking at mid-height is optional. The studs are columns
/// and the rest are beams, all ordinary elements that the framing keeps in
/// step with its parameters, its wall and the wall's doors and windows
/// (`element_set` on the framing regenerates them, keeping their ids). A
/// member edited or deleted by hand is detached and left alone, and
/// deleting the wall deletes its framing and members. Default types come
/// from the profile catalog, made when missing:
/// - light steel: `Ue 90x40x12x0,95` studs, `U 92x38x0,95` tracks and `Ue 140x40x12x0,95` headers;
/// - wood: `Peça 6x8` studs and plates, and `Peça 6x16` headers.
///
/// Curved walls, walls already framed and out-of-range values are refused
/// before anything is made. One undo step; returns `framings`, each with its
/// `id`, `wall` and `members`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct WallFramingCreate {
    /// Ids of the straight walls to frame (each gets its own framing).
    pub walls: Vec<u64>,
    /// `light_steel` (default) or `wood`.
    #[serde(default)]
    pub system: Option<String>,
    /// Stud spacing on center from the wall start, meters, 0.2 to 1.2 (default 0.4 for light steel, 0.6 for wood).
    #[serde(default)]
    pub spacing: Option<f64>,
    /// Column type of the studs, id or name (default: the system's catalog stud).
    #[serde(default)]
    pub stud_type: Option<Ref>,
    /// Beam type of the tracks or plates and of the blocking, id or name (default: the system's catalog track or plate).
    #[serde(default)]
    pub track_type: Option<Ref>,
    /// Beam type of the headers and sills, id or name (default: the system's catalog header).
    #[serde(default)]
    pub header_type: Option<Ref>,
    /// An extra top plate (wood only; default on for wood).
    #[serde(default)]
    pub double_top: Option<bool>,
    /// A row of blocking at mid-height between the studs (default off).
    #[serde(default)]
    pub blocking: Option<bool>,
}

impl Command for WallFramingCreate {
    const NAME: &'static str = "wall_framing_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        // Everything that can be refused is checked before anything is made.
        let p = s.project();
        if self.walls.is_empty() {
            return Err(EngineError::invalid("walls: give at least one wall id"));
        }
        let system = match &self.system {
            Some(c) => FramingSystem::from_code(c)
                .ok_or_else(|| EngineError::invalid(format!("unknown system '{c}' (light_steel, wood)")))?,
            None => FramingSystem::LightSteel,
        };
        let mut walls: Vec<(ElementId, LevelId)> = Vec::new();
        for &raw in &self.walls {
            let id = refs::element(p, raw)?;
            let Some((e, w)) = p.element(id).and_then(|e| e.as_wall().map(|w| (e, w))) else {
                return Err(EngineError::invalid(format!("element {raw} is not a wall")));
            };
            if w.arc().is_some() {
                return Err(EngineError::invalid(format!("wall {raw} is curved: only straight walls are framed")));
            }
            if p.wall_framing_of(id).is_some() {
                return Err(EngineError::invalid(format!("wall {raw} is framed already")));
            }
            if walls.iter().any(|(x, _)| *x == id) {
                return Err(EngineError::invalid(format!("wall {raw} is listed twice")));
            }
            walls.push((id, e.level));
        }
        let placeholder = ElementTypeId(0);
        let mut template = WallFraming::new(ElementId(0), system, placeholder, placeholder, placeholder);
        if let Some(v) = self.spacing {
            template.spacing = v;
        }
        if let Some(v) = self.double_top {
            template.double_top = v;
        }
        template.blocking = self.blocking.unwrap_or(false);
        template.check().map_err(EngineError::invalid)?;
        let given = |r: Option<&Ref>, c: Category| r.map(|r| refs::type_of(p, c, Some(r))).transpose();
        let stud_given = given(self.stud_type.as_ref(), Category::Column)?;
        let track_given = given(self.track_type.as_ref(), Category::Beam)?;
        let header_given = given(self.header_type.as_ref(), Category::Beam)?;
        let host_type = p.default_type(Category::WallFraming);
        let rows = framing_rows(system);
        let depth = s.doc.undo_depth();
        let result = (|| -> Result<Value> {
            template.stud_type = type_or_row(s, stud_given, Category::Column, rows.stud)?;
            template.track_type = type_or_row(s, track_given, Category::Beam, rows.track)?;
            template.header_type = type_or_row(s, header_given, Category::Beam, rows.header)?;
            template.check_types(s.project()).map_err(EngineError::invalid)?;
            let made = edit(s, t!("tx-wall-framing"), |e| {
                let type_id = match host_type {
                    Some(t) => t,
                    None => {
                        let id = ElementTypeId(e.alloc_id());
                        e.add_type(new_wall_framing_type(id))
                    }
                };
                Ok(walls
                    .iter()
                    .map(|&(wall, level)| {
                        let kind = ElementKind::WallFraming(Box::new(WallFraming { wall, ..template.clone() }));
                        (add_element(e, level, type_id, kind), wall)
                    })
                    .collect::<Vec<_>>())
            })?;
            let p = s.project();
            let framings: Vec<Value> = made
                .iter()
                .map(|&(id, wall)| {
                    let members: Vec<u64> = p.host_members_of(id).iter().map(|m| m.0).collect();
                    json!({ "id": id.0, "wall": wall.0, "members": members })
                })
                .collect();
            Ok(json!({ "framings": framings }))
        })();
        match result {
            Ok(v) => {
                s.doc.squash_since(depth, t!("tx-wall-framing"));
                Ok(v)
            }
            Err(e) => {
                s.undo_to(depth);
                Err(e)
            }
        }
    }
}

/// `element_get`'s derived values of a framing: its member ids by role
/// kind, how many roles are detached, its wall's length and whether the
/// layout reached the member limit (`at_limit`).
pub(crate) fn derived(p: &cc_core::Project, f: &WallFraming, out: &mut serde_json::Map<String, Value>) {
    let ids = |k: FramingRoleKind| -> Vec<u64> {
        f.members.iter().filter(|m| m.role.kind == k).map(|m| m.id.0).collect()
    };
    out.insert(
        "members".into(),
        json!({
            "studs": ids(FramingRoleKind::Stud),
            "king_studs": ids(FramingRoleKind::KingStud),
            "jack_studs": ids(FramingRoleKind::JackStud),
            "cripples": ids(FramingRoleKind::Cripple),
            "bottom_tracks": ids(FramingRoleKind::BottomTrack),
            "top_tracks": ids(FramingRoleKind::TopTrack),
            "headers": ids(FramingRoleKind::Header),
            "sills": ids(FramingRoleKind::Sill),
            "blocking": ids(FramingRoleKind::Blocking),
        }),
    );
    out.insert("detached".into(), json!(f.detached.len()));
    out.insert("length".into(), json!(f.wall_length(p)));
    out.insert("at_limit".into(), json!(f.layout(p).len() >= wf::MAX_MEMBERS));
}
```

Notes:
- `refs::element(p, raw)` returns `not_found` for a missing id (the `999_999` case).
- If the borrow checker complains that `p` (borrowed from `s`) lives across `s.doc.undo_depth()`, compute `depth` first. The steel command has the same shape, so copy its order.
- `cargo check` reports unused imports (e.g. `LevelId` if inferred); prune them.

- [ ] **Step 4: Refuse a new wall in `element_set`.** In `crates/cc_engine/src/commands/edit.rs`, inside `match &kind` after the generic host check, add:

```rust
        cc_core::ElementKind::WallFraming(f) => {
            if let cc_core::ElementKind::WallFraming(old) = &element.kind
                && old.wall != f.wall
            {
                return Err(EngineError::invalid("a framing stays on its wall: delete it and frame the other wall"));
            }
        }
```

  The shared host check already refuses `members` and runs `check_parameters` (spacing, types, wall).

- [ ] **Step 5: Wire `element_get`, i18n, reference.**
  - Add the `query.rs` arm.
  - Add `tx-wall-framing` to the 12 files.
  - Run `cargo run -q -p cc_cli -- reference > docs/commands.md`.

- [ ] **Step 6: Run the tests.** Run `cargo nextest run -p cc_engine --test main wall_framing steel_building truss`, then `cargo nextest run -p cc_engine reference i18n`. Expected: pass.
  - If `studs_turn_their_depth_across_the_wall` finds `width > depth`, the catalog's Ue column outline is laid out the other way: print `section_box` for the stud type. In that case the layout must turn studs by `angle + 90°` instead, and Task 1's rotation test changes with it. Change both together and note it in the commit.
  - If the wood stud count is not 7, print the stud type's `section_box`; the count holds for any width up to 0.09.

- [ ] **Step 7: Clippy, fmt, commit.** Run `cargo clippy -p cc_engine --all-targets -- -D warnings` and `cargo fmt --all`, then:

```bash
git add -A
git commit -s -m "Add wall_framing_create with light steel and wood catalog defaults"
```

---

### Task 4: The Light Steel Frame and Wood Frame buttons, dialog and properties (cc_app)

**Files:**
- Create:
  - `crates/cc_app/src/wall_framing_ui.rs`;
  - `assets/icons/ui/wall-framing.svg`.
- Modify:
  - `crates/cc_app/src/main.rs`: `mod wall_framing_ui;` in alphabetical order (after `mod view_cube;`).
  - `crates/cc_app/src/icons.rs`:
    - `Icon::WallFraming` after `Icon::SteelBuilding` in the enum;
    - the same in the list of all icons (raise its array length by one if it has one);
    - the arm `Icon::WallFraming => include_str!("../../../assets/icons/ui/wall-framing.svg"),`.
  - `crates/cc_app/src/commands.rs`:
    - `UiCommand::LightSteelFrame` (doc `/// Opens the Wall Framing dialog for light steel.`) and `UiCommand::WoodFrame` (doc `/// Opens the Wall Framing dialog for wood.`) after `SteelBuilding`;
    - both in `all()` after `SteelBuilding`;
    - icon `LightSteelFrame | WoodFrame => Icon::WallFraming,`;
    - labels `t!("action-light-steel-frame")`/`t!("action-wood-frame")`;
    - descriptions `t!("cmd-desc-light-steel-frame")`/`t!("cmd-desc-wood-frame")`;
    - engine name `LightSteelFrame | WoodFrame => "wall_framing_create",`;
    - in `run`, `LightSteelFrame => crate::wall_framing_ui::open(state, cc_core::wall_framing::FramingSystem::LightSteel),` and `WoodFrame => crate::wall_framing_ui::open(state, cc_core::wall_framing::FramingSystem::Wood),`;
    - the category description arm becomes `Category::WallFraming => t!("cmd-desc-light-steel-frame"),`.
  - `crates/cc_app/src/ribbon.rs`:
    - `category_icon`: `Category::WallFraming => Icon::WallFraming,`;
    - `structure_tab`: in the `M::Steel` framing group, after its `stacks(..)` call, `command_button(ui, state, LightSteelFrame);`. In the `M::Timber` framing group, after its `stacks(..)` call, `command_button(ui, state, WoodFrame);`.
  - `crates/cc_app/src/state.rs` `Dialogs`: `/// *Structure → Light Steel Frame / Wood Frame*.` `pub wall_framing: Option<crate::wall_framing_ui::WallFramingDialog>,` after `steel_building`.
  - `crates/cc_app/src/dialogs.rs`: `crate::wall_framing_ui::show(ctx, state);` after `crate::steel_building_ui::show(ctx, state);`.
  - `crates/cc_app/src/properties.rs` (`element_props`):
    - the arm `ElementKind::WallFraming(f) => crate::wall_framing_ui::props(ui, state, id, f),` replacing Task 1's `{}`;
    - right after the `match &el.kind { .. }` block, before the comment "Rooms, grids and dimensions have their own identity rows", `crate::wall_framing_ui::member_row(ui, state, id);`.
  - `assets/i18n/*.ftl` (12): the T4 keys from the table, plus `msg-no-walls-to-frame`, `msg-walls-framed`, `cmd-desc-light-steel-frame` and `cmd-desc-wood-frame`.
- Test: `crates/cc_app/src/ui_tests.rs`.

**Interfaces:**
- Consumes:
  - `WallFramingCreate` (Task 3), `cc_core::wall_framing::*` (Tasks 1–2) and `Project::{member_host_of, wall_framing_of}` (Task 2);
  - from the truss plan: `member_host_ui::{finished, length, member_rows}` and `grid_frame_ui::combo`;
  - `engine_edit::element`, `properties::{key, length_drag}`, `table::{row, section, readonly_row, checkbox}`, `dialog::Dialog`;
  - `Project::types_of_material`, `AppState::active_level`, `i18n::tr`.
- Produces:
  - `pub struct WallFramingDialog { pub system, pub spacing, pub stud_type, pub track_type, pub header_type: Option<ElementTypeId>, pub double_top, pub blocking, pub error: Option<String> }`, with `WallFramingDialog::new(FramingSystem)`;
  - `wall_framing_ui::{open(state, FramingSystem), show(ctx, state), walls_to_frame(&AppState) -> Vec<ElementId>, command(&WallFramingDialog, &[ElementId]) -> WallFramingCreate}`;
  - `wall_framing_ui::{edit(state, id, field, f), props(ui, state, id, &WallFraming), member_row(ui, state, member)}`.

- [ ] **Step 1: Write the failing UI tests** in `ui_tests.rs`:

```rust
/// Light Steel Frame shows in the Steel framing group, Wood Frame in the
/// Timber one, neither for concrete.
#[test]
fn wall_framing_buttons_follow_the_structure_material() {
    use cc_core::StructuralMaterial as M;
    for (material, steel, wood) in [(M::Concrete, false, false), (M::Steel, true, false), (M::Timber, false, true)] {
        let mut state = sample_state();
        state.ribbon_tab = RibbonTab::Structure;
        state.prefs.structure_material = material;
        let harness = ribbon_harness(state, 1800.0);
        assert_eq!(harness.query_by_label(&t!("action-light-steel-frame")).is_some(), steel, "{material:?}");
        assert_eq!(harness.query_by_label(&t!("action-wood-frame")).is_some(), wood, "{material:?}");
    }
}

/// *Structure → Light Steel Frame* opens the dialog; OK frames the selected
/// wall only, in one undo step, and selects the framing.
#[test]
fn wall_framing_dialog_frames_the_selected_wall() {
    use cc_core::wall_framing::FramingSystem;
    let mut state = sample_state();
    state.ribbon_tab = RibbonTab::Structure;
    state.prefs.structure_material = cc_core::StructuralMaterial::Steel;
    let args = serde_json::json!({"start": [100, 100], "end": [103.6, 100], "height": 3.0});
    let wall = cc_core::ElementId(state.session.execute("wall_create", args).unwrap()["id"].as_u64().unwrap());
    state.selection = [wall].into_iter().collect();
    let depth = state.session.doc.undo_depth();
    let mut harness = app_harness(state);
    harness.get_by_role_and_label(Role::Button, &t!("action-light-steel-frame")).click();
    harness.run();
    let system = harness.state().dialogs.wall_framing.as_ref().map(|d| d.system);
    assert_eq!(system, Some(FramingSystem::LightSteel));
    harness.get_by_label(&t!("action-ok")).click();
    harness.run();
    let state = harness.state();
    assert!(state.dialogs.wall_framing.is_none(), "OK was accepted: {:?}", state.message);
    let framings: Vec<(cc_core::ElementId, cc_core::ElementId, usize)> = state
        .project()
        .elements
        .values()
        .filter_map(|e| match &e.kind {
            cc_core::ElementKind::WallFraming(f) => Some((e.id, f.wall, f.members.len())),
            _ => None,
        })
        .collect();
    assert_eq!(framings.len(), 1, "only the selected wall");
    assert_eq!((framings[0].1, framings[0].2), (wall, 12), "10 studs and two tracks");
    assert_eq!(state.session.doc.undo_depth(), depth + 1);
    assert!(state.selection.contains(&framings[0].0));
}

/// A framing's rows edit it through `element_set`, *Reset Members* brings
/// back a deleted stud, and a member's *Framing* row selects the framing.
#[test]
fn wall_framing_properties_and_the_member_row() {
    use cc_core::wall_framing::{FramingRoleKind, WallFraming};
    let mut state = sample_state();
    let args = serde_json::json!({"start": [100, 100], "end": [103.6, 100], "height": 3.0});
    let wall = state.session.execute("wall_create", args).unwrap()["id"].as_u64().unwrap();
    let out = state.session.execute("wall_framing_create", serde_json::json!({"walls": [wall]})).unwrap();
    let host = cc_core::ElementId(out["framings"][0]["id"].as_u64().unwrap());
    let get = |s: &AppState| match &s.project().element(host).unwrap().kind {
        cc_core::ElementKind::WallFraming(f) => (**f).clone(),
        _ => panic!("not a wall framing"),
    };
    let studs = |f: &WallFraming| f.members.iter().filter(|m| m.role.kind == FramingRoleKind::Stud).count();
    // The path the palette's rows take.
    crate::wall_framing_ui::edit(&mut state, host, "spacing", |f| f.spacing = 0.6);
    assert_eq!(studs(&get(&state)), 7);
    let stud = get(&state).members.iter().find(|m| m.role.kind == FramingRoleKind::Stud).unwrap().id;
    state.session.execute("elements_delete", serde_json::json!({"ids": [stud.0]})).unwrap();
    assert_eq!(get(&state).detached.len(), 1);
    state.selection = [host].into_iter().collect();
    let mut harness = themed_harness(state, egui::vec2(360.0, 1600.0), egui::Theme::Light, |ui, state| {
        crate::properties::show(ui, state);
    });
    harness.run();
    assert!(harness.query_by_label(&t!("prop-stud-spacing")).is_some());
    harness.get_by_label(&t!("action-reset-members")).click();
    harness.run();
    let f = get(harness.state());
    assert_eq!((studs(&f), f.detached.len()), (7, 0));
    let member = f.members[0].id;
    harness.state_mut().selection = [member].into_iter().collect();
    harness.run();
    harness.get_by_label(&t!("action-select-framing")).click();
    harness.run();
    let selection = &harness.state().selection;
    assert!(selection.len() == 1 && selection.contains(&host));
}
```

(`sample_state`, `app_harness`, `ribbon_harness`, `themed_harness` and `Role` are the helpers the steel building and truss tests use.)

- [ ] **Step 2: Run them to see them fail.** Run `cargo nextest run -p cc_app wall_framing`. Expected: compile errors (`wall_framing_ui`, `dialogs.wall_framing` and the UiCommands are missing).

- [ ] **Step 3: The icon** `assets/icons/ui/wall-framing.svg` (24 grid, white strokes 1.75, round caps and joins, soft fill at 20%): tracks top and bottom, studs, and an opening with a header, a sill and cripples:

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M3 4h18v16H3z" fill="#fff" fill-opacity=".2" stroke="none"/><path d="M3 4h18M3 20h18M4 4v16M8 4v16M20 4v16M11 4v16M17 4v16"/><path d="M11 9h6M11 15h6M14 4v5M14 15v5" stroke-width="1.25"/></svg>
```

Wire it in `icons.rs` as listed under *Files*.

- [ ] **Step 4: Write `wall_framing_ui.rs`:**

```rust
//! *Structure → Light Steel Frame / Wood Frame*: the dialog that runs
//! `wall_framing_create` on the selected walls (or the active level's), the
//! properties of a selected framing (through `element_set`, which
//! regenerates its members in the same undo step), and the *Framing* row
//! of its members.

use crate::dialog::Dialog;
use crate::grid_frame_ui::combo;
use crate::member_host_ui::{length, member_rows};
use crate::state::AppState;
use crate::t;
use crate::table::{readonly_row, row, section};
use crate::theme::PALETTE;
use cc_core::wall_framing::{self as wf, FramingSystem, WallFraming};
use cc_core::{Category, ElementId, ElementKind, ElementTypeId, StructuralMaterial};
use cc_engine::Ref;
use cc_engine::commands::wall_framing::WallFramingCreate;
use egui::{Context, RichText, Ui};

#[derive(Debug, Clone)]
pub struct WallFramingDialog {
    pub system: FramingSystem,
    pub spacing: f64,
    /// `None`: the catalog default.
    pub stud_type: Option<ElementTypeId>,
    pub track_type: Option<ElementTypeId>,
    pub header_type: Option<ElementTypeId>,
    pub double_top: bool,
    pub blocking: bool,
    pub error: Option<String>,
}

impl WallFramingDialog {
    /// The system's defaults: spacing, top plates, catalog types, no blocking.
    pub fn new(system: FramingSystem) -> Self {
        Self {
            system,
            spacing: system.default_spacing(),
            stud_type: None,
            track_type: None,
            header_type: None,
            double_top: system.default_double_top(),
            blocking: false,
            error: None,
        }
    }
}

pub fn open(state: &mut AppState, system: FramingSystem) {
    state.dialogs.wall_framing = Some(WallFramingDialog::new(system));
}

/// The material of a system's members.
fn material(system: FramingSystem) -> StructuralMaterial {
    match system {
        FramingSystem::LightSteel => StructuralMaterial::Steel,
        FramingSystem::Wood => StructuralMaterial::Timber,
    }
}

fn systems() -> Vec<(FramingSystem, String)> {
    FramingSystem::ALL.into_iter().map(|s| (s, crate::i18n::tr(s.key()))).collect()
}

/// The catalog default, then the project's types of `category` in the system's material.
fn choices(state: &AppState, category: Category, system: FramingSystem) -> Vec<(Option<ElementTypeId>, String)> {
    let mut out = vec![(None, t!("opt-catalog-default"))];
    out.extend(state.project().types_of_material(category, material(system)).map(|t| (Some(t.id), t.name.clone())));
    out
}

/// The walls OK frames: the selected walls, or every wall of the active
/// level when no wall is selected; only straight walls not framed yet.
pub fn walls_to_frame(state: &AppState) -> Vec<ElementId> {
    let p = state.project();
    let selected: Vec<ElementId> =
        state.selection.iter().copied().filter(|id| p.element(*id).is_some_and(|e| e.as_wall().is_some())).collect();
    let candidates = if selected.is_empty() {
        let Some(level) = state.active_level() else { return Vec::new() };
        p.elements.values().filter(|e| e.level == level && e.as_wall().is_some()).map(|e| e.id).collect()
    } else {
        selected
    };
    candidates
        .into_iter()
        .filter(|id| p.element(*id).and_then(|e| e.as_wall()).is_some_and(|w| w.arc().is_none()))
        .filter(|id| p.wall_framing_of(*id).is_none())
        .collect()
}

/// The command OK runs for `walls`.
pub fn command(d: &WallFramingDialog, walls: &[ElementId]) -> WallFramingCreate {
    let type_ref = |t: Option<ElementTypeId>| t.map(|t| Ref::Id(t.0));
    WallFramingCreate {
        walls: walls.iter().map(|w| w.0).collect(),
        system: Some(d.system.code().into()),
        spacing: Some(d.spacing),
        stud_type: type_ref(d.stud_type),
        track_type: type_ref(d.track_type),
        header_type: type_ref(d.header_type),
        double_top: Some(d.double_top),
        blocking: Some(d.blocking),
    }
}

pub fn show(ctx: &Context, state: &mut AppState) {
    let Some(mut d) = state.dialogs.wall_framing.clone() else { return };
    let studs = choices(state, Category::Column, d.system);
    let beams = choices(state, Category::Beam, d.system);
    let systems = systems();
    let mut ok = None;
    let response = Dialog::new("wall-framing", t!("dlg-wall-framing")).width(440.0).show(
        ctx,
        |ui| {
            let label = |ui: &mut Ui, k: String| {
                ui.label(RichText::new(k).color(PALETTE.text_muted));
            };
            egui::Grid::new("wall-framing-fields").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
                label(ui, t!("prop-framing-system"));
                let system = d.system;
                combo(ui, "wf-system", &mut d.system, &systems);
                if d.system != system {
                    // Another system: its spacing, top plates and catalog types.
                    let fresh = WallFramingDialog::new(d.system);
                    (d.spacing, d.double_top) = (fresh.spacing, fresh.double_top);
                    (d.stud_type, d.track_type, d.header_type) = (None, None, None);
                }
                ui.end_row();
                label(ui, t!("prop-stud-spacing"));
                ui.add(crate::properties::length_drag(&mut d.spacing).range(wf::SPACING));
                ui.end_row();
                label(ui, t!("label-stud-type"));
                combo(ui, "wf-stud", &mut d.stud_type, &studs);
                ui.end_row();
                label(ui, t!("label-track-type"));
                combo(ui, "wf-track", &mut d.track_type, &beams);
                ui.end_row();
                label(ui, t!("label-header-type"));
                combo(ui, "wf-header", &mut d.header_type, &beams);
                ui.end_row();
            });
            ui.add_space(4.0);
            if d.system == FramingSystem::Wood {
                ui.checkbox(&mut d.double_top, t!("prop-double-top"));
            }
            ui.checkbox(&mut d.blocking, t!("prop-blocking"));
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
        let walls = walls_to_frame(state);
        if walls.is_empty() {
            d.error = Some(t!("msg-no-walls-to-frame"));
            state.dialogs.wall_framing = Some(d);
            return;
        }
        match state.session.run(command(&d, &walls)) {
            Ok(out) => {
                let framings = out["framings"].as_array().cloned().unwrap_or_default();
                let count: usize = framings.iter().map(|f| f["members"].as_array().map_or(0, Vec::len)).sum();
                state.message = t!("msg-walls-framed", "walls" => framings.len(), "count" => count);
                state.selection = framings.iter().filter_map(|f| f["id"].as_u64()).map(ElementId).collect();
                state.dialogs.wall_framing = None;
            }
            Err(e) => {
                d.error = Some(e.message);
                state.dialogs.wall_framing = Some(d);
            }
        }
        return;
    }
    if response.clicked.is_some() || response.dismissed {
        state.dialogs.wall_framing = None;
    } else {
        state.dialogs.wall_framing = Some(d);
    }
}

/// Change framing `id` as `f` changes a copy of it (`element_set`, which
/// regenerates its members); edits under one `field` merge while dragging.
pub fn edit(state: &mut AppState, id: ElementId, field: &str, f: impl FnOnce(&mut WallFraming)) {
    crate::engine_edit::element(state, id, Some(crate::properties::key((id, field))), |e| {
        if let ElementKind::WallFraming(x) = &mut e.kind {
            f(x);
        }
    });
}

/// The rows of a selected framing.
pub fn props(ui: &mut Ui, state: &mut AppState, id: ElementId, framing: &WallFraming) {
    let mut x = framing.clone();
    let mut done = false;
    let all = |c: Category| -> Vec<(ElementTypeId, String)> {
        state.project().types_of(c).map(|t| (t.id, t.name.clone())).collect()
    };
    let (columns, beams) = (all(Category::Column), all(Category::Beam));
    section(ui, "wall-framing", t!("section-wall-framing"), |ui| {
        readonly_row(ui, &t!("prop-framing-system"), crate::i18n::tr(framing.system.key()));
        done |= length(ui, &t!("prop-stud-spacing"), &mut x.spacing, wf::SPACING);
        if x.system == FramingSystem::Wood
            && row(ui, &t!("prop-double-top"), |ui| crate::table::checkbox(ui, &mut x.double_top)).changed()
        {
            done = true;
        }
        if row(ui, &t!("prop-blocking"), |ui| crate::table::checkbox(ui, &mut x.blocking)).changed() {
            done = true;
        }
        if member_rows(ui, framing) {
            x.detached.clear();
            done = true;
        }
    });
    section(ui, "wall-framing-types", t!("section-type"), |ui| {
        let before = (x.stud_type, x.track_type, x.header_type);
        row(ui, &t!("label-stud-type"), |ui| combo(ui, &format!("wf-stud-{}", id.0), &mut x.stud_type, &columns));
        row(ui, &t!("label-track-type"), |ui| combo(ui, &format!("wf-track-{}", id.0), &mut x.track_type, &beams));
        row(ui, &t!("label-header-type"), |ui| {
            combo(ui, &format!("wf-header-{}", id.0), &mut x.header_type, &beams)
        });
        done |= (x.stud_type, x.track_type, x.header_type) != before;
    });
    if x != *framing {
        edit(state, id, "wall-framing", move |f| *f = x);
    }
    if done {
        state.session.doc.break_merge();
    }
}

/// The *Framing* row of a member of a wall framing: its button selects the framing.
pub fn member_row(ui: &mut Ui, state: &mut AppState, member: ElementId) {
    let p = state.project();
    let Some(host) = p
        .member_host_of(member)
        .filter(|h| matches!(p.element(*h).map(|e| &e.kind), Some(ElementKind::WallFraming(_))))
    else {
        return;
    };
    let mut select = false;
    section(ui, "member-framing", t!("section-wall-framing"), |ui| {
        select = row(ui, &t!("prop-wall-framing"), |ui| ui.button(t!("action-select-framing"))).clicked();
    });
    if select {
        state.selection = [host].into_iter().collect();
    }
}
```

Adapt to the real API:
- `readonly_row`'s value parameter: if it takes `&str`, pass `&crate::i18n::tr(..)`. Copy the steel building's use.
- `row` returns the closure's value (the steel building wraps `row(..)` in `finished(..)`). If it returns something else, take the button's `Response` from inside the closure into a local.
- If `crate::i18n::tr` is named differently, use what `truss_ui.rs` uses for `TrussShape::key`.
- If `member_host_ui::length` is not `pub(crate)`, make it so (the truss plan defines it `pub(crate)`).

- [ ] **Step 5: Wire the commands, ribbon, state, dialogs and properties** as listed under *Files*. Then add the i18n keys to the 12 files (T4 rows of the table and the four list keys).

- [ ] **Step 6: Run the tests.** Run `cargo check --tests -p cc_app`, then `cargo nextest run -p cc_app wall_framing steel_building truss structure_material i18n icons ribbon`. Expected: pass.
  - If the ribbon test does not find a button at 1800 px, widen the harness to 2200 px. Do not change the ribbon for the test.
  - If the properties harness does not reach *Reset Members* or the *Framing* button, collapse the type section first (click its header) or make the harness taller.
  - If a Structure-tab screenshot reference exists for steel or timber, rerun it with `UPDATE_SNAPSHOTS=1 cargo nextest run -p cc_app snapshots` and check the new image shows the button.

- [ ] **Step 7: Clippy, fmt, commit.** Run `cargo clippy -p cc_app --all-targets -- -D warnings` and `cargo fmt --all`, then:

```bash
git add -A
git commit -s -m "Light Steel Frame and Wood Frame: dialog, properties and member row"
```

---

### Task 5: Documentation and full check

**Files:**
- Modify:
  - `AGENTS.md`:
    - in the member-hosts bullet the truss branch added, mention `MemberHost::inputs` (the elements whose change, type edit or departing hosted element regenerates the host) and `member_level`, plus `Project::member_host_of`;
    - add a new **Wall framing** bullet right after it. It covers:
      - `cc_core::wall_framing`: `ElementKind::WallFraming(WallFraming)`, category `WallFraming`, and the `TypeData::WallFraming {}` type created with the first one;
      - the framing's fields: `wall`, `FramingSystem` light steel/wood, the stud (column), track (beam) and header (beam) types, `spacing` 0.2–1.2, `double_top` (wood only) and `blocking`;
      - `layout(&Project)` in the wall frame: the body centerline `u`, end studs flush, studs every `k·spacing` dropped where an opening's framing stands, king and jack studs, header, sill (windows) and cripples (`2k` above, `2k + 1` below), and blocking keyed by its left stud; `FramingRole`/`FramingRoleKind`, `END_STUD`, `MIN_PIECE`, `MAX_MEMBERS`; studs turned with the wall so their depth runs across it, beams with joins off;
      - its inputs (the wall and its doors and windows) and `member_level`;
      - removal (`framings_of` in `remove_element`), copy (with its wall only) and `repair_wall_framings` (one per wall);
      - `cc_engine::commands::wall_framing`: `wall_framing_create` (`framing_rows` through `type_or_row`/`catalog_type`, curved and framed walls refused, one undo step, `{framings: [{id, wall, members}]}`), `element_get` `derived`, and `element_set` refusing a new `wall`;
      - plans and IFC: plans draw only the members, and IFC leaves the host out;
      - the app: the *Light Steel Frame* (Steel) and *Wood Frame* (Timber) buttons, `wall_framing_ui.rs` (the dialog, palette rows with *Reset Members*, and members' *Framing* row) and `Icon::WallFraming`.
  - `docs/interface.md` and `docs/interface.pt-BR.md`: in the Structure tab paragraph, Steel's framing group has *Light Steel Frame* and Timber's has *Wood Frame*. Then add a paragraph on the dialog (system, spacing, types, double top plate, blocking; selected walls or the active level's), the framing's properties, detaching and *Reset Members*, and the members' *Framing* row.
  - `docs/automation.md` and `docs/automacao.md`: a short paragraph after the truss one, with `{"command": "wall_framing_create", "args": {"walls": [12, 13], "system": "wood", "blocking": true}}` and `{"command": "element_set", "args": {"id": 42, "params": {"spacing": 0.4}}}`, in each file's language.
  - `CHANGELOG.md` under `## [Unreleased]` → `### Added`: light steel and wood framing of walls. Members are studs, tracks or plates, king and jack studs, headers, sills, cripples and optional blocking as ordinary columns and beams, regenerated with their ids kept when the wall, its doors and windows or the framing change, and detached when edited by hand. Add `wall_framing_create`, and *Structure → Light Steel Frame* / *Wood Frame* with their dialog and properties.
- [ ] **Step 1: Write the docs** above, concise and in the style of the neighboring entries.
- [ ] **Step 2: Full check.**
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo nextest run --workspace`
  - `cargo test --workspace --doc`

  Expected: all green. `docs/commands.md` must match `cargo run -q -p cc_cli -- reference` (a test checks it).
- [ ] **Step 3: Check it in the app** (`cargo run -p cc_app -- examples/casa_simples.ccad`):
  - frame a wall with a window from *Structure → Steel → Light Steel Frame*;
  - check the studs in plan and 3D, then move the window and see the framing follow;
  - delete a stud and use *Reset Members*;
  - delete the wall and undo.

  Note anything odd in the final report rather than changing the spec's behavior.
- [ ] **Step 4: Commit.**

```bash
git add -A
git commit -s -m "Document wall framing"
```

---

## Self-review

- Spec coverage:
  - Host element, category, `TypeData::WallFraming {}`, the fields `wall`, `system`, `stud_type`, `track_type`, `header_type`, `spacing` (0.2–1.2, 0.4/0.6), `double_top` (wood, default on), `blocking` (default off), `members` and `detached`: Task 1.
  - Members as real columns and beams in the wall frame: Task 1 (`layout`).
  - Regeneration on its own parameters, its wall and the wall's openings, through `MemberHost::inputs` with an empty default: Task 2.
  - Deleting the wall deletes the framing and the members (hosted like rebar, via `remove_element`): Task 2.
  - One framing per wall: refused by the engine (Task 3) and repaired (Task 2).
  - Curved walls refused: Task 3 (layout empty in Task 1).
  - Layout:
    - bottom track not split at doors, `n` top plates and studs at 0, L and `k·spacing`: Task 1;
    - studs dropped in openings and closer than a stud width, studs from base + t to top − n·t, rotation: Task 1;
    - king, jack, header, sill (windows), cripples and clipping: Task 1;
    - blocking, `MAX_MEMBERS = 4000` and the roles and keys, stable when an unrelated opening moves: Tasks 1 and 2.
  - Defaults table through `catalog_type`: Task 3.
  - `wall_framing_create` parameters, one undo step, refusals before any edit, `{framings: [{id, wall, members}]}`: Task 3.
  - `element_get`/`element_set` (parameters, `wall` unchangeable): Task 3.
  - Views: plans draw nothing for the host and the studs show as columns; elevations and sections draw the members: no code (members are ordinary elements). Checked in Task 5 Step 3. IFC leaves the host out: Task 3 test.
  - App buttons (Steel and Timber panels), dialog, OK on the selected walls or the active level's, palette rows with *Reset Members*, the members' *Framing* row, 12 languages: Task 4.
  - Spec tests:
    - inputs (move the wall and keep the ids; a window adds kings, jacks, header, sill and cripples and drops studs; delete the wall; undo): Task 2;
    - layout (10 studs, the window's members, a door without a sill, wood double top, blocking count, curved refused): Tasks 1 and 3;
    - engine (default types for both systems, a second framing refused, `element_set` of spacing in one step): Task 3;
    - app (the dialog frames a selected wall): Task 4.
- Names used across tasks:
  - `cc_core`: `WallFraming`, `FramingSystem`, `FramingRoleKind`, `FramingRole`, `FramingMember`, `MAX_MEMBERS`, `MIN_PIECE`, `END_STUD`, `SPACING`, `LIGHT_STEEL_SPACING`, `WOOD_SPACING`, `host_type`, `layout`, `top_plates`, `check`, `check_types`, `check_wall`, `reset_out_of_range`, `type_ids`, `wall_length`, `key`, `from_key`;
  - member hosts: `MemberHost::{inputs, member_level}`, `Project::{member_host_of, framings_of, wall_framing_of, repair_wall_framings}`;
  - `cc_engine`: `WallFramingCreate`, `FramingRows`, `framing_rows`, `new_wall_framing_type`, `wall_framing::derived`;
  - `cc_app`: `WallFramingDialog`, `wall_framing_ui::{open, show, walls_to_frame, command, edit, props, member_row}`, `UiCommand::{LightSteelFrame, WoodFrame}`, `Icon::WallFraming`.

## Decisions where the spec was open or inconsistent

1. `inputs` takes the project (`fn inputs(&self, project: &Project) -> Vec<ElementId>`). An edit regenerates a host when it touches an input, an input's type, or an element that an input hosted before the edit (so a deleted or re-hosted window still regenerates its old wall's framing).
2. `MemberHost` also gains `member_level` (default `None`): a framing's members go on its wall's level, even after the wall moves to another level.
3. Jack studs stand at the opening's edges and king studs one stud width outside them; the header spans `u0 − w … u1 + w` and the sill `u0 … u1`. The spec's coordinates put the jacks inside the opening, which would narrow it by two stud widths.
4. Studs turn with the wall direction, not +90°: a column section's depth is its local y, so this puts the depth across the wall, which is what the spec wants. Task 3 verifies it on the catalog Ue profile, with a fallback.
5. End studs are flush with the wall ends and keyed `END_STUD = u16::MAX`. The regular stud next to the end is dropped when too close; the end stud stays.
6. Cripple indices are `2k` (above) and `2k + 1` (below). Kings and jacks are 0 on the start side and 1 on the end side. Blocking is keyed by its left stud and uses the track type, with its top at mid-height + t/2. Top plates count from the topmost.
7. All framing beams have joins off. Pieces shorter than 0.05 m are skipped. The header's underside is capped under the top tracks, and a sill is made only when it clears the bottom track.
8. `double_top` on light steel is ignored rather than refused.
9. A wall bent after framing keeps its framing with no members (they come back if it is straightened); only creation refuses curved walls.
10. Openings are ordered by `(offset, id)`, so a skipped opening (no readable size) still takes its index.
11. Copies: a copied wall takes its framing along, and a framing copied alone is skipped. Repair drops framings whose wall is gone and later duplicates.
12. App:
    - OK frames the selected walls, or the active level's walls when none is selected, filtered to straight walls not framed yet, with a message when none is left;
    - the palette shows the system read-only; the dialog's system choice resets spacing, double top and types;
    - one new icon serves both buttons;
    - the *Framing* row shows only on members of wall framings (`Project::member_host_of` is generic).
13. Not covered: a change of a level elevation that moves a wall's top without changing the wall, and a wall top attached to a roof (the framing uses `wall_top_z`). Both regenerate on the wall's or framing's next change.
