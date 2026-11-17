# Steel and Timber Connections Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A parametric `Connection` element at one end of a column or beam (base plate, end plate, fin plate, timber bolt group, hanger) whose plates and bolts `cc_geom` derives from the member on every build, placed by `connections_create`, edited through `element_set`, scheduled by plate mass and bolts, exported to IFC as an element assembly of plates and fasteners, and placed from the Structure tab's steel and timber *Connections* groups.

**Architecture:**
- `cc_core::connection` holds the model: `Connection { member, end: MemberEnd, kind: ConnectionKind, params: ConnectionParams }`, the bounds (`LENGTH`, `COUNT`), the defaults per kind, `fits`, `Project::connections_of`/`connection_at` and `repair_connections` (one per member end, out-of-range values reset). It is wired like rebar: removed with its member, follows its phases and option, never copied, placed by its member (no placement of its own).
- `cc_geom::connection` builds a `ConnectionGeom` per connection after the beams are resolved (`ModelGeometry::connections`): plates as oriented `Block`s, bolts as `Rod`s (washers too), plate volume/mass, bolt count/length, or a `ConnectionWarning` and nothing else; `build_all` adds one steel solid per connection (category `Connection`). Plans draw it with `draw_mesh`.
- `cc_engine::commands::connection` holds `connections_create`, the `element_set` check and `element_get`'s derived values; `cc_core::schedule` + `cc_views::schedule` get the four new fields.
- `cc_ifc::export` writes an `IfcElementAssembly` per connection aggregating `IfcPlate`s and `IfcMechanicalFastener`s, tied to the member by `IfcRelConnectsElements`.
- `cc_app`: `UiCommand::Connection(kind)` buttons in new *Connections* groups (steel, timber), `connection_ui.rs` (placing, palette rows), one shared `connection` icon.

**Tech Stack:** Rust 2024 workspace. egui/eframe app with egui_kittest UI tests. Fluent i18n in 12 languages. IFC4 STEP writer (in-house). cargo-nextest.

**Spec:** `docs/superpowers/specs/2026-11-13-connections-design.md`

## Global Constraints

- Work only in the worktree `D:\constructcad-connections` (branch `feat/connections`, stacked on `feat/wall-framing`). Never touch `D:\constructcad`, `D:\constructcad-truss` or any other worktree. Before Task 1, once `feat/wall-framing` has all its tasks committed: `git -C D:\constructcad-truss worktree add D:\constructcad-connections -b feat/connections feat/wall-framing`, then copy the spec to `docs/superpowers/specs/2026-11-13-connections-design.md` and this plan to `docs/superpowers/plans/2026-11-13-connections.md` in the new worktree (committed with Task 1).
- Never push, never run `gh`, never merge. Commit with `git commit -s`. No Claude attribution, no Co-Authored-By.
- Non-test code never panics: no `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!` or `unimplemented!` (clippy denies them). No `unsafe`.
- Every new interface or transaction text is an i18n key added to all 12 files in `assets/i18n/`: `pt-BR`, `en`, `es`, `fr`, `de`, `it`, `lo`, `pl`, `uk-UA`, `zh-TW`, `zh-CN`, `ja` (tests fail on a missing key, or on languages with different keys or placeholders).
- Every engine command parameter has a doc comment; `connections_create` is registered in `crates/cc_engine/src/registry.rs`. Regenerate the reference with `cargo run -q -p cc_cli -- reference > docs/commands.md` (a test checks it).
- Integration tests are one binary per crate: a new file in `crates/cc_engine/tests/` or `crates/cc_ifc/tests/` needs `mod <name>;` in that folder's `main.rs`.
- Tests run with cargo-nextest: `cargo nextest run -p <crate> <filter>` (`--test main` for integration tests). `cargo check --tests -p <crate>` for type errors. Doctests: `cargo test --workspace --doc`.
- rustfmt `max_width = 120`: `cargo fmt --all`. Clippy: `cargo clippy --workspace --all-targets -- -D warnings`.
- Code, comments and identifiers are in English.
- `cc_core`, `cc_geom` and `cc_ifc` are the stable base. `feat/wall-framing` adds a `WallFraming` member host and may add a `Category` (and icons): every fixed-size array this plan grows (`Category::ALL`, `MODEL`, `SCHEDULABLE`, `Icon::ALL`) grows by one from whatever length the base has; the counts below assume the `feat/parametric-truss` base (36, 21, 24, 182).
- Names are new and must not clash: module `cc_core::connection`; `Connection`, `ConnectionKind`, `ConnectionParams`, `MemberEnd`; `Category::Connection`, `TypeData::Connection {}`, `ElementKind::Connection(Connection)`; module `cc_geom::connection` (`Block`, `Rod`, `Bolt`, `Plate`, `ConnectionGeom`, `ConnectionWarning`); `ScheduleField::{ConnectionKind, PlateMass, BoltCount, BoltDiameter}`; `cc_engine::commands::connection::ConnectionsCreate` (`connections_create`); `UiCommand::Connection(ConnectionKind)`; `Icon::Connection`; `cc_app::connection_ui`. (`ComponentConnector`, `component_connectors` and `Icon::Connector` exist and are unrelated.)
- Values (verbatim from the spec, plus the decisions below):
  - Kinds `BasePlate` (steel, a column's base), `EndPlate` (steel, a beam end at a column face), `FinPlate` (steel, a beam end at a column or a beam), `TimberBolts` (timber, a beam end at a column), `Hanger` (steel for timber, a beam end at a beam); codes `base_plate`, `end_plate`, `fin_plate`, `timber_bolts`, `hanger`.
  - `MemberEnd`: `Base` (columns), `Start`, `End` (beams).
  - Every length 1 mm to 2 m (`LENGTH = 0.001..=2.0`); `width` and `length` may also be 0 ("from the member section plus `edge`"); rows and columns 1 to 10 (`COUNT`). Commands refuse out-of-range values; repair resets them to the kind's defaults.
  - A member end carries at most one connection: repair keeps the lowest id. Copying a member does not copy its connections.
  - Steel density 7850 kg/m³ (`cc_geom::rebar::STEEL_DENSITY`).
  - Fin plate: length `2·edge + (columns − 1)·spacing` along the beam, height `0.8 ×` section height (`FIN_HEIGHT`), starting at the end face. Hanger sides to `0.6 ×` the beam height (`HANGER_SIDE`), extending `edge` along the beam. End and fin plate bolts are `t + 20 mm` long (`BOLT_EXTRA = 0.02`); timber bolts are beam width `+ 2 ×` washer thickness.
- Decisions this plan makes where the spec is open (each also listed at the end):
  - Defaults per kind (`thickness, width, length, bolt_diameter, rows, columns, spacing, edge, anchor_length`): BasePlate `0.025, 0, 0, 0.020, 2, 2, 0.15, 0.05, 0.40`; EndPlate `0.016, 0, 0, 0.016, 2, 2, 0.10, 0.04, 0.40`; FinPlate `0.010, 0, 0, 0.016, 3, 1, 0.07, 0.04, 0.40`; TimberBolts `0.005, 0, 0, 0.012, 2, 2, 0.10, 0.08, 0.40`; Hanger `0.002, 0, 0, 0.010, 1, 2, 0.05, 0.10, 0.40`.
  - What `width`/`length` mean per kind: base plate: along the column section's local x / y; end plate: across the beam / up; fin plate: `length` along the beam, `width` its height; hanger: `length` along the beam (0: `edge`), `width` the inside width between its sides (0: the beam width). TimberBolts ignore both.
  - Bolt layout: base and end plates spread `columns` × `rows` evenly between the edge distances (one bolt: the middle; `spacing` unused). Fin plates and bolted joints put columns along the beam at `edge + i·spacing` from the face and rows centered vertically `spacing` apart. Hangers spread their bolts over the middle half of the plate length and of the side height, across the beam through both sides (length inside width `+ 2t + 20 mm`).
  - Anchor bolts run from the plate's top to `anchor_length` below its bottom (length `t + anchor_length`).
  - Timber washers: thickness = `thickness`, diameter `3 ×` bolt diameter (`WASHER_RATIO`); a bolted joint has no plate (plate volume and mass 0).
  - Beam end faces are vertical (the body is built that way, also on sloped beams): the end plate stands against the face, normal to the beam's plan axis (for level beams, the beam axis).
  - A hanger is one bent plate of three boxes (base across the inside width only, sides from the base's bottom to `0.6 h`, so they do not overlap): one `IfcPlate`.
  - A connection that cannot be built gets a `ConnectionWarning`: `MissingMember`, or `Unsuitable` (the member is of the other category, its end is not framed as the kind needs, or its section fails). It has no parts and no solid.
  - Solids use the project's first material of class `Metal`; with none, the gray `STEEL_COLOR`.
  - `connections_create`: `members` empty → the members of `level` if given, else of the session's active level, else of every level; explicit members of the wrong category are refused; ends that do not suit are not candidates; `skipped` counts candidate ends that already have a connection.
  - `element_set` refuses a change of `member`, `end` or `kind` (place another); `params` are checked. The element's `params` nest the parameters: `{"params": {"params": {"rows": 3}}}`.
  - Plans draw a connection where its member stands (`project.element(member).level`), as cut and seen (`draw_mesh`); structural plans show them.
  - Buttons use the singular kind names (Base Plate, End Plate, Fin Plate, Bolted Joint, Hanger), as Column and Beam do; one shared `connection` icon without a badge (badges mark the kind of action in this codebase).
  - Schedules of connections: available `ConnectionKind, Host, PlateMass, BoltCount, BoltDiameter, Volume` (plus the common, type and phase fields); defaults `Mark, ConnectionKind, PlateMass, BoltCount, BoltDiameter, Comments`; `PlateMass` and `BoltCount` add up in the totals; `BoltDiameter` reads `Ø 16 mm` (inches when imperial).
  - IFC: plates `NOTDEFINED` with an `IfcPlateType` per connection type; fasteners typed by an `IfcMechanicalFastenerType` per connection type and name (`Bolt`, `Anchor Bolt`); a bolt's washers are items of its fastener's body; parts are placed relative to the assembly. A new fixture `crates/cc_ifc/tests/fixtures/connections.ccad` is exported and validated in CI, and `scripts/validate_ifc.py` checks `IfcPlate` and `IfcMechanicalFastener` too.
  - No `FORMAT_VERSION` bump (new variants only, as trusses did).
  - A connection's `level` is its member's at creation.

## i18n keys

Each task adds the keys it uses to all 12 files, appended at the end under a comment `# Connections` (Task 1 creates the comment; later tasks append under it). Columns: key, task, then pt-BR, en, es, fr, de, it, lo, pl, uk-UA, zh-TW, zh-CN, ja.

| key | task | pt-BR | en | es | fr | de | it | lo | pl | uk-UA | zh-TW | zh-CN | ja |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| category-connection | 1 | Ligações | Connections | Uniones | Assemblages | Anschlüsse | Giunti | ຂໍ້ຕໍ່ | Połączenia | З'єднання | 接頭 | 节点 | 接合部 |
| tpl-type-connection | 1 | Ligação | Connection | Unión | Assemblage | Anschluss | Giunto | ຂໍ້ຕໍ່ | Połączenie | З'єднання | 接頭 | 节点 | 接合部 |
| connection-base-plate | 1 | Placa de base | Base Plate | Placa base | Platine de pied | Fußplatte | Piastra di base | ແຜ່ນຮອງຕີນເສົາ | Blacha podstawy | Опорна плита | 柱底板 | 柱脚底板 | ベースプレート |
| connection-end-plate | 1 | Placa de topo | End Plate | Placa de testa | Platine d'about | Stirnplatte | Piastra di testa | ແຜ່ນປາຍຄານ | Blacha czołowa | Торцева пластина | 端板 | 端板 | エンドプレート |
| connection-fin-plate | 1 | Chapa de alma | Fin Plate | Cartela de alma | Gousset d'âme | Fahnenblech | Piastra d'anima | ແຜ່ນຕໍ່ເອວຄານ | Blacha żeberkowa | Ребро на стінці | 腹板連接板 | 腹板连接板 | フィンプレート |
| connection-timber-bolts | 1 | Ligação parafusada | Bolted Joint | Unión atornillada | Assemblage boulonné | Bolzenverbindung | Giunto bullonato | ຂໍ້ຕໍ່ໂບລທ໌ | Połączenie śrubowe | Болтове з'єднання | 螺栓接頭 | 螺栓节点 | ボルト接合 |
| connection-hanger | 1 | Suporte de viga | Hanger | Colgador de viga | Sabot de solive | Balkenschuh | Scarpa per travi | ເຫຼັກແຂວນຄານ | Wieszak belki | Підвіс балки | 梁托架 | 梁托 | 梁受け金物 |
| member-end-base | 1 | Base | Base | Base | Pied | Fuß | Base | ຕີນ | Podstawa | Основа | 底部 | 底部 | 柱脚 |
| col-connection-kind | 3 | Tipo de ligação | Connection Kind | Clase de unión | Genre d'assemblage | Anschlussart | Tipologia di giunto | ປະເພດຂໍ້ຕໍ່ | Rodzaj połączenia | Вид з'єднання | 接頭類別 | 节点种类 | 接合部の種類 |
| col-plate-mass | 3 | Massa das placas | Plate Mass | Masa de placas | Masse des platines | Plattenmasse | Massa piastre | ມວນແຜ່ນ | Masa blach | Маса пластин | 鋼板質量 | 钢板质量 | プレート質量 |
| col-bolt-count | 3 | Parafusos | Bolts | Pernos | Boulons | Schrauben | Bulloni | ໂບລທ໌ | Śruby | Болти | 螺栓數 | 螺栓数 | ボルト本数 |
| col-bolt-diameter | 3 | Diâmetro do parafuso | Bolt Diameter | Diámetro del perno | Diamètre des boulons | Schraubendurchmesser | Diametro bulloni | ເສັ້ນຜ່າໃຈກາງໂບລທ໌ | Średnica śrub | Діаметр болтів | 螺栓直徑 | 螺栓直径 | ボルト径 |
| tx-create-connections | 3 | Criar ligações | Create Connections | Crear uniones | Créer des assemblages | Anschlüsse erstellen | Crea giunti | ສ້າງຂໍ້ຕໍ່ | Utwórz połączenia | Створити з'єднання | 建立接頭 | 创建节点 | 接合部を作成 |
| group-connections | 5 | Ligações | Connections | Uniones | Assemblages | Anschlüsse | Giunti | ຂໍ້ຕໍ່ | Połączenia | З'єднання | 接頭 | 节点 | 接合部 |
| section-connection | 5 | Ligação | Connection | Unión | Assemblage | Anschluss | Giunto | ຂໍ້ຕໍ່ | Połączenie | З'єднання | 接頭 | 节点 | 接合部 |
| prop-connection-kind | 5 | Tipo de ligação | Kind | Clase | Genre | Art | Tipologia | ປະເພດ | Rodzaj | Вид | 類別 | 种类 | 種類 |
| prop-member-end | 5 | Extremidade | Member End | Extremo | Extrémité | Stabende | Estremità | ປາຍຊິ້ນສ່ວນ | Koniec pręta | Кінець елемента | 構件端 | 构件端 | 部材端 |
| prop-plate-thickness | 5 | Espessura da placa | Plate Thickness | Espesor de placa | Épaisseur de platine | Plattendicke | Spessore piastra | ຄວາມໜາແຜ່ນ | Grubość blachy | Товщина пластини | 板厚 | 板厚 | 板厚 |
| prop-plate-width | 5 | Largura da placa | Plate Width | Ancho de placa | Largeur de platine | Plattenbreite | Larghezza piastra | ຄວາມກວ້າງແຜ່ນ | Szerokość blachy | Ширина пластини | 板寬 | 板宽 | 板幅 |
| prop-plate-length | 5 | Comprimento da placa | Plate Length | Largo de placa | Longueur de platine | Plattenlänge | Lunghezza piastra | ຄວາມຍາວແຜ່ນ | Długość blachy | Довжина пластини | 板長 | 板长 | 板長 |
| prop-bolt-rows | 5 | Linhas de parafusos | Bolt Rows | Filas de pernos | Rangées de boulons | Schraubenreihen | File di bulloni | ແຖວໂບລທ໌ | Rzędy śrub | Ряди болтів | 螺栓排數 | 螺栓排数 | ボルト行数 |
| prop-bolt-columns | 5 | Colunas de parafusos | Bolt Columns | Columnas de pernos | Files de boulons | Schraubenspalten | Colonne di bulloni | ຖັນໂບລທ໌ | Kolumny śrub | Стовпці болтів | 螺栓列數 | 螺栓列数 | ボルト列数 |
| prop-bolt-spacing | 5 | Espaçamento dos parafusos | Bolt Spacing | Separación de pernos | Entraxe des boulons | Schraubenabstand | Interasse bulloni | ໄລຍະຫ່າງໂບລທ໌ | Rozstaw śrub | Крок болтів | 螺栓間距 | 螺栓间距 | ボルトピッチ |
| prop-edge-distance | 5 | Distância à borda | Edge Distance | Distancia al borde | Pince | Randabstand | Distanza dal bordo | ໄລຍະຈາກຂອບ | Odległość od krawędzi | Відстань до краю | 邊距 | 边距 | 縁端距離 |
| prop-anchor-length | 5 | Comprimento do chumbador | Anchor Length | Longitud de anclaje | Longueur d'ancrage | Ankerlänge | Lunghezza tirafondi | ຄວາມຍາວສະມໍ | Długość kotew | Довжина анкерів | 錨栓長度 | 锚栓长度 | アンカー長さ |
| connection-size-auto | 5 | auto | auto | auto | auto | auto | auto | ອັດຕະໂນມັດ | auto | Автоматично | 自動 | 自动 | 自動 |

Keys with longer text (all Task 5):

`msg-connections-placed` (placeholders `$count`, `$skipped`):
- pt-BR: { $count } ligações colocadas ({ $skipped } extremidades já tinham uma).
- en: { $count } connections placed ({ $skipped } member ends already had one).
- es: { $count } uniones colocadas ({ $skipped } extremos ya tenían una).
- fr: { $count } assemblages placés ({ $skipped } extrémités en avaient déjà un).
- de: { $count } Anschlüsse gesetzt ({ $skipped } Stabenden hatten bereits einen).
- it: { $count } giunti inseriti ({ $skipped } estremità ne avevano già uno).
- lo: ວາງຂໍ້ຕໍ່ແລ້ວ { $count } ອັນ ({ $skipped } ປາຍມີຂໍ້ຕໍ່ຢູ່ແລ້ວ).
- pl: Wstawione połączenia: { $count } (końce, które już je miały: { $skipped }).
- uk-UA: Розміщено з'єднань: { $count }. Кінців, що вже мали з'єднання: { $skipped }.
- zh-TW: 已放置 { $count } 個接頭（另有 { $skipped } 個構件端已有接頭）。
- zh-CN: 已放置 { $count } 个节点（{ $skipped } 个构件端已有节点）。
- ja: { $count } 個の接合部を配置しました (すでに接合部のある { $skipped } 箇所の部材端はスキップ)。

`warn-connection-end`:
- pt-BR: Esta extremidade não aceita este tipo de ligação; nada é desenhado.
- en: This member end does not take this kind of connection; nothing is drawn.
- es: Este extremo no admite esta clase de unión; no se dibuja nada.
- fr: Cette extrémité n'accepte pas ce genre d'assemblage ; rien n'est dessiné.
- de: Dieses Stabende nimmt diese Anschlussart nicht auf; es wird nichts gezeichnet.
- it: Questa estremità non accetta questa tipologia di giunto; non viene disegnato nulla.
- lo: ປາຍນີ້ບໍ່ຮັບຂໍ້ຕໍ່ປະເພດນີ້; ບໍ່ມີຫຍັງຖືກແຕ້ມ.
- pl: Ten koniec nie przyjmuje tego rodzaju połączenia; nic nie jest rysowane.
- uk-UA: Цей кінець не приймає такий вид з'єднання; нічого не креслиться.
- zh-TW: 此構件端不適用此類接頭，因此不繪製。
- zh-CN: 此构件端不适用此类节点，因此不绘制。
- ja: この部材端にはこの種類の接合部を設けられないため、何も描画されません。

`cmd-desc-base-plate`:
- pt-BR: Coloca uma placa de base de aço com chumbadores sob cada pilar selecionado (ou todos os do nível ativo), com tamanhos tirados da seção do pilar.
- en: Place a steel base plate with anchor bolts under each selected column (or every column of the active level), sized from the column's section.
- es: Coloca una placa base de acero con pernos de anclaje bajo cada pilar seleccionado (o todos los del nivel activo), con tamaños tomados de la sección del pilar.
- fr: Place une platine de pied en acier avec tiges d'ancrage sous chaque poteau sélectionné (ou tous ceux du niveau actif), dimensionnée d'après la section du poteau.
- de: Setzt eine Stahlfußplatte mit Ankerbolzen unter jede ausgewählte Stütze (oder alle der aktiven Ebene), bemessen nach dem Stützenquerschnitt.
- it: Inserisce una piastra di base in acciaio con tirafondi sotto ogni pilastro selezionato (o tutti quelli del livello attivo), dimensionata dalla sezione del pilastro.
- lo: ວາງແຜ່ນຮອງຕີນເສົາເຫຼັກພ້ອມໂບລທ໌ສະມໍ ໃຕ້ເສົາທີ່ເລືອກແຕ່ລະຕົ້ນ (ຫຼື ທຸກຕົ້ນຂອງຊັ້ນທີ່ໃຊ້ຢູ່), ຂະໜາດຕາມໜ້າຕັດເສົາ.
- pl: Wstawia stalową blachę podstawy z kotwami pod każdym zaznaczonym słupem (lub wszystkimi na aktywnym poziomie), o wymiarach z przekroju słupa.
- uk-UA: Розміщує сталеву опорну плиту з анкерними болтами під кожною вибраною колоною (або всіма на активному рівні), з розмірами за перерізом колони.
- zh-TW: 在每根選取的柱（或作用中樓層的所有柱）下方放置附錨栓的鋼柱底板，尺寸依柱斷面而定。
- zh-CN: 在每根选中的柱（或当前标高的所有柱）下放置带锚栓的钢柱脚底板，尺寸取自柱截面。
- ja: 選択した各柱（または作業中のレベルのすべての柱）の下に、アンカーボルト付きの鋼製ベースプレートを柱断面に合わせた寸法で配置します。

`cmd-desc-end-plate`:
- pt-BR: Coloca uma placa de topo parafusada em cada extremidade de viga que termina na face de um pilar (vigas selecionadas, ou as do nível ativo).
- en: Place a bolted end plate at each beam end that stops at a column face (the selected beams, or those of the active level).
- es: Coloca una placa de testa atornillada en cada extremo de viga que termina en la cara de un pilar (las vigas seleccionadas, o las del nivel activo).
- fr: Place une platine d'about boulonnée à chaque extrémité de poutre arrêtée contre la face d'un poteau (les poutres sélectionnées, ou celles du niveau actif).
- de: Setzt eine verschraubte Stirnplatte an jedes Trägerende, das an einer Stützenfläche endet (die ausgewählten Träger oder die der aktiven Ebene).
- it: Inserisce una piastra di testa bullonata a ogni estremità di trave che termina sulla faccia di un pilastro (le travi selezionate, o quelle del livello attivo).
- lo: ວາງແຜ່ນປາຍຄານແບບໂບລທ໌ ທີ່ປາຍຄານທຸກປາຍທີ່ຈົບຢູ່ໜ້າເສົາ (ຄານທີ່ເລືອກ ຫຼື ຄານຂອງຊັ້ນທີ່ໃຊ້ຢູ່).
- pl: Wstawia skręcaną blachę czołową na każdym końcu belki, który kończy się na licu słupa (zaznaczone belki lub te z aktywnego poziomu).
- uk-UA: Розміщує болтову торцеву пластину на кожному кінці балки, що впирається в грань колони (вибрані балки або балки активного рівня).
- zh-TW: 在每個止於柱面的梁端放置螺栓端板（選取的梁，或作用中樓層的梁）。
- zh-CN: 在每个止于柱面的梁端放置螺栓端板（选中的梁，或当前标高的梁）。
- ja: 柱面で止まる各梁端にボルト留めのエンドプレートを配置します（選択した梁、または作業中のレベルの梁）。

`cmd-desc-fin-plate`:
- pt-BR: Coloca uma chapa de alma parafusada em cada extremidade de viga que chega a um pilar ou a outra viga (vigas selecionadas, ou as do nível ativo).
- en: Place a bolted fin plate along the web at each beam end that meets a column or another beam (the selected beams, or those of the active level).
- es: Coloca una cartela de alma atornillada en cada extremo de viga que llega a un pilar o a otra viga (las vigas seleccionadas, o las del nivel activo).
- fr: Place un gousset d'âme boulonné à chaque extrémité de poutre qui rejoint un poteau ou une autre poutre (les poutres sélectionnées, ou celles du niveau actif).
- de: Setzt ein verschraubtes Fahnenblech am Steg jedes Trägerendes, das auf eine Stütze oder einen anderen Träger trifft (die ausgewählten Träger oder die der aktiven Ebene).
- it: Inserisce una piastra d'anima bullonata a ogni estremità di trave che incontra un pilastro o un'altra trave (le travi selezionate, o quelle del livello attivo).
- lo: ວາງແຜ່ນຕໍ່ເອວຄານແບບໂບລທ໌ ທີ່ປາຍຄານທຸກປາຍທີ່ພົບເສົາ ຫຼື ຄານອື່ນ (ຄານທີ່ເລືອກ ຫຼື ຄານຂອງຊັ້ນທີ່ໃຊ້ຢູ່).
- pl: Wstawia skręcaną blachę żeberkową przy środniku na każdym końcu belki, który styka się ze słupem lub inną belką (zaznaczone belki lub te z aktywnego poziomu).
- uk-UA: Розміщує болтове ребро вздовж стінки на кожному кінці балки, що сходиться з колоною чи іншою балкою (вибрані балки або балки активного рівня).
- zh-TW: 在每個與柱或其他梁相接的梁端，沿腹板放置螺栓腹板連接板（選取的梁，或作用中樓層的梁）。
- zh-CN: 在每个与柱或其他梁相接的梁端，沿腹板放置螺栓腹板连接板（选中的梁，或当前标高的梁）。
- ja: 柱または他の梁に接する各梁端に、ウェブに沿ってボルト留めのフィンプレートを配置します（選択した梁、または作業中のレベルの梁）。

`cmd-desc-timber-bolts`:
- pt-BR: Coloca um grupo de parafusos com arruelas em cada extremidade de viga que chega a um pilar (vigas selecionadas, ou as do nível ativo).
- en: Place a bolt group with washers at each beam end that meets a column (the selected beams, or those of the active level).
- es: Coloca un grupo de pernos con arandelas en cada extremo de viga que llega a un pilar (las vigas seleccionadas, o las del nivel activo).
- fr: Place un groupe de boulons avec rondelles à chaque extrémité de poutre qui rejoint un poteau (les poutres sélectionnées, ou celles du niveau actif).
- de: Setzt eine Schraubengruppe mit Unterlegscheiben an jedes Trägerende, das auf eine Stütze trifft (die ausgewählten Träger oder die der aktiven Ebene).
- it: Inserisce un gruppo di bulloni con rondelle a ogni estremità di trave che incontra un pilastro (le travi selezionate, o quelle del livello attivo).
- lo: ວາງກຸ່ມໂບລທ໌ພ້ອມແຫວນຮອງ ທີ່ປາຍຄານທຸກປາຍທີ່ພົບເສົາ (ຄານທີ່ເລືອກ ຫຼື ຄານຂອງຊັ້ນທີ່ໃຊ້ຢູ່).
- pl: Wstawia grupę śrub z podkładkami na każdym końcu belki, który styka się ze słupem (zaznaczone belki lub te z aktywnego poziomu).
- uk-UA: Розміщує групу болтів із шайбами на кожному кінці балки, що сходиться з колоною (вибрані балки або балки активного рівня).
- zh-TW: 在每個與柱相接的梁端放置附墊圈的螺栓群（選取的梁，或作用中樓層的梁）。
- zh-CN: 在每个与柱相接的梁端放置带垫圈的螺栓群（选中的梁，或当前标高的梁）。
- ja: 柱に接する各梁端に、座金付きのボルト群を配置します（選択した梁、または作業中のレベルの梁）。

`cmd-desc-hanger`:
- pt-BR: Coloca um suporte de viga em chapa de aço dobrada sob cada extremidade de viga que chega a outra viga (vigas selecionadas, ou as do nível ativo).
- en: Place a bent steel hanger under each beam end that meets another beam (the selected beams, or those of the active level).
- es: Coloca un colgador de chapa de acero plegada bajo cada extremo de viga que llega a otra viga (las vigas seleccionadas, o las del nivel activo).
- fr: Place un sabot en tôle d'acier pliée sous chaque extrémité de poutre qui rejoint une autre poutre (les poutres sélectionnées, ou celles du niveau actif).
- de: Setzt einen Balkenschuh aus gekantetem Stahlblech unter jedes Trägerende, das auf einen anderen Träger trifft (die ausgewählten Träger oder die der aktiven Ebene).
- it: Inserisce una scarpa in lamiera d'acciaio piegata sotto ogni estremità di trave che incontra un'altra trave (le travi selezionate, o quelle del livello attivo).
- lo: ວາງເຫຼັກແຂວນຄານແບບແຜ່ນເຫຼັກພັບ ໃຕ້ປາຍຄານທຸກປາຍທີ່ພົບຄານອື່ນ (ຄານທີ່ເລືອກ ຫຼື ຄານຂອງຊັ້ນທີ່ໃຊ້ຢູ່).
- pl: Wstawia wieszak z giętej blachy stalowej pod każdym końcem belki, który styka się z inną belką (zaznaczone belki lub te z aktywnego poziomu).
- uk-UA: Розміщує підвіс із гнутого сталевого листа під кожним кінцем балки, що сходиться з іншою балкою (вибрані балки або балки активного рівня).
- zh-TW: 在每個與其他梁相接的梁端下方放置彎折鋼板梁托架（選取的梁，或作用中樓層的梁）。
- zh-CN: 在每个与其他梁相接的梁端下放置折弯钢板梁托（选中的梁，或当前标高的梁）。
- ja: 他の梁に接する各梁端の下に、鋼板を折り曲げた梁受け金物を配置します（選択した梁、または作業中のレベルの梁）。

Existing keys reused (do not add): `prop-host` (the member), `prop-start`, `prop-end` (member ends), `col-plate-mass`/`col-bolt-count`/`col-bolt-diameter` also label the palette's rows.

---

### Task 1: The connection model (cc_core)

**Files:**
- Create: `crates/cc_core/src/connection.rs`
- Modify:
  - `crates/cc_core/src/lib.rs`: `pub mod connection;` after `pub mod component_script;`; `pub use connection::{Connection, ConnectionKind, ConnectionParams, MemberEnd};` next to the other `pub use` lines.
  - `crates/cc_core/src/element.rs`: variant `/// A steel or timber joint at one end of a column or beam (placed by its member).` `Connection(crate::connection::Connection),` after `Truss(..)` (after `WallFraming` if the base has it); `Element::category` arm `ElementKind::Connection(_) => Category::Connection,`; `Element::translate`: add `| ElementKind::Connection(_)` to the "Placed by their hosts." arm.
  - `crates/cc_core/src/transform.rs`: `Element::transform`: `ElementKind::Rebar(_) | ElementKind::Valve(_) | ElementKind::Connection(_) => {}` (comment "Bars, valves and connections follow their host."); `Project::copy_set`: replace the final `out.retain(..)` by `out.retain(|e| !managed.contains(&e.id) && !matches!(e.kind, ElementKind::Connection(_)));` and extend its doc comment: "Connections are never copied: they belong to their member's end."
  - `crates/cc_core/src/document.rs` (`remove_element`, ~255): `pending.extend(self.project.connections_of(id).map(|(e, _)| e.id));` after the rebar line.
  - `crates/cc_core/src/phase.rs` (`phase_host`): `ElementKind::Connection(c) => Some(c.member),`; doc: "..., the stair of a railing, the member of a connection."
  - `crates/cc_core/src/types.rs`:
    - `Category::Connection` at the end of the enum (doc `/// Steel and timber connections: plates and bolts at member ends.`);
    - `Category::ALL` grows by one with `Category::Connection` right after `Category::Rebar`; `MODEL` and `SCHEDULABLE` grow by one with it after `Category::Rebar`;
    - `Category::key`: `Category::Connection => "category-connection",`;
    - `TypeData::Connection {}` after `Truss {}` (doc `/// Connections (no properties of their own; types name them).`);
    - every `TypeData` match that lists `| TypeData::Truss {}` (materials ~612, rename_material ~687, main_material ~1004) adds `| TypeData::Connection {}`; `ElementType::category`: `TypeData::Connection {} => Category::Connection,`.
  - `crates/cc_core/src/repair.rs`: `element_is_valid`: `ElementKind::Connection(_) => true,` (comment: "Out-of-range values are reset by `repair_connections`."); type loop: `| TypeData::Connection {}` next to `| TypeData::Truss {}`; `host_ok`: `ElementKind::Connection(c) => self.element(c.member).is_some_and(|m| c.fits(&m.kind)),`; after `repairs += self.repair_member_hosts();` add `// Connections: one per member end, numbers in range.` `repairs += self.repair_connections();`.
  - `crates/cc_core/src/schedule.rs`: `available`: `Category::Connection => &[],` and `defaults`: `Category::Connection => Vec::new(),` for now (Task 3 fills them).
  - Other crates' exhaustive matches the compiler lists (`cargo check --workspace --tests`): `crates/cc_views/src/legend.rs` (~260) `| TypeData::Connection {}`; `crates/cc_ifc/src/export.rs` (~1724) `| TypeData::Connection {}` (comment: "..., trusses, connections and pads have no IFC type object"); `crates/cc_app/src/commands.rs` `place_label` `Category::Connection => t!("category-connection"),` and `place_description` `Category::Connection => t!("category-connection"),`; `crates/cc_app/src/tools.rs` (~388) add `| Category::Connection` to the `return None` arm; `crates/cc_app/src/ribbon.rs` `category_icon`: `Category::Connection => Icon::Rebar,` for now (Task 5 switches it to `Icon::Connection`); `crates/cc_app/src/properties.rs` (~543): `ElementKind::Connection(_) => {}` for now.
  - `assets/i18n/*.ftl` (12): the Task 1 keys of the table (`category-connection`, `tpl-type-connection`, the five `connection-*` kind names, `member-end-base`) under `# Connections`.
- Test: `crates/cc_core/src/connection.rs` (`mod tests`).

**Interfaces:**
- Consumes: `crate::member_host::{within, reset}` (`pub(crate)`), `EditCtx::{alloc_id, add_type, add_element, remove_element, element_mut}`, `Project::{copy_set, repair, element}`, `crate::StructuralMaterial`.
- Produces (module `cc_core::connection`):
  - `const LENGTH: RangeInclusive<f64> = 0.001..=2.0`, `const COUNT: RangeInclusive<u32> = 1..=10`.
  - `enum ConnectionKind { BasePlate (default), EndPlate, FinPlate, TimberBolts, Hanger }` (`Copy, Eq, Hash, Ord, Serialize, Deserialize`) with `ALL`, `code() -> &'static str`, `from_code(&str) -> Option<Self>`, `key() -> &'static str`, `at_column() -> bool`, `material() -> StructuralMaterial`, `defaults() -> ConnectionParams`.
  - `enum MemberEnd { Base, Start, End }` (`Copy, Eq, Hash, Ord, Serialize, Deserialize`) with `ALL`, `code()`, `key()`.
  - `struct ConnectionParams { thickness, width, length, bolt_diameter: f64, rows, columns: u32, spacing, edge, anchor_length: f64 }` (`Copy, PartialEq, Serialize, Deserialize`, `Default` = BasePlate's) with `check(&self) -> Result<(), String>`, `reset_out_of_range(&mut self, kind) -> usize`.
  - `struct Connection { member: ElementId, end: MemberEnd, kind: ConnectionKind, params: ConnectionParams }` with `new(member, end, kind) -> Self` (the kind's defaults), `fits(&self, member: &ElementKind) -> bool`.
  - `fn host_type(id: ElementTypeId, name: String) -> ElementType`.
  - `Project::connections_of(&self, member) -> impl Iterator<Item = (&Element, &Connection)>`, `Project::connection_at(&self, member, end) -> Option<ElementId>`, `Project::repair_connections(&mut self) -> usize` (`pub(crate)`).
  - `ElementKind::Connection(Connection)`, `Category::Connection`, `TypeData::Connection {}`.

- [ ] **Step 1: Write the module's tests first.** Create `crates/cc_core/src/connection.rs` with only the `use` lines and this test module (the items do not exist yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Column, Document, Element, LevelId, WallTop};
    use glam::DVec2;

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

    /// A template project with one column at (2, 3) and a base plate under
    /// it: `(document, column, connection)`.
    fn site() -> (Document, ElementId, ElementId) {
        let mut doc = Document::new(Project::from_template());
        let level = doc.project.levels[0].id;
        let (column, connection) = doc.edit("site", |e| {
            let column_type = e.project().default_type(crate::Category::Column).expect("a column type");
            let ty = ElementTypeId(e.alloc_id());
            e.add_type(host_type(ty, "Connection".into()));
            let col = Column {
                position: DVec2::new(2.0, 3.0),
                rotation_deg: 0.0,
                base_offset: 0.0,
                height: 3.0,
                top: WallTop::Unconnected,
                structural: true,
            };
            let column = ElementId(e.alloc_id());
            e.add_element(element(column, level, column_type, ElementKind::Column(col)));
            let id = ElementId(e.alloc_id());
            let c = Connection::new(column, MemberEnd::Base, ConnectionKind::BasePlate);
            e.add_element(element(id, level, ty, ElementKind::Connection(c)));
            (column, id)
        });
        (doc, column, connection)
    }

    #[test]
    fn codes_round_trip() {
        for k in ConnectionKind::ALL {
            assert_eq!(ConnectionKind::from_code(k.code()), Some(k));
            assert_eq!(k.defaults().check(), Ok(()), "{k:?} defaults are in range");
        }
        assert_eq!(ConnectionKind::from_code(" End_Plate "), Some(ConnectionKind::EndPlate));
        assert_eq!(ConnectionKind::from_code("weld"), None);
        assert!(ConnectionKind::BasePlate.at_column() && !ConnectionKind::Hanger.at_column());
        assert_eq!(ConnectionKind::TimberBolts.material(), crate::StructuralMaterial::Timber);
        assert_eq!(ConnectionKind::FinPlate.material(), crate::StructuralMaterial::Steel);
    }

    #[test]
    fn bounds_are_checked() {
        let ok = ConnectionKind::EndPlate.defaults();
        let bad = |f: fn(&mut ConnectionParams)| {
            let mut p = ok;
            f(&mut p);
            p.check()
        };
        assert!(bad(|p| p.width = 0.0).is_ok(), "0: from the member");
        assert!(bad(|p| p.width = 0.0005).is_err());
        assert!(bad(|p| p.length = 2.5).is_err());
        assert!(bad(|p| p.thickness = 0.0).is_err(), "only width and length may be 0");
        assert!(bad(|p| p.bolt_diameter = f64::NAN).is_err());
        assert!(bad(|p| p.rows = 0).is_err());
        assert!(bad(|p| p.columns = 11).is_err());
        assert!(bad(|p| p.anchor_length = 2.0).is_ok());
    }

    #[test]
    fn a_connection_fits_its_member_end() {
        let column = ElementKind::Column(Column {
            position: DVec2::ZERO,
            rotation_deg: 0.0,
            base_offset: 0.0,
            height: 3.0,
            top: WallTop::Unconnected,
            structural: true,
        });
        let beam = ElementKind::Beam(crate::Beam::new(DVec2::ZERO, DVec2::X, 3.0));
        let c = |end, kind| Connection::new(ElementId(1), end, kind);
        assert!(c(MemberEnd::Base, ConnectionKind::BasePlate).fits(&column));
        assert!(!c(MemberEnd::Start, ConnectionKind::BasePlate).fits(&column));
        assert!(!c(MemberEnd::Base, ConnectionKind::BasePlate).fits(&beam));
        assert!(c(MemberEnd::End, ConnectionKind::Hanger).fits(&beam));
        assert!(!c(MemberEnd::Base, ConnectionKind::EndPlate).fits(&column));
    }

    /// Repair keeps the first connection of a member end and resets numbers
    /// out of range to the kind's defaults.
    #[test]
    fn repair_keeps_one_per_end_and_resets_numbers() {
        let (doc, column, first) = site();
        let mut p = doc.project.clone();
        let mut second = p.element(first).unwrap().clone();
        second.id = ElementId(p.alloc_id());
        second.uid = crate::Uid::generate();
        p.elements.insert(second.id, second.clone());
        if let Some(ElementKind::Connection(c)) = p.elements.get_mut(&first).map(|e| &mut e.kind) {
            c.params.rows = 0;
            c.params.thickness = f64::NAN;
        }
        assert_eq!(p.repair(), 3, "the second one, rows and thickness");
        assert!(p.element(second.id).is_none());
        let Some(ElementKind::Connection(c)) = p.element(first).map(|e| &e.kind) else { panic!("kept") };
        assert_eq!((c.params.rows, c.params.thickness), (2, 0.025));
        assert_eq!(p.connection_at(column, MemberEnd::Base), Some(first));
    }

    #[test]
    fn a_connection_on_the_wrong_member_is_dropped() {
        let (doc, _, first) = site();
        let mut p = doc.project.clone();
        if let Some(ElementKind::Connection(c)) = p.elements.get_mut(&first).map(|e| &mut e.kind) {
            c.end = MemberEnd::Start;
        }
        assert_eq!(p.repair(), 1);
        assert!(p.element(first).is_none());
    }

    #[test]
    fn it_goes_with_its_member_and_is_not_copied() {
        let (mut doc, column, connection) = site();
        assert!(doc.project.copy_set(&[column, connection]).iter().all(|e| e.id != connection));
        assert_eq!(crate::phase::phase_host(doc.project.element(connection).unwrap()), Some(column));
        doc.edit("delete", |e| e.remove_element(column));
        assert!(doc.project.element(connection).is_none());
        assert_eq!(doc.project.repairs_needed(), 0);
    }
}
```

`site()` uses the template's default column type; if `default_type(Column)` is `None` in the base, add a `TypeData::Column { section: ColumnSection::Rectangular { width: 0.2, depth: 0.2 }, material: "Steel".into(), profile: None, profile_rotation_deg: 0.0, profile_mirror: false }` type the same way.

- [ ] **Step 2: Run them to see them fail.** `cargo nextest run -p cc_core connection`. Expected: compile errors (`Connection`, `ConnectionKind`, `host_type`, … missing).

- [ ] **Step 3: Write the module** (above the tests):

```rust
//! Steel and timber connections: an element at one end of a column or beam
//! (`member`, `end`) of a standard kind, whose plates and bolts
//! `cc_geom::connection` derives from the member's geometry on every build.
//! It has no placement of its own: it is removed with its member
//! ([`crate::EditCtx::remove_element`]), follows its phases and design option
//! ([`crate::phase::phase_host`]), is never copied ([`Project::copy_set`]),
//! and a member end carries at most one ([`Project::repair_connections`]
//! keeps the lowest id).

use crate::{Element, ElementId, ElementKind, ElementType, ElementTypeId, Project, StructuralMaterial, TypeData};
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;

/// Every length of a connection, meters. A plate's `width` and `length`
/// may also be 0: taken from the member's section plus `edge`.
pub const LENGTH: RangeInclusive<f64> = 0.001..=2.0;
/// Bolt rows and columns.
pub const COUNT: RangeInclusive<u32> = 1..=10;

/// The standard joints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub enum ConnectionKind {
    /// A steel plate under a column with anchor bolts.
    #[default]
    BasePlate,
    /// A steel plate on a beam end that stops at a column face, bolted through.
    EndPlate,
    /// A steel plate along the web of a beam end at a column or a beam.
    FinPlate,
    /// A group of bolts with washers through a timber beam end at a column.
    TimberBolts,
    /// A bent steel plate under a timber beam end at a beam.
    Hanger,
}

impl ConnectionKind {
    pub const ALL: [ConnectionKind; 5] = [
        ConnectionKind::BasePlate,
        ConnectionKind::EndPlate,
        ConnectionKind::FinPlate,
        ConnectionKind::TimberBolts,
        ConnectionKind::Hanger,
    ];

    pub fn code(self) -> &'static str {
        match self {
            ConnectionKind::BasePlate => "base_plate",
            ConnectionKind::EndPlate => "end_plate",
            ConnectionKind::FinPlate => "fin_plate",
            ConnectionKind::TimberBolts => "timber_bolts",
            ConnectionKind::Hanger => "hanger",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.code().eq_ignore_ascii_case(code.trim()))
    }

    /// The i18n key of its name.
    pub fn key(self) -> &'static str {
        match self {
            ConnectionKind::BasePlate => "connection-base-plate",
            ConnectionKind::EndPlate => "connection-end-plate",
            ConnectionKind::FinPlate => "connection-fin-plate",
            ConnectionKind::TimberBolts => "connection-timber-bolts",
            ConnectionKind::Hanger => "connection-hanger",
        }
    }

    /// Whether it sits at a column's base (else at a beam's start or end).
    pub fn at_column(self) -> bool {
        self == ConnectionKind::BasePlate
    }

    /// The Structure tab material whose *Connections* group offers it.
    pub fn material(self) -> StructuralMaterial {
        match self {
            ConnectionKind::TimberBolts | ConnectionKind::Hanger => StructuralMaterial::Timber,
            _ => StructuralMaterial::Steel,
        }
    }

    /// The parameters a new connection of this kind gets.
    pub fn defaults(self) -> ConnectionParams {
        let p = |thickness, bolt_diameter, rows, columns, spacing, edge| ConnectionParams {
            thickness,
            width: 0.0,
            length: 0.0,
            bolt_diameter,
            rows,
            columns,
            spacing,
            edge,
            anchor_length: 0.4,
        };
        match self {
            ConnectionKind::BasePlate => p(0.025, 0.020, 2, 2, 0.15, 0.05),
            ConnectionKind::EndPlate => p(0.016, 0.016, 2, 2, 0.10, 0.04),
            ConnectionKind::FinPlate => p(0.010, 0.016, 3, 1, 0.07, 0.04),
            ConnectionKind::TimberBolts => p(0.005, 0.012, 2, 2, 0.10, 0.08),
            ConnectionKind::Hanger => p(0.002, 0.010, 1, 2, 0.05, 0.10),
        }
    }
}

/// Which end of its member a connection sits at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MemberEnd {
    /// A column's base.
    Base,
    /// A beam's start.
    Start,
    /// A beam's end.
    End,
}

impl MemberEnd {
    pub const ALL: [MemberEnd; 3] = [MemberEnd::Base, MemberEnd::Start, MemberEnd::End];

    pub fn code(self) -> &'static str {
        match self {
            MemberEnd::Base => "base",
            MemberEnd::Start => "start",
            MemberEnd::End => "end",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            MemberEnd::Base => "member-end-base",
            MemberEnd::Start => "prop-start",
            MemberEnd::End => "prop-end",
        }
    }
}

/// The sizes of a connection, meters (see the kind for what each means).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConnectionParams {
    /// Plate thickness (a bolted joint's washers).
    pub thickness: f64,
    /// Plate width; 0: from the member's section plus `edge`.
    pub width: f64,
    /// Plate length; 0: from the member's section (or bolts) plus `edge`.
    pub length: f64,
    pub bolt_diameter: f64,
    pub rows: u32,
    pub columns: u32,
    /// Bolt to bolt.
    pub spacing: f64,
    /// Bolt to the plate's edge (or to the member's end).
    pub edge: f64,
    /// Anchor bolts below a base plate.
    pub anchor_length: f64,
}

impl Default for ConnectionParams {
    fn default() -> Self {
        ConnectionKind::BasePlate.defaults()
    }
}

impl ConnectionParams {
    /// Whether every value is in range.
    pub fn check(&self) -> Result<(), String> {
        use crate::member_host::within;
        for (name, v) in [
            ("thickness", self.thickness),
            ("bolt_diameter", self.bolt_diameter),
            ("spacing", self.spacing),
            ("edge", self.edge),
            ("anchor_length", self.anchor_length),
        ] {
            within(name, v, &LENGTH)?;
        }
        for (name, v) in [("width", self.width), ("length", self.length)] {
            if v != 0.0 {
                within(name, v, &LENGTH).map_err(|e| format!("{e} (or 0: from the member)"))?;
            }
        }
        for (name, n) in [("rows", self.rows), ("columns", self.columns)] {
            if !COUNT.contains(&n) {
                return Err(format!("{name} must be between {} and {}", COUNT.start(), COUNT.end()));
            }
        }
        Ok(())
    }

    /// Values out of range back to `kind`'s defaults (a bad width or length
    /// to 0); returns how many changed.
    pub fn reset_out_of_range(&mut self, kind: ConnectionKind) -> usize {
        use crate::member_host::reset;
        let d = kind.defaults();
        let mut n = reset(&mut self.thickness, &LENGTH, d.thickness)
            + reset(&mut self.bolt_diameter, &LENGTH, d.bolt_diameter)
            + reset(&mut self.spacing, &LENGTH, d.spacing)
            + reset(&mut self.edge, &LENGTH, d.edge)
            + reset(&mut self.anchor_length, &LENGTH, d.anchor_length);
        for v in [&mut self.width, &mut self.length] {
            if *v != 0.0 {
                n += reset(v, &LENGTH, 0.0);
            }
        }
        for (v, default) in [(&mut self.rows, d.rows), (&mut self.columns, d.columns)] {
            if !COUNT.contains(v) {
                *v = default;
                n += 1;
            }
        }
        n
    }
}

/// A connection at one end of a column or beam.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    /// The column or beam.
    pub member: ElementId,
    pub end: MemberEnd,
    pub kind: ConnectionKind,
    #[serde(default)]
    pub params: ConnectionParams,
}

impl Connection {
    /// A connection with the kind's default parameters.
    pub fn new(member: ElementId, end: MemberEnd, kind: ConnectionKind) -> Self {
        Self { member, end, kind, params: kind.defaults() }
    }

    /// Whether it can sit on `member` at its end: a base plate on a column's
    /// base, the other kinds on a beam's start or end.
    pub fn fits(&self, member: &ElementKind) -> bool {
        match (member, self.end) {
            (ElementKind::Column(_), MemberEnd::Base) => self.kind.at_column(),
            (ElementKind::Beam(_), MemberEnd::Start | MemberEnd::End) => !self.kind.at_column(),
            _ => false,
        }
    }
}

/// The type a project gets with its first connection.
pub fn host_type(id: ElementTypeId, name: String) -> ElementType {
    ElementType { id, name, data: TypeData::Connection {}, info: Default::default(), uid: Default::default() }
}

impl Project {
    /// The connections on `member`.
    pub fn connections_of(&self, member: ElementId) -> impl Iterator<Item = (&Element, &Connection)> {
        self.elements.values().filter_map(move |e| match &e.kind {
            ElementKind::Connection(c) if c.member == member => Some((e, c)),
            _ => None,
        })
    }

    /// The connection at `member`'s `end`, if it has one.
    pub fn connection_at(&self, member: ElementId, end: MemberEnd) -> Option<ElementId> {
        self.connections_of(member).find(|(_, c)| c.end == end).map(|(e, _)| e.id)
    }

    /// From a file: one connection per member end (the lowest id stays),
    /// numbers back in range; returns the repairs.
    pub(crate) fn repair_connections(&mut self) -> usize {
        let mut repairs = 0;
        let mut seen = std::collections::BTreeSet::new();
        let mut extra = Vec::new();
        for e in self.elements.values_mut() {
            let ElementKind::Connection(c) = &mut e.kind else { continue };
            if !seen.insert((c.member, c.end)) {
                extra.push(e.id);
                continue;
            }
            repairs += c.params.reset_out_of_range(c.kind);
        }
        repairs += extra.len();
        for id in extra {
            self.elements.remove(&id);
        }
        repairs
    }
}
```

- [ ] **Step 4: Wire the variant** as listed under *Files* (lib, element, transform, document, phase, types, repair, schedule, the other crates' arms, the Task 1 i18n keys under a new `# Connections` comment at the end of each `.ftl`). `cargo check --workspace --tests` lists every exhaustive match left; add the `Connection` arm next to the `Truss` one each time.

- [ ] **Step 5: Run the tests.** `cargo nextest run -p cc_core connection repair transform phase` and `cargo nextest run -p cc_engine i18n`. Expected: pass.

- [ ] **Step 6: Clippy, fmt, commit.** `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all`, then:

```bash
git add -A
git commit -s -m "Add the connection element at a member end"
```

---

### Task 2: Connection geometry and plans (cc_geom, cc_views)

**Files:**
- Create: `crates/cc_geom/src/connection.rs`
- Modify:
  - `crates/cc_geom/src/lib.rs`: `pub mod connection;` after `pub mod component;`.
  - `crates/cc_geom/src/build.rs`: field `/// Connections: their plates and bolts, from their members' geometry.` `pub connections: BTreeMap<ElementId, crate::connection::ConnectionGeom>,` after `rebar`; in `build_all`, `geom.build_connections(project);` right after the `geom.rebar = ...` line; the method `build_connections` (Step 4).
  - `crates/cc_views/src/plan.rs`: `is_structural` adds `| ElementKind::Connection(_)`; `draw_connections` called in `plan_of` right after `draw_structure(..)` inside the same `if !ceiling`; a test.
- Test: `crates/cc_geom/src/connection.rs` (`mod tests`), `crates/cc_views/src/plan.rs` (`mod tests`).

**Interfaces:**
- Consumes: Task 1 (`Connection`, `ConnectionKind`, `ConnectionParams`, `MemberEnd`, `connections_of`); `BeamGeom::{world, z_at}` and fields `dir`, `normal`, `half`, `start_ext`, `end_ext`, `start`, `end`; `BeamEnd`; `Project::{type_section, column_span, element_type, materials}`; `SectionOutline::bbox`; `crate::mep::cylinder`; `crate::rebar::STEEL_DENSITY`; `Solid::{new, of_material, add_polygons}`.
- Produces (module `cc_geom::connection`):
  - consts `BOLT_EXTRA = 0.02`, `WASHER_RATIO = 3.0`, `FIN_HEIGHT = 0.8`, `HANGER_SIDE = 0.6`, `STEEL_COLOR: [u8; 3] = [150, 156, 162]`.
  - `struct Block { center: DVec3, x: DVec3, y: DVec3, size: DVec3 }` with `z()`, `volume()`, `faces() -> Vec<Vec<DVec3>>` (`x`, `y` unit and square; `size` along `x`, `y`, `z = x × y`).
  - `struct Rod { a: DVec3, b: DVec3, diameter: f64 }` with `length()`; `struct Bolt { shank: Rod, washers: Vec<Rod> }`; `struct Plate { blocks: Vec<Block> }` with `volume()`.
  - `enum ConnectionWarning { MissingMember, Unsuitable }` with `code() -> &'static str` (`missing_member`, `unsuitable`).
  - `struct ConnectionGeom { element, member: ElementId, kind: ConnectionKind, plates: Vec<Plate>, bolts: Vec<Bolt>, bolt_diameter: f64, warning: Option<ConnectionWarning> }` with `plate_volume()`, `plate_mass()`, `bolt_count()`, `bolt_length()`, `faces()`.
  - `fn beam_end_join(g: &BeamGeom, end: MemberEnd) -> Option<BeamEnd>`, `fn suits(kind: ConnectionKind, joined: BeamEnd) -> bool`, `fn steel_material(project: &Project) -> Option<&str>`, `fn solid(project: &Project, element: ElementId) -> Solid`, `fn connection_geom(project, beams, id, c) -> ConnectionGeom`, `fn build_connections(project, beams) -> BTreeMap<ElementId, ConnectionGeom>`.
  - `ModelGeometry::connections`.

- [ ] **Step 1: Write the failing tests** at the bottom of the new `crates/cc_geom/src/connection.rs` (with its `use` lines only):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModelGeometry;
    use cc_core::{
        Beam, ColumnSection, Document, EditCtx, Element, ElementType, ElementTypeId, LevelId, SectionShape, WallTop,
    };

    fn add(e: &mut EditCtx, level: LevelId, type_id: ElementTypeId, kind: ElementKind) -> ElementId {
        let id = ElementId(e.alloc_id());
        e.add_element(Element {
            id,
            level,
            type_id,
            mark: String::new(),
            comments: String::new(),
            kind,
            uid: Default::default(),
            phase_created: Default::default(),
            phase_demolished: None,
            design_option: None,
            paints: Vec::new(),
        })
    }

    /// A template project with a 0.30 x 0.20 column type (3 m high), a
    /// 0.20 x 0.40 beam type (top at 3 m) and a connection type.
    struct Site {
        doc: Document,
        level: LevelId,
        column: ElementTypeId,
        beam: ElementTypeId,
        connection: ElementTypeId,
    }

    fn site() -> Site {
        let mut doc = Document::new(Project::from_template());
        let level = doc.project.levels[0].id;
        let (column, beam, connection) = doc.edit("types", |e| {
            let mut add_type = |data: TypeData| {
                let id = ElementTypeId(e.alloc_id());
                e.add_type(ElementType { id, name: format!("T{}", id.0), data, info: Default::default(), uid: Default::default() })
            };
            let column = add_type(TypeData::Column {
                section: ColumnSection::Rectangular { width: 0.3, depth: 0.2 },
                material: "Steel".into(),
                profile: None,
                profile_rotation_deg: 0.0,
                profile_mirror: false,
            });
            let beam = add_type(TypeData::Beam {
                width: 0.2,
                height: 0.4,
                material: "Steel".into(),
                shape: SectionShape::Solid,
                profile: None,
                profile_rotation_deg: 0.0,
                profile_mirror: false,
            });
            (column, beam, add_type(TypeData::Connection {}))
        });
        Site { doc, level, column, beam, connection }
    }

    impl Site {
        fn z0(&self) -> f64 {
            self.doc.project.level_elevation(self.level)
        }

        fn column(&mut self, at: DVec2) -> ElementId {
            let (level, ty) = (self.level, self.column);
            let col = cc_core::Column {
                position: at,
                rotation_deg: 0.0,
                base_offset: 0.0,
                height: 3.0,
                top: WallTop::Unconnected,
                structural: true,
            };
            self.doc.edit("column", |e| add(e, level, ty, ElementKind::Column(col)))
        }

        fn beam(&mut self, a: DVec2, b: DVec2) -> ElementId {
            let (level, ty) = (self.level, self.beam);
            self.doc.edit("beam", |e| add(e, level, ty, ElementKind::Beam(Beam::new(a, b, 3.0))))
        }

        fn connect(&mut self, member: ElementId, end: MemberEnd, kind: ConnectionKind, f: impl FnOnce(&mut ConnectionParams)) -> ElementId {
            let (level, ty) = (self.level, self.connection);
            let mut c = Connection::new(member, end, kind);
            f(&mut c.params);
            self.doc.edit("connection", |e| add(e, level, ty, ElementKind::Connection(c)))
        }

        fn geom(&self, id: ElementId) -> (ConnectionGeom, ModelGeometry) {
            let g = ModelGeometry::build(&self.doc.project);
            (g.connections.get(&id).cloned().expect("built"), g)
        }
    }

    fn close(a: DVec3, b: DVec3) -> bool {
        a.distance(b) < 1e-9
    }

    #[test]
    fn a_block_is_closed_and_outward() {
        let b = Block {
            center: DVec3::new(1.0, 2.0, 3.0),
            x: DVec3::new(0.6, 0.8, 0.0),
            y: DVec3::Z,
            size: DVec3::new(0.3, 0.2, 0.01),
        };
        let mut s = Solid::new(ElementId(1), Category::Connection, [0; 3]);
        s.add_polygons(b.faces());
        assert_eq!(s.faces.len(), 6);
        assert!((s.volume() - 0.3 * 0.2 * 0.01).abs() < 1e-12, "{}", s.volume());
    }

    #[test]
    fn a_base_plate_sits_under_its_column() {
        let mut s = site();
        let col = s.column(DVec2::new(2.0, 3.0));
        let id = s.connect(col, MemberEnd::Base, ConnectionKind::BasePlate, |_| {});
        let (g, model) = s.geom(id);
        assert_eq!(g.warning, None);
        let plate = g.plates[0].blocks[0];
        // Section 0.30 x 0.20 plus 2 x 0.05 at the edges, 25 mm thick, its top at the column's base.
        assert!(close(plate.size, DVec3::new(0.4, 0.3, 0.025)), "{:?}", plate.size);
        assert!(close(plate.center, DVec3::new(2.0, 3.0, s.z0() - 0.0125)), "{:?}", plate.center);
        assert_eq!(g.bolt_count(), 4);
        for b in &g.bolts {
            let (x, y) = ((b.shank.a.x - 2.0).abs(), (b.shank.a.y - 3.0).abs());
            assert!((x - 0.15).abs() < 1e-9 && (y - 0.1).abs() < 1e-9, "anchors at the edge distance: {x} {y}");
            assert!((b.shank.a.z - s.z0()).abs() < 1e-9, "from the plate's top");
            assert!((b.shank.length() - (0.025 + 0.4)).abs() < 1e-9);
        }
        assert!((g.plate_mass() - 0.4 * 0.3 * 0.025 * 7850.0).abs() < 1e-9);
        let solids: Vec<&Solid> = model.solids_of(id).collect();
        assert_eq!(solids.len(), 1);
        assert_eq!(solids[0].category, Category::Connection);
        // A given size wins over the section's.
        let other = s.column(DVec2::new(8.0, 0.0));
        let id = s.connect(other, MemberEnd::Base, ConnectionKind::BasePlate, |p| p.width = 0.5);
        assert!((s.geom(id).0.plates[0].blocks[0].size.x - 0.5).abs() < 1e-12);
    }

    #[test]
    fn anchors_come_in_rows_and_columns() {
        let mut s = site();
        let col = s.column(DVec2::ZERO);
        let id = s.connect(col, MemberEnd::Base, ConnectionKind::BasePlate, |p| (p.rows, p.columns) = (3, 4));
        assert_eq!(s.geom(id).0.bolt_count(), 12);
    }

    /// An end plate stands against the beam's end face, its normal along the
    /// beam's axis also when the beam is turned, bolted through along it.
    #[test]
    fn an_end_plate_follows_a_turned_beam() {
        let mut s = site();
        s.column(DVec2::ZERO);
        s.column(DVec2::new(3.0, 4.0));
        let beam = s.beam(DVec2::ZERO, DVec2::new(3.0, 4.0));
        let id = s.connect(beam, MemberEnd::End, ConnectionKind::EndPlate, |_| {});
        let (g, model) = s.geom(id);
        assert!(matches!(model.beams[&beam].end, BeamEnd::Column(_)));
        assert_eq!(g.warning, None);
        let axis = DVec3::new(0.6, 0.8, 0.0);
        let plate = g.plates[0].blocks[0];
        assert!(plate.z().cross(axis).length() < 1e-9, "normal {:?}", plate.z());
        assert!(plate.center.truncate().perp_dot(axis.truncate()).abs() < 1e-9, "on the axis");
        assert!((plate.center.z - (s.z0() + 2.8)).abs() < 1e-9, "mid-height of the beam");
        assert!(close(plate.size, DVec3::new(0.2 + 0.08, 0.4 + 0.08, 0.016)), "{:?}", plate.size);
        assert_eq!(g.bolt_count(), 4);
        for b in &g.bolts {
            assert!((b.shank.b - b.shank.a).cross(axis).length() < 1e-9);
            assert!((b.shank.length() - (0.016 + BOLT_EXTRA)).abs() < 1e-9);
        }
    }

    #[test]
    fn a_fin_plate_is_as_long_as_its_bolts_need() {
        let mut s = site();
        s.column(DVec2::ZERO);
        s.column(DVec2::new(5.0, 0.0));
        let beam = s.beam(DVec2::ZERO, DVec2::new(5.0, 0.0));
        let id = s.connect(beam, MemberEnd::Start, ConnectionKind::FinPlate, |p| p.columns = 3);
        let (g, _) = s.geom(id);
        let plate = g.plates[0].blocks[0];
        // 2 x 0.04 + 2 x 0.07 along the beam, 0.8 x 0.40 high, from the column face at x = 0.15.
        assert!((plate.size.x - 0.22).abs() < 1e-12 && (plate.size.y - 0.32).abs() < 1e-12, "{:?}", plate.size);
        assert!(close(plate.center, DVec3::new(0.15 + 0.11, 0.0, s.z0() + 2.8)), "{:?}", plate.center);
        assert_eq!(g.bolt_count(), 9, "three rows of three");
    }

    #[test]
    fn timber_bolts_cross_the_beam_with_washers() {
        let mut s = site();
        s.column(DVec2::ZERO);
        s.column(DVec2::new(5.0, 0.0));
        let beam = s.beam(DVec2::ZERO, DVec2::new(5.0, 0.0));
        let id = s.connect(beam, MemberEnd::End, ConnectionKind::TimberBolts, |_| {});
        let (g, _) = s.geom(id);
        assert!(g.plates.is_empty() && g.plate_volume() == 0.0);
        assert_eq!(g.bolt_count(), 4);
        for b in &g.bolts {
            assert!((b.shank.length() - (0.2 + 2.0 * 0.005)).abs() < 1e-9, "{}", b.shank.length());
            assert_eq!(b.washers.len(), 2);
            assert!(b.washers.iter().all(|w| (w.diameter - 3.0 * 0.012).abs() < 1e-12));
        }
        // The first column of bolts is the edge distance in from the face at x = 4.85.
        let nearest = g.bolts.iter().map(|b| b.shank.a.x).fold(f64::NEG_INFINITY, f64::max);
        assert!((nearest - (4.85 - 0.08)).abs() < 1e-9, "{nearest}");
    }

    #[test]
    fn a_hanger_is_a_bent_plate_under_the_beam_end() {
        let mut s = site();
        s.beam(DVec2::ZERO, DVec2::new(6.0, 0.0));
        let joist = s.beam(DVec2::new(3.0, 0.0), DVec2::new(3.0, 4.0));
        let id = s.connect(joist, MemberEnd::Start, ConnectionKind::Hanger, |_| {});
        let (g, model) = s.geom(id);
        assert_eq!(model.beams[&joist].start, BeamEnd::Beam);
        assert_eq!(g.warning, None);
        assert_eq!(g.plates.len(), 1);
        let blocks = &g.plates[0].blocks;
        assert_eq!(blocks.len(), 3, "base and two sides");
        let (t, len, inner, side) = (0.002, 0.1, 0.2, HANGER_SIDE * 0.4);
        let bottom = s.z0() + 2.6;
        let top = blocks.iter().map(|b| b.center.z + b.size.y / 2.0).fold(f64::NEG_INFINITY, f64::max);
        assert!((top - (bottom + side)).abs() < 1e-9, "sides up to 0.6 h: {top}");
        assert!((blocks[0].center.z - (bottom - t / 2.0)).abs() < 1e-9, "base under the beam");
        let volume = len * inner * t + 2.0 * len * (side + t) * t;
        assert!((g.plate_volume() - volume).abs() < 1e-12, "{} vs {volume}", g.plate_volume());
    }

    #[test]
    fn a_connection_moves_with_its_column() {
        let mut s = site();
        let col = s.column(DVec2::new(2.0, 3.0));
        let id = s.connect(col, MemberEnd::Base, ConnectionKind::BasePlate, |_| {});
        s.doc.edit("move", |e| e.element_mut(col).unwrap().translate(DVec2::new(1.0, 0.0)));
        let (g, _) = s.geom(id);
        assert!(close(g.plates[0].blocks[0].center, DVec3::new(3.0, 3.0, s.z0() - 0.0125)));
    }

    #[test]
    fn a_missing_member_or_a_free_end_builds_nothing() {
        let mut s = site();
        let lost = s.connect(ElementId(999_999), MemberEnd::Base, ConnectionKind::BasePlate, |_| {});
        s.column(DVec2::ZERO);
        let beam = s.beam(DVec2::ZERO, DVec2::new(5.0, 0.0));
        let free = s.connect(beam, MemberEnd::End, ConnectionKind::EndPlate, |_| {});
        let (g, model) = s.geom(lost);
        assert_eq!(g.warning, Some(ConnectionWarning::MissingMember));
        assert!(g.plates.is_empty() && g.bolts.is_empty());
        assert_eq!(model.solids_of(lost).count(), 0);
        let (g, model) = s.geom(free);
        assert_eq!(g.warning, Some(ConnectionWarning::Unsuitable));
        assert_eq!(model.solids_of(free).count(), 0);
    }
}
```

(`ModelGeometry::solids_of` is the method `query.rs` uses; `model.beams[&beam]` needs `BTreeMap` indexing, which is fine in tests.)

- [ ] **Step 2: Run them to see them fail.** `cargo nextest run -p cc_geom connection`. Expected: compile errors (`Block`, `ConnectionGeom`, `ModelGeometry::connections`, … missing).

- [ ] **Step 3: Write the module** (above the tests):

```rust
//! Steel and timber connections (see [`cc_core::connection`]): the plates
//! and bolts of each, derived from its member's geometry on every build, so
//! they follow the member. Plates are oriented boxes ([`Block`]), bolts and
//! washers cylinders ([`Rod`]). A connection whose member is missing, or
//! whose member end is not framed as its kind needs, builds nothing and
//! carries a [`ConnectionWarning`].

use crate::{BeamEnd, BeamGeom, Solid};
use cc_core::connection::{Connection, ConnectionKind, ConnectionParams, MemberEnd};
use cc_core::{Category, Element, ElementId, ElementKind, Project, TypeData};
use glam::{DVec2, DVec3};
use std::collections::BTreeMap;

/// End and fin plate bolts are the plate's thickness plus this (nut and head), meters.
pub const BOLT_EXTRA: f64 = 0.02;
/// Washer diameter per bolt diameter.
pub const WASHER_RATIO: f64 = 3.0;
/// A fin plate's height per section height.
pub const FIN_HEIGHT: f64 = 0.8;
/// A hanger's sides per beam height.
pub const HANGER_SIDE: f64 = 0.6;
/// Connections drawn without a metal material in the project.
pub const STEEL_COLOR: [u8; 3] = [150, 156, 162];

/// A box: its center, two square unit axes `x` and `y` (the third is
/// `x × y`) and its size along the three.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Block {
    pub center: DVec3,
    pub x: DVec3,
    pub y: DVec3,
    pub size: DVec3,
}

impl Block {
    pub fn z(&self) -> DVec3 {
        self.x.cross(self.y)
    }

    pub fn volume(&self) -> f64 {
        self.size.x * self.size.y * self.size.z
    }

    /// The six faces, wound outward.
    pub fn faces(&self) -> Vec<Vec<DVec3>> {
        let (x, y, z) = (self.x * (self.size.x / 2.0), self.y * (self.size.y / 2.0), self.z() * (self.size.z / 2.0));
        let p = |i: f64, j: f64, k: f64| self.center + x * i + y * j + z * k;
        vec![
            vec![p(-1.0, -1.0, 1.0), p(1.0, -1.0, 1.0), p(1.0, 1.0, 1.0), p(-1.0, 1.0, 1.0)],
            vec![p(-1.0, -1.0, -1.0), p(-1.0, 1.0, -1.0), p(1.0, 1.0, -1.0), p(1.0, -1.0, -1.0)],
            vec![p(1.0, -1.0, -1.0), p(1.0, 1.0, -1.0), p(1.0, 1.0, 1.0), p(1.0, -1.0, 1.0)],
            vec![p(-1.0, -1.0, -1.0), p(-1.0, -1.0, 1.0), p(-1.0, 1.0, 1.0), p(-1.0, 1.0, -1.0)],
            vec![p(-1.0, 1.0, -1.0), p(-1.0, 1.0, 1.0), p(1.0, 1.0, 1.0), p(1.0, 1.0, -1.0)],
            vec![p(-1.0, -1.0, -1.0), p(1.0, -1.0, -1.0), p(1.0, -1.0, 1.0), p(-1.0, -1.0, 1.0)],
        ]
    }
}

/// A cylinder from `a` to `b`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rod {
    pub a: DVec3,
    pub b: DVec3,
    pub diameter: f64,
}

impl Rod {
    pub fn length(&self) -> f64 {
        self.a.distance(self.b)
    }
}

/// A bolt (or anchor bolt) and its washers.
#[derive(Debug, Clone, PartialEq)]
pub struct Bolt {
    pub shank: Rod,
    pub washers: Vec<Rod>,
}

/// A plate: one box, or several for a bent plate (a hanger's base and sides).
#[derive(Debug, Clone, PartialEq)]
pub struct Plate {
    pub blocks: Vec<Block>,
}

impl Plate {
    pub fn volume(&self) -> f64 {
        self.blocks.iter().map(Block::volume).sum()
    }
}

/// Why a connection built nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionWarning {
    /// Its member is gone.
    MissingMember,
    /// Its member end does not take this kind (or the member's section fails).
    Unsuitable,
}

impl ConnectionWarning {
    pub fn code(self) -> &'static str {
        match self {
            ConnectionWarning::MissingMember => "missing_member",
            ConnectionWarning::Unsuitable => "unsuitable",
        }
    }
}

/// The parts of one connection.
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionGeom {
    pub element: ElementId,
    pub member: ElementId,
    pub kind: ConnectionKind,
    pub plates: Vec<Plate>,
    pub bolts: Vec<Bolt>,
    pub bolt_diameter: f64,
    pub warning: Option<ConnectionWarning>,
}

impl ConnectionGeom {
    pub fn plate_volume(&self) -> f64 {
        self.plates.iter().map(Plate::volume).sum()
    }

    /// Kilograms of steel in the plates.
    pub fn plate_mass(&self) -> f64 {
        self.plate_volume() * crate::rebar::STEEL_DENSITY
    }

    pub fn bolt_count(&self) -> usize {
        self.bolts.len()
    }

    /// The length of one bolt (they are alike); 0 with none.
    pub fn bolt_length(&self) -> f64 {
        self.bolts.first().map_or(0.0, |b| b.shank.length())
    }

    /// The faces of every part.
    pub fn faces(&self) -> Vec<Vec<DVec3>> {
        let blocks = self.plates.iter().flat_map(|p| &p.blocks).flat_map(Block::faces);
        let rods = self
            .bolts
            .iter()
            .flat_map(|b| std::iter::once(&b.shank).chain(&b.washers))
            .flat_map(|r| crate::mep::cylinder(r.a, r.b, r.diameter / 2.0));
        blocks.chain(rods).collect()
    }
}

/// How the end `end` of a beam is framed (`None` for a column's base).
pub fn beam_end_join(g: &BeamGeom, end: MemberEnd) -> Option<BeamEnd> {
    match end {
        MemberEnd::Start => Some(g.start),
        MemberEnd::End => Some(g.end),
        MemberEnd::Base => None,
    }
}

/// Whether a beam end framed as `joined` takes a connection of `kind`.
pub fn suits(kind: ConnectionKind, joined: BeamEnd) -> bool {
    match kind {
        ConnectionKind::EndPlate | ConnectionKind::TimberBolts => matches!(joined, BeamEnd::Column(_)),
        ConnectionKind::FinPlate => matches!(joined, BeamEnd::Column(_) | BeamEnd::Beam),
        ConnectionKind::Hanger => joined == BeamEnd::Beam,
        ConnectionKind::BasePlate => false,
    }
}

/// The material connections are drawn in: the project's first metal.
pub fn steel_material(project: &Project) -> Option<&str> {
    project.materials.iter().find(|m| m.identity.class == cc_core::MaterialClass::Metal).map(|m| m.name.as_str())
}

/// An empty solid of connection `element`, in steel.
pub fn solid(project: &Project, element: ElementId) -> Solid {
    match steel_material(project) {
        Some(m) => Solid::of_material(element, Category::Connection, project, m),
        None => Solid::new(element, Category::Connection, STEEL_COLOR),
    }
}

/// `n` positions spread evenly over `[-half, half]` (the middle for one).
fn spread(n: u32, half: f64) -> Vec<f64> {
    match n {
        0 => Vec::new(),
        1 => vec![0.0],
        _ => (0..n).map(|i| -half + 2.0 * half * f64::from(i) / f64::from(n - 1)).collect(),
    }
}

/// `n` positions `spacing` apart, centered on 0.
fn centered(n: u32, spacing: f64) -> Vec<f64> {
    (0..n).map(|i| (f64::from(i) - f64::from(n.saturating_sub(1)) / 2.0) * spacing).collect()
}

fn flat(v: DVec2) -> DVec3 {
    DVec3::new(v.x, v.y, 0.0)
}

/// A column's base: the center of its section's box at the base, the
/// section's axes in plan and the box's size.
struct ColumnFrame {
    base: DVec3,
    x: DVec3,
    y: DVec3,
    size: DVec2,
}

fn column_frame(project: &Project, member: &Element, col: &cc_core::Column) -> Option<ColumnFrame> {
    let ty = project.element_type(member.type_id)?;
    let section = project.type_section(&ty.data)?;
    let (lo, hi) = section.outline.bbox();
    let turn = if col.rotation_deg.is_finite() { col.rotation_deg.to_radians() } else { 0.0 };
    let rot = DVec2::from_angle(turn);
    let center = col.position + rot.rotate((lo + hi) * 0.5);
    let (base, _) = project.column_span(member, col);
    let frame = ColumnFrame { base: center.extend(base), x: flat(rot), y: flat(rot.perp()), size: hi - lo };
    (frame.base.is_finite() && frame.size.is_finite()).then_some(frame)
}

/// A beam's end face: the point on the axis there, the plan direction out
/// of the beam through it, the beam's left, the section's bottom and top
/// elevations at the face, the half width and how the end is framed.
struct EndFrame {
    face: DVec2,
    out: DVec2,
    side: DVec2,
    bottom: f64,
    top: f64,
    half: f64,
    joined: BeamEnd,
}

impl EndFrame {
    fn height(&self) -> f64 {
        self.top - self.bottom
    }

    /// The face's point at mid-height.
    fn middle(&self) -> DVec3 {
        self.face.extend((self.bottom + self.top) * 0.5)
    }
}

fn end_frame(g: &BeamGeom, end: MemberEnd) -> Option<EndFrame> {
    let (u, out) = match end {
        MemberEnd::Start => ((g.start_ext.0 + g.start_ext.1) * 0.5, -g.dir),
        MemberEnd::End => ((g.end_ext.0 + g.end_ext.1) * 0.5, g.dir),
        MemberEnd::Base => return None,
    };
    let (bottom, top) = g.z_at(u);
    let joined = beam_end_join(g, end)?;
    Some(EndFrame { face: g.world(u, 0.0), out, side: g.normal, bottom, top, half: g.half, joined })
}

type Parts = (Vec<Plate>, Vec<Bolt>);

fn base_plate(f: &ColumnFrame, p: &ConnectionParams) -> Parts {
    let w = if p.width > 0.0 { p.width } else { f.size.x + 2.0 * p.edge };
    let l = if p.length > 0.0 { p.length } else { f.size.y + 2.0 * p.edge };
    let t = p.thickness;
    let plate = Block { center: f.base - DVec3::Z * (t / 2.0), x: f.x, y: f.y, size: DVec3::new(w, l, t) };
    let mut bolts = Vec::new();
    for y in spread(p.rows, (l / 2.0 - p.edge).max(0.0)) {
        for x in spread(p.columns, (w / 2.0 - p.edge).max(0.0)) {
            let top = f.base + f.x * x + f.y * y;
            let shank = Rod { a: top, b: top - DVec3::Z * (t + p.anchor_length), diameter: p.bolt_diameter };
            bolts.push(Bolt { shank, washers: Vec::new() });
        }
    }
    (vec![Plate { blocks: vec![plate] }], bolts)
}

fn end_plate(f: &EndFrame, p: &ConnectionParams) -> Parts {
    let (out, side) = (flat(f.out), flat(f.side));
    let w = if p.width > 0.0 { p.width } else { 2.0 * f.half + 2.0 * p.edge };
    let l = if p.length > 0.0 { p.length } else { f.height() + 2.0 * p.edge };
    let t = p.thickness;
    let center = f.middle() + out * (t / 2.0);
    let plate = Block { center, x: side, y: DVec3::Z, size: DVec3::new(w, l, t) };
    let half_bolt = (t + BOLT_EXTRA) / 2.0;
    let mut bolts = Vec::new();
    for z in spread(p.rows, (l / 2.0 - p.edge).max(0.0)) {
        for x in spread(p.columns, (w / 2.0 - p.edge).max(0.0)) {
            let at = center + side * x + DVec3::Z * z;
            let shank = Rod { a: at - out * half_bolt, b: at + out * half_bolt, diameter: p.bolt_diameter };
            bolts.push(Bolt { shank, washers: Vec::new() });
        }
    }
    (vec![Plate { blocks: vec![plate] }], bolts)
}

fn fin_plate(f: &EndFrame, p: &ConnectionParams) -> Parts {
    let (inward, side) = (-flat(f.out), flat(f.side));
    let len = if p.length > 0.0 {
        p.length
    } else {
        2.0 * p.edge + f64::from(p.columns.saturating_sub(1)) * p.spacing
    };
    let h = if p.width > 0.0 { p.width } else { FIN_HEIGHT * f.height() };
    let t = p.thickness;
    let plate = Block { center: f.middle() + inward * (len / 2.0), x: inward, y: DVec3::Z, size: DVec3::new(len, h, t) };
    let half_bolt = (t + BOLT_EXTRA) / 2.0;
    let mut bolts = Vec::new();
    for i in 0..p.columns {
        let u = p.edge + f64::from(i) * p.spacing;
        for z in centered(p.rows, p.spacing) {
            let at = f.middle() + inward * u + DVec3::Z * z;
            let shank = Rod { a: at - side * half_bolt, b: at + side * half_bolt, diameter: p.bolt_diameter };
            bolts.push(Bolt { shank, washers: Vec::new() });
        }
    }
    (vec![Plate { blocks: vec![plate] }], bolts)
}

fn timber_bolts(f: &EndFrame, p: &ConnectionParams) -> Parts {
    let (inward, side) = (-flat(f.out), flat(f.side));
    let tw = p.thickness;
    let washer = WASHER_RATIO * p.bolt_diameter;
    let mut bolts = Vec::new();
    for i in 0..p.columns {
        let u = p.edge + f64::from(i) * p.spacing;
        for z in centered(p.rows, p.spacing) {
            let at = f.middle() + inward * u + DVec3::Z * z;
            let reach = f.half + tw;
            let shank = Rod { a: at - side * reach, b: at + side * reach, diameter: p.bolt_diameter };
            let washers = [1.0, -1.0]
                .map(|s| Rod { a: at + side * (s * f.half), b: at + side * (s * reach), diameter: washer })
                .to_vec();
            bolts.push(Bolt { shank, washers });
        }
    }
    (Vec::new(), bolts)
}

fn hanger(f: &EndFrame, p: &ConnectionParams) -> Parts {
    let (inward, side) = (-flat(f.out), flat(f.side));
    let t = p.thickness;
    let len = if p.length > 0.0 { p.length } else { p.edge };
    let inner = if p.width > 0.0 { p.width } else { 2.0 * f.half };
    let side_h = HANGER_SIDE * f.height();
    let mid = flat(f.face) + inward * (len / 2.0);
    // The base under the beam across the inside width; the sides from the
    // base's bottom up, beside it (no overlap).
    let mut blocks = vec![Block {
        center: mid + DVec3::Z * (f.bottom - t / 2.0),
        x: inward,
        y: side,
        size: DVec3::new(len, inner, t),
    }];
    for s in [-1.0, 1.0] {
        blocks.push(Block {
            center: mid + side * (s * (inner + t) / 2.0) + DVec3::Z * (f.bottom - t + (side_h + t) / 2.0),
            x: inward,
            y: DVec3::Z,
            size: DVec3::new(len, side_h + t, t),
        });
    }
    let reach = inner / 2.0 + t + BOLT_EXTRA / 2.0;
    let mut bolts = Vec::new();
    for z in spread(p.rows, side_h * 0.25) {
        for u in spread(p.columns, len * 0.25) {
            let at = mid + inward * u + DVec3::Z * (f.bottom + side_h / 2.0 + z);
            let shank = Rod { a: at - side * reach, b: at + side * reach, diameter: p.bolt_diameter };
            bolts.push(Bolt { shank, washers: Vec::new() });
        }
    }
    (vec![Plate { blocks }], bolts)
}

/// The parts of a beam end connection, `None` when the end does not suit it.
fn beam_parts(kind: ConnectionKind, f: &EndFrame, p: &ConnectionParams) -> Option<Parts> {
    if !suits(kind, f.joined) {
        return None;
    }
    match kind {
        ConnectionKind::EndPlate => Some(end_plate(f, p)),
        ConnectionKind::FinPlate => Some(fin_plate(f, p)),
        ConnectionKind::TimberBolts => Some(timber_bolts(f, p)),
        ConnectionKind::Hanger => Some(hanger(f, p)),
        ConnectionKind::BasePlate => None,
    }
}

/// Connection `id` as `c` and its member make it.
pub fn connection_geom(
    project: &Project,
    beams: &BTreeMap<ElementId, BeamGeom>,
    id: ElementId,
    c: &Connection,
) -> ConnectionGeom {
    let mut g = ConnectionGeom {
        element: id,
        member: c.member,
        kind: c.kind,
        plates: Vec::new(),
        bolts: Vec::new(),
        bolt_diameter: c.params.bolt_diameter,
        warning: None,
    };
    let Some(member) = project.element(c.member) else {
        g.warning = Some(ConnectionWarning::MissingMember);
        return g;
    };
    let p = &c.params;
    let built = match &member.kind {
        ElementKind::Column(col) if c.kind.at_column() && c.end == MemberEnd::Base => {
            column_frame(project, member, col).map(|f| base_plate(&f, p))
        }
        ElementKind::Beam(_) if !c.kind.at_column() => {
            beams.get(&c.member).and_then(|b| end_frame(b, c.end)).and_then(|f| beam_parts(c.kind, &f, p))
        }
        _ => None,
    };
    match built {
        Some((plates, bolts)) => (g.plates, g.bolts) = (plates, bolts),
        None => g.warning = Some(ConnectionWarning::Unsuitable),
    }
    g
}

/// Every connection of the project, from the resolved beams.
pub fn build_connections(project: &Project, beams: &BTreeMap<ElementId, BeamGeom>) -> BTreeMap<ElementId, ConnectionGeom> {
    project
        .elements
        .values()
        .filter_map(|e| match &e.kind {
            ElementKind::Connection(c) => Some((e.id, connection_geom(project, beams, e.id, c))),
            _ => None,
        })
        .collect()
}
```

(`TypeData` is only used by the tests: move it into the test module's `use` if clippy reports it unused.)

- [ ] **Step 4: Hook it into the build.** In `crates/cc_geom/src/build.rs`, add the field (Files) and, after `geom.rebar = crate::rebar::build_rebar(project, &geom.beams);`, `geom.build_connections(project);`, with the method next to `build_masses`:

```rust
    /// Connections: the plates and bolts of each from its member (after the
    /// beams' ends are resolved), one steel solid per connection.
    fn build_connections(&mut self, project: &Project) {
        self.connections = crate::connection::build_connections(project, &self.beams);
        for g in self.connections.values() {
            let faces = g.faces();
            if faces.is_empty() {
                continue;
            }
            let mut s = crate::connection::solid(project, g.element);
            s.add_polygons(faces);
            self.solids.push(s);
        }
    }
```

- [ ] **Step 5: Run the geometry tests.** `cargo nextest run -p cc_geom connection`. Expected: pass. If `an_end_plate_follows_a_turned_beam` reads `BeamEnd::Free`, the column at (3, 4) is too small to hold the beam's end: keep the test and check `beam_between_columns_stops_at_their_faces` in `beam.rs` for how the end must lie inside the outline (it must: the beam ends at the column's center). If the hanger's joist start reads `Free`, compare with `a_beam_ending_on_another_stops_at_its_side` (it uses the same T).

- [ ] **Step 6: Plans.** Write the failing test in `crates/cc_views/src/plan.rs` `mod tests`, next to `trusses_draw_their_span_lines`:

```rust
    /// A connection draws its plate and bolts where its member stands.
    #[test]
    fn connections_draw_on_their_members_level() {
        use cc_core::connection::{Connection, ConnectionKind, MemberEnd};
        let mut doc = Document::new(Project::from_template());
        let level = doc.project.levels[0].id;
        let element = move |id, type_id, kind| Element {
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
        };
        let conn = doc.edit("t", |e| {
            let column_t = e.project().default_type(Category::Column).unwrap();
            let ty = cc_core::ElementTypeId(e.alloc_id());
            e.add_type(cc_core::connection::host_type(ty, "Connection".into()));
            let col = ElementId(e.alloc_id());
            let column = cc_core::Column {
                position: glam::DVec2::new(1.0, 1.0),
                rotation_deg: 0.0,
                base_offset: 0.0,
                height: 3.0,
                top: cc_core::WallTop::Unconnected,
                structural: true,
            };
            e.add_element(element(col, column_t, ElementKind::Column(column)));
            let id = ElementId(e.alloc_id());
            let c = Connection::new(col, MemberEnd::Base, ConnectionKind::BasePlate);
            e.add_element(element(id, ty, ElementKind::Connection(c)))
        });
        let geom = ModelGeometry::build(&doc.project);
        let d = plan(&doc.project, &geom, level, 1.2, false, 100);
        let lines =
            d.prims.iter().filter(|p| matches!(p, Prim::Line { element: Some(e), .. } if *e == conn)).count();
        assert!(lines >= 4, "the plate's outline at least: {lines}");
    }
```

Run `cargo nextest run -p cc_views connections_draw` (FAILS: 0 lines). Then add to `plan.rs`:

```rust
/// Connections whose member stands on `level`: their plates and bolts as
/// they cut and show (see [`draw_mesh`]).
fn draw_connections(d: &mut Drawing2D, project: &Project, geom: &ModelGeometry, level: LevelId, cut_z: f64) {
    for g in geom.connections.values() {
        if project.element(g.member).is_some_and(|m| m.level == level) {
            draw_mesh(d, &g.faces(), cut_z, false, g.element);
        }
    }
}
```

and in `plan_of`:

```rust
    if !ceiling {
        draw_structure(&mut d, project, geom, level, cut_z, spacing, structural);
        draw_connections(&mut d, project, geom, level, cut_z);
    }
```

plus `| ElementKind::Connection(_)` in `is_structural`'s `true` arm. (If `plan()`'s signature in the base differs from the truss test's, copy the call the neighboring tests make.)

- [ ] **Step 7: Run the tests.** `cargo nextest run -p cc_geom connection beam rebar` and `cargo nextest run -p cc_views plan`. Expected: pass.

- [ ] **Step 8: Clippy, fmt, commit.** `cargo clippy -p cc_geom -p cc_views --all-targets -- -D warnings`, `cargo fmt --all`, then:

```bash
git add -A
git commit -s -m "Build connection plates and bolts from their members"
```

---

### Task 3: `connections_create`, element parameters and schedules (cc_engine, cc_core, cc_views)

**Files:**
- Create: `crates/cc_engine/src/commands/connection.rs`, `crates/cc_engine/tests/connections.rs`
- Modify:
  - `crates/cc_engine/src/commands/mod.rs`: `pub mod connection;` after `pub mod component_script;`.
  - `crates/cc_engine/src/registry.rs`: `connection::ConnectionsCreate,` right after `rebar::RebarTypical,`.
  - `crates/cc_engine/src/commands/edit.rs` (`patched`, the `match &kind` before `_ => {}`): the connection arm (Step 4).
  - `crates/cc_engine/src/commands/query.rs` (`element_json`, after the `geom.valves` block): `if let Some(c) = geom.connections.get(&id) { super::connection::derived(c, &mut derived); }`.
  - `crates/cc_engine/tests/main.rs`: `mod connections;` (alphabetical, after `mod component_script;`).
  - `crates/cc_core/src/schedule.rs`: four fields, their keys, `available`/`defaults` for `Category::Connection` (Step 5).
  - `crates/cc_views/src/schedule.rs`: `summable`, `value_cached` arms, `Host` arm, `field_value` (Step 6).
  - `docs/commands.md`: regenerated.
  - `assets/i18n/*.ftl` (12): the Task 3 keys (`col-connection-kind`, `col-plate-mass`, `col-bolt-count`, `col-bolt-diameter`, `tx-create-connections`).
- Test: `crates/cc_engine/tests/connections.rs`.

**Interfaces:**
- Consumes: Task 1 (`Connection::new`, `ConnectionKind::{from_code, defaults, at_column}`, `ConnectionParams::check`, `Project::connection_at`, `host_type`), Task 2 (`ModelGeometry::connections`, `ConnectionGeom::{plate_volume, plate_mass, bolt_count, bolt_length}`, `beam_end_join`, `suits`, `ConnectionWarning::code`); engine helpers `edit`, `add_element`, `ids_json`, `patch_value`, `refs::{element, level}`, `Session::{active_level, project_and_geometry}`.
- Produces:
  - `pub struct ConnectionsCreate { kind: String, members: Vec<u64>, level: Option<Ref>, params: Option<Value> }` (`Default`, `Serialize`), command `connections_create` returning `{"created": [ids], "skipped": n}`.
  - `pub fn new_connection_type(id: ElementTypeId) -> ElementType`; `pub(crate) fn params_arg(kind, &Value) -> Result<ConnectionParams>`; `pub(crate) fn check(old: &Connection, new: &Connection) -> Result<()>`; `pub(crate) fn derived(g: &ConnectionGeom, out: &mut Map<String, Value>)` (keys `plate_volume`, `plate_mass`, `bolt_count`, `bolt_diameter`, `bolt_length`, `warning`).
  - `ScheduleField::{ConnectionKind, PlateMass, BoltCount, BoltDiameter}` (keys `col-connection-kind`, `col-plate-mass`, `col-bolt-count`, `col-bolt-diameter`).

- [ ] **Step 1: Write the failing engine tests** in `crates/cc_engine/tests/connections.rs` (and its `mod` line):

```rust
//! Steel and timber connections: placed on member ends by
//! `connections_create`, edited through `element_set`, removed with their
//! members and scheduled by plate mass and bolts.

use cc_core::connection::MemberEnd;
use cc_core::{Category, ElementId, ElementKind, ScheduleField};
use cc_engine::Session;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn run(s: &mut Session, name: &str, args: Value) -> Value {
    s.execute(name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().unwrap()
}

fn created(v: &Value) -> Vec<u64> {
    v["created"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap()).collect()
}

/// Two columns 5 m apart with a beam between them (`main`), a beam from
/// the right column out to (5, 4) (`out`, its far end free) and one from
/// the middle of `main` to (2.5, -3) (`tee`, its start on `main`).
fn frame(s: &mut Session) -> ([u64; 2], u64, u64, u64) {
    let a = id(&run(s, "column_create", json!({"position": [0, 0]})));
    let b = id(&run(s, "column_create", json!({"position": [5, 0]})));
    let main = id(&run(s, "beam_create", json!({"start": [0, 0], "end": [5, 0]})));
    let out = id(&run(s, "beam_create", json!({"start": [5, 0], "end": [5, 4]})));
    let tee = id(&run(s, "beam_create", json!({"start": [2.5, 0], "end": [2.5, -3]})));
    ([a, b], main, out, tee)
}

/// The member and end of every connection.
fn placed(s: &Session) -> BTreeSet<(u64, MemberEnd)> {
    s.project()
        .elements
        .values()
        .filter_map(|e| match &e.kind {
            ElementKind::Connection(c) => Some((c.member.0, c.end)),
            _ => None,
        })
        .collect()
}

#[test]
fn base_plates_go_on_every_column_once() {
    let mut s = Session::default();
    let (cols, ..) = frame(&mut s);
    let columns = s.project().elements_of(Category::Column).count();
    let depth = s.doc.undo_depth();
    let first = run(&mut s, "connections_create", json!({"kind": "base_plate"}));
    assert_eq!(created(&first).len(), columns);
    assert_eq!(first["skipped"], json!(0));
    assert_eq!(s.doc.undo_depth(), depth + 1, "one undo step");
    assert!(cols.iter().all(|c| placed(&s).contains(&(*c, MemberEnd::Base))));
    assert_eq!(s.project().types_of(Category::Connection).count(), 1, "the type comes with the first");
    let again = run(&mut s, "connections_create", json!({"kind": "base_plate"}));
    assert!(created(&again).is_empty());
    assert_eq!(again["skipped"], json!(columns));
    assert_eq!(s.doc.undo_depth(), depth + 1, "nothing to do is no undo step");
    run(&mut s, "undo", json!({}));
    assert!(placed(&s).is_empty());
    assert_eq!(s.project().types_of(Category::Connection).count(), 0, "undone with them");
}

#[test]
fn end_plates_go_only_on_beam_ends_at_columns() {
    let mut s = Session::default();
    let (_, main, out, _) = frame(&mut s);
    let r = run(&mut s, "connections_create", json!({"kind": "end_plate"}));
    assert_eq!(created(&r).len(), 3);
    let want = BTreeSet::from([(main, MemberEnd::Start), (main, MemberEnd::End), (out, MemberEnd::Start)]);
    assert_eq!(placed(&s), want);
}

#[test]
fn each_kind_takes_the_ends_it_suits() {
    for (kind, want) in [("fin_plate", 4), ("timber_bolts", 3), ("hanger", 1)] {
        let mut s = Session::default();
        let (_, _, _, tee) = frame(&mut s);
        let r = run(&mut s, "connections_create", json!({"kind": kind}));
        assert_eq!(created(&r).len(), want, "{kind}");
        if kind == "hanger" {
            assert_eq!(placed(&s), BTreeSet::from([(tee, MemberEnd::Start)]));
        }
    }
}

#[test]
fn bad_arguments_are_refused() {
    let mut s = Session::default();
    let (cols, main, ..) = frame(&mut s);
    for args in [
        json!({"kind": "weld"}),
        json!({"kind": "base_plate", "params": {"rows": 0}}),
        json!({"kind": "base_plate", "params": {"bolt_diameter": 3.0}}),
        json!({"kind": "base_plate", "params": {"bolts": 2}}),
        json!({"kind": "base_plate", "members": [main]}),
        json!({"kind": "end_plate", "members": [cols[0]]}),
    ] {
        assert!(s.execute("connections_create", args.clone()).is_err(), "{args}");
    }
    assert!(placed(&s).is_empty());
    let r = run(&mut s, "connections_create", json!({"kind": "base_plate", "members": [cols[0]], "params": {"rows": 3}}));
    let c = created(&r)[0];
    assert_eq!(run(&mut s, "element_get", json!({"id": c}))["derived"]["bolt_count"], json!(6));
}

#[test]
fn element_set_refuses_bad_parameters() {
    let mut s = Session::default();
    let (cols, ..) = frame(&mut s);
    let c = created(&run(&mut s, "connections_create", json!({"kind": "base_plate", "members": [cols[0]]})))[0];
    let params = &run(&mut s, "element_get", json!({"id": c}))["params"];
    assert_eq!(params["kind"], json!("BasePlate"));
    assert_eq!(params["params"]["rows"], json!(2));
    let depth = s.doc.undo_depth();
    for patch in [
        json!({"params": {"rows": 0}}),
        json!({"params": {"columns": 11}}),
        json!({"params": {"bolt_diameter": 3.0}}),
        json!({"params": {"width": 0.0005}}),
        json!({"params": {"thickness": -0.01}}),
        json!({"member": cols[1]}),
        json!({"end": "Start"}),
        json!({"kind": "EndPlate"}),
    ] {
        assert!(s.execute("element_set", json!({"id": c, "params": patch})).is_err(), "{patch}");
    }
    assert_eq!(s.doc.undo_depth(), depth);
    run(&mut s, "element_set", json!({"id": c, "params": {"params": {"columns": 3, "width": 0.6}}}));
    let d = &run(&mut s, "element_get", json!({"id": c}))["derived"];
    assert_eq!(d["bolt_count"], json!(6));
}

#[test]
fn deleting_a_column_removes_its_base_plate() {
    let mut s = Session::default();
    let (cols, ..) = frame(&mut s);
    let c = created(&run(&mut s, "connections_create", json!({"kind": "base_plate", "members": [cols[0]]})))[0];
    run(&mut s, "elements_delete", json!({"ids": [cols[0]]}));
    assert!(s.project().element(ElementId(c)).is_none());
}

#[test]
fn repair_drops_a_second_connection_on_one_end() {
    let mut s = Session::default();
    let (cols, ..) = frame(&mut s);
    let c = created(&run(&mut s, "connections_create", json!({"kind": "base_plate", "members": [cols[0]]})))[0];
    let mut p = s.project().clone();
    let mut dup = p.element(ElementId(c)).unwrap().clone();
    dup.id = ElementId(p.alloc_id());
    dup.uid = cc_core::Uid::generate();
    p.elements.insert(dup.id, dup);
    assert_eq!(p.repair(), 1);
    assert_eq!(p.elements_of(Category::Connection).count(), 1);
    assert!(p.element(ElementId(c)).is_some(), "the first one stays");
}

/// `derived` and the schedule fields agree: plate mass is volume x 7850.
#[test]
fn schedules_weigh_the_plates() {
    let mut s = Session::default();
    let (cols, ..) = frame(&mut s);
    let c = created(&run(&mut s, "connections_create", json!({"kind": "base_plate", "members": [cols[0]]})))[0];
    let d = run(&mut s, "element_get", json!({"id": c}))["derived"].clone();
    let volume = d["plate_volume"].as_f64().unwrap();
    assert!(volume > 0.0);
    assert!((d["plate_mass"].as_f64().unwrap() - volume * 7850.0).abs() < 1e-9);
    assert_eq!((d["bolt_count"].clone(), d["bolt_diameter"].clone()), (json!(4), json!(0.02)));
    assert_eq!(d["warning"], Value::Null);
    let (p, geom) = s.project_and_geometry();
    let e = p.element(ElementId(c)).unwrap();
    let number = |f| cc_views::field_value(p, geom, e, f).and_then(|(_, n)| n);
    assert!((number(ScheduleField::PlateMass).unwrap() - volume * 7850.0).abs() < 1e-6);
    assert_eq!(number(ScheduleField::BoltCount), Some(4.0));
    assert_eq!(number(ScheduleField::BoltDiameter), Some(0.02));
    let column = p.element(ElementId(cols[1])).unwrap();
    assert!(cc_views::field_value(p, geom, column, ScheduleField::PlateMass).is_none(), "connections only");
    let view = run(&mut s, "view_create", json!({"kind": "schedule", "category": "connection"}));
    let table = run(&mut s, "schedule_table", json!({"view": view["id"]}));
    assert_eq!(table["rows"].as_array().unwrap().len(), 1);
}
```

(If the tee's start reads `free` in `element_get`'s `derived.start_join`, the template beam type's join with `main` failed: check `beam_create`'s defaults (`vertical`, `top_offset`) match `main`'s; both default to the level height, so they share a depth.)

- [ ] **Step 2: Run them to see them fail.** `cargo nextest run -p cc_engine --test main connections`. Expected: compile errors (`ScheduleField::PlateMass`, …) or `unknown command connections_create`.

- [ ] **Step 3: Write `commands/connection.rs`:**

```rust
//! Steel and timber connections (see `cc_core::connection`): placing them
//! on member ends, the checks `element_set` runs on them and the values
//! `element_get` derives.

use super::{add_element, edit, ids_json};
use crate::error::{EngineError, Result};
use crate::refs::{self, Ref};
use crate::registry::{Command, CommandKind};
use crate::session::Session;
use crate::t;
use cc_core::connection::{self as cn, Connection, ConnectionKind, ConnectionParams, MemberEnd};
use cc_core::{Category, ElementId, ElementKind, ElementType, ElementTypeId, LevelId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The type a project gets with its first connection.
pub fn new_connection_type(id: ElementTypeId) -> ElementType {
    cn::host_type(id, t!("tpl-type-connection"))
}

fn kind_arg(code: &str) -> Result<ConnectionKind> {
    ConnectionKind::from_code(code).ok_or_else(|| {
        let codes: Vec<&str> = ConnectionKind::ALL.iter().map(|k| k.code()).collect();
        EngineError::invalid(format!("unknown kind '{code}' ({})", codes.join(", ")))
    })
}

/// `kind`'s defaults with `patch` merged in, every name known and every value in range.
pub(crate) fn params_arg(kind: ConnectionKind, patch: &Value) -> Result<ConnectionParams> {
    let defaults = kind.defaults();
    let known = serde_json::to_value(defaults).map_err(|e| EngineError::invalid(e.to_string()))?;
    let (Value::Object(given), Value::Object(names)) = (patch, &known) else {
        return Err(EngineError::invalid("params must be an object"));
    };
    if let Some(bad) = given.keys().find(|k| !names.contains_key(*k)) {
        let all: Vec<&String> = names.keys().collect();
        return Err(EngineError::invalid(format!("unknown parameter '{bad}' ({all:?})")));
    }
    let params: ConnectionParams = super::patch_value(&defaults, patch)?;
    params.check().map_err(EngineError::invalid)?;
    Ok(params)
}

/// Place steel or timber connections of one `kind` at every member end that
/// suits it: `base_plate` under columns; `end_plate` and `timber_bolts` at
/// beam ends that stop at a column; `fin_plate` at beam ends at a column or
/// another beam; `hanger` at beam ends at another beam. Ends that already
/// have a connection are skipped. Plates and bolts follow their member.
/// One undo step; returns the new ids (`created`) and how many ends were
/// `skipped`.
#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct ConnectionsCreate {
    /// `base_plate`, `end_plate`, `fin_plate`, `timber_bolts` or `hanger`.
    pub kind: String,
    /// Column ids (base plates) or beam ids (the other kinds); default: every one of `level`.
    #[serde(default)]
    pub members: Vec<u64>,
    /// Level id or name when no `members` are given (default: the active level, else every level).
    #[serde(default)]
    pub level: Option<Ref>,
    /// Sizes to change from the kind's defaults, meters: `thickness`, `width` and `length` (0: from the
    /// member's section plus `edge`), `bolt_diameter`, `rows` and `columns` (1 to 10), `spacing`, `edge`,
    /// `anchor_length`. Lengths are 0.001 to 2.
    #[serde(default)]
    pub params: Option<Value>,
}

impl Command for ConnectionsCreate {
    const NAME: &'static str = "connections_create";
    const KIND: CommandKind = CommandKind::Edit;
    fn run(self, s: &mut Session) -> Result<Value> {
        let kind = kind_arg(&self.kind)?;
        let params = match &self.params {
            Some(v) => params_arg(kind, v)?,
            None => kind.defaults(),
        };
        let wanted = if kind.at_column() { Category::Column } else { Category::Beam };
        let p = s.project();
        let mut members: Vec<ElementId> = if self.members.is_empty() {
            let level = match &self.level {
                Some(r) => Some(refs::level(p, r)?),
                None => s.active_level(),
            };
            p.elements_of(wanted).filter(|e| level.is_none_or(|l| e.level == l)).map(|e| e.id).collect()
        } else {
            let mut out = Vec::new();
            for &raw in &self.members {
                let id = refs::element(p, raw)?;
                if p.element(id).map(|e| e.category()) != Some(wanted) {
                    let what = if kind.at_column() { "column" } else { "beam" };
                    return Err(EngineError::invalid(format!("element {raw} is not a {what}")));
                }
                out.push(id);
            }
            out
        };
        members.sort();
        members.dedup();
        let (p, geom) = s.project_and_geometry();
        let mut todo: Vec<(LevelId, Connection)> = Vec::new();
        let mut skipped = 0usize;
        for id in members {
            let Some(e) = p.element(id) else { continue };
            let ends: Vec<MemberEnd> = if kind.at_column() {
                vec![MemberEnd::Base]
            } else {
                let Some(g) = geom.beams.get(&id) else { continue };
                [MemberEnd::Start, MemberEnd::End]
                    .into_iter()
                    .filter(|end| {
                        cc_geom::connection::beam_end_join(g, *end).is_some_and(|j| cc_geom::connection::suits(kind, j))
                    })
                    .collect()
            };
            for end in ends {
                if p.connection_at(id, end).is_some() {
                    skipped += 1;
                    continue;
                }
                let mut c = Connection::new(id, end, kind);
                c.params = params;
                todo.push((e.level, c));
            }
        }
        if todo.is_empty() {
            return Ok(json!({"created": [], "skipped": skipped}));
        }
        let host_type = p.default_type(Category::Connection);
        let created = edit(s, t!("tx-create-connections"), |e| {
            let type_id = match host_type {
                Some(t) => t,
                None => {
                    let id = ElementTypeId(e.alloc_id());
                    e.add_type(new_connection_type(id))
                }
            };
            Ok(todo.into_iter().map(|(level, c)| add_element(e, level, type_id, ElementKind::Connection(c))).collect::<Vec<_>>())
        })?;
        Ok(json!({"created": ids_json(&created), "skipped": skipped}))
    }
}

/// `element_set` on a connection: it keeps its member, end and kind, and
/// its sizes stay in range.
pub(crate) fn check(old: &Connection, new: &Connection) -> Result<()> {
    if (new.member, new.end, new.kind) != (old.member, old.end, old.kind) {
        return Err(EngineError::invalid(
            "a connection keeps its member, end and kind: delete it and place another",
        ));
    }
    new.params.check().map_err(EngineError::invalid)
}

/// `element_get`'s derived values of a connection.
pub(crate) fn derived(g: &cc_geom::connection::ConnectionGeom, out: &mut serde_json::Map<String, Value>) {
    out.insert("plate_volume".into(), json!(g.plate_volume()));
    out.insert("plate_mass".into(), json!(g.plate_mass()));
    out.insert("bolt_count".into(), json!(g.bolt_count()));
    out.insert("bolt_diameter".into(), json!(g.bolt_diameter));
    out.insert("bolt_length".into(), json!(g.bolt_length()));
    out.insert("warning".into(), json!(g.warning.map(|w| w.code())));
}
```

Borrow note: `p` (from `s.project()`) ends before `s.project_and_geometry()`; `s.active_level()` takes `&self` and coexists with `p`. If the borrow checker objects to `s.active_level()` while `p` lives, read `let active = s.active_level();` before `let p = s.project();`. `host_type` is `Copy` and `todo` is owned, so `edit(s, ..)` borrows `s` only after the last use of `p` and `geom`.

- [ ] **Step 4: `element_set`.** In `commands/edit.rs` `patched`, add before `_ => {}`:

```rust
        cc_core::ElementKind::Connection(c) => {
            if let cc_core::ElementKind::Connection(old) = &element.kind {
                super::connection::check(old, c)?;
            }
        }
```

And in `query.rs` add the `derived` call (Files).

- [ ] **Step 5: Schedule fields (cc_core).** In `crates/cc_core/src/schedule.rs`, append to `ScheduleField` (after `CableLength`):

```rust
    /// Kind of a connection (base plate, end plate…).
    ConnectionKind,
    /// Steel mass of a connection's plates, kilograms.
    PlateMass,
    /// Number of bolts of a connection.
    BoltCount,
    /// Bolt diameter of a connection.
    BoltDiameter,
```

`key`: `ScheduleField::ConnectionKind => "col-connection-kind",`, `ScheduleField::PlateMass => "col-plate-mass",`, `ScheduleField::BoltCount => "col-bolt-count",`, `ScheduleField::BoltDiameter => "col-bolt-diameter",`. `available`: `Category::Connection => &[F::ConnectionKind, F::Host, F::PlateMass, F::BoltCount, F::BoltDiameter, F::Volume],`. `defaults`: `Category::Connection => vec![F::Mark, F::ConnectionKind, F::PlateMass, F::BoltCount, F::BoltDiameter, F::Comments],`.

- [ ] **Step 6: Schedule values (cc_views).** In `crates/cc_views/src/schedule.rs`:
  - `summable`: add `| ScheduleField::PlateMass | ScheduleField::BoltCount`.
  - `F::Host`: in the inner `match &e.kind`, add `ElementKind::Connection(c) => Some(c.member),`.
  - `value_cached`, before `F::Number`:

```rust
        F::ConnectionKind => match &e.kind {
            ElementKind::Connection(c) => Value::Key(c.kind.key()),
            _ => Value::Missing,
        },
        F::PlateMass => geom.connections.get(&e.id).map_or(Value::Missing, |g| Value::Weight(g.plate_mass())),
        F::BoltCount => geom.connections.get(&e.id).map_or(Value::Missing, |g| Value::Count(g.bolt_count())),
        F::BoltDiameter => geom.connections.get(&e.id).map_or(Value::Missing, |g| {
            if format::imperial() {
                Value::Text(format!("Ø {}\"", format::inches(g.bolt_diameter)))
            } else {
                Value::Text(format!("Ø {} mm", format::number(g.bolt_diameter * 1000.0, 0)))
            }
        }),
```

  - `field_value`, in the `Value::Text(t)` branch's `match`, before the catch-all `_ =>` arm: `(ScheduleField::BoltDiameter, _) => geom.connections.get(&e.id).map(|g| g.bolt_diameter),`.

- [ ] **Step 7: Register, i18n, reference.** Add `connection::ConnectionsCreate,` to the registry; the Task 3 keys of the table to the 12 `.ftl` files under `# Connections`; then `cargo run -q -p cc_cli -- reference > docs/commands.md`.

- [ ] **Step 8: Run the tests.** `cargo nextest run -p cc_engine --test main connections`, `cargo nextest run -p cc_engine reference i18n`, `cargo nextest run -p cc_core schedule`, `cargo nextest run -p cc_views schedule`. Expected: pass (the schedule tests check that every value lies on a field that applies to its category and that defaults are available).

- [ ] **Step 9: Clippy, fmt, commit.** `cargo clippy -p cc_core -p cc_views -p cc_engine --all-targets -- -D warnings`, `cargo fmt --all`, then:

```bash
git add -A
git commit -s -m "Add connections_create, connection parameters and schedule fields"
```

---

### Task 4: IFC export (cc_ifc, CI)

**Files:**
- Create: `crates/cc_ifc/tests/connections.rs`, `crates/cc_ifc/tests/fixtures/connections.ccad` (written by the ignored test)
- Modify:
  - `crates/cc_ifc/src/export.rs`: `write_connections`, `block_solid`, `rod_solid`, `connection_part_type` (Step 3); `write_all` calls `self.write_connections();` right after `self.write_rebar();` (columns and beams are written before, so their products exist).
  - `crates/cc_ifc/tests/main.rs`: `mod connections;` before `mod rebar;`.
  - `.github/workflows/ci.yml`: export and validate the fixture (Step 6).
  - `scripts/validate_ifc.py`: `"IfcPlate", "IfcMechanicalFastener"` appended to `ELEMENTS`.
- Test: `crates/cc_ifc/tests/connections.rs`.

**Interfaces:**
- Consumes: Task 1 (`Connection::new`, `host_type`), Task 2 (`ModelGeometry::connections`, `ConnectionGeom`, `Block::{z, size, center, x}`, `Rod::{a, b, diameter, length}`, `steel_material`); exporter helpers `storey_placement`, `local_placement`, `placement3`, `guid`, `guid_of`, `register`, `products`, `member_types`, `material_style`, `styled`, `shape`, `material`, `associate_material`, `aggregate`, `opt_str`, `Frame`.
- Produces: per connection `IFCELEMENTASSEMBLY` (`ObjectType` = kind code, `AssemblyPlace` `NOTDEFINED`, `USERDEFINED`), `IFCPLATE` per plate (`NOTDEFINED`, typed by `IFCPLATETYPE` "<type> - Plate"), `IFCMECHANICALFASTENER` per bolt (`NominalDiameter`, `NominalLength`, `ANCHORBOLT` or `BOLT`, typed by `IFCMECHANICALFASTENERTYPE` "<type> - Bolt"/"Anchor Bolt"), `IFCRELAGGREGATES`, `IFCRELCONNECTSELEMENTS` (member → assembly).

- [ ] **Step 1: Write the failing tests** in `crates/cc_ifc/tests/connections.rs`:

```rust
//! Connections export as element assemblies of plates and mechanical
//! fasteners, tied to their members; importing them keeps the existing
//! fallback (no native round trip).

use cc_core::connection::{Connection, ConnectionKind, MemberEnd};
use cc_core::{
    Beam, Category, Column, Document, EditCtx, Element, ElementId, ElementKind, ElementTypeId, LevelId, Project,
    WallTop,
};
use cc_ifc::{ExportOptions, export, import};
use glam::DVec2;

fn add(e: &mut EditCtx, level: LevelId, type_id: ElementTypeId, kind: ElementKind) -> ElementId {
    let id = ElementId(e.alloc_id());
    e.add_element(Element {
        id,
        level,
        type_id,
        mark: String::new(),
        comments: String::new(),
        kind,
        uid: Default::default(),
        phase_created: Default::default(),
        phase_demolished: None,
        design_option: None,
        paints: Vec::new(),
    })
}

/// Two columns 5 m apart with a beam between them and a beam from its
/// middle: base plates under the columns, end plates at the first beam's
/// ends and a hanger under the second beam's start.
pub(crate) fn connections_project() -> Project {
    let mut doc = Document::new(Project::from_template());
    let level = doc.project.levels[0].id;
    doc.edit("connections", |e| {
        let column_t = e.project().default_type(Category::Column).unwrap();
        let beam_t = e.project().default_type(Category::Beam).unwrap();
        let ty = ElementTypeId(e.alloc_id());
        e.add_type(cc_core::connection::host_type(ty, "Connection".into()));
        let column = |x: f64| {
            ElementKind::Column(Column {
                position: DVec2::new(x, 0.0),
                rotation_deg: 0.0,
                base_offset: 0.0,
                height: 3.0,
                top: WallTop::Unconnected,
                structural: true,
            })
        };
        let a = add(e, level, column_t, column(0.0));
        let b = add(e, level, column_t, column(5.0));
        let main = add(e, level, beam_t, ElementKind::Beam(Beam::new(DVec2::ZERO, DVec2::new(5.0, 0.0), 3.0)));
        let tee = add(e, level, beam_t, ElementKind::Beam(Beam::new(DVec2::new(2.5, 0.0), DVec2::new(2.5, 3.0), 3.0)));
        for (member, end, kind) in [
            (a, MemberEnd::Base, ConnectionKind::BasePlate),
            (b, MemberEnd::Base, ConnectionKind::BasePlate),
            (main, MemberEnd::Start, ConnectionKind::EndPlate),
            (main, MemberEnd::End, ConnectionKind::EndPlate),
            (tee, MemberEnd::Start, ConnectionKind::Hanger),
        ] {
            add(e, level, ty, ElementKind::Connection(Connection::new(member, end, kind)));
        }
    });
    doc.project
}

#[test]
fn connections_export_as_assemblies_of_plates_and_fasteners() {
    let project = connections_project();
    let geom = cc_geom::ModelGeometry::build(&project);
    assert!(geom.connections.values().all(|g| g.warning.is_none()), "{:?}", geom.connections);
    let text = export(&project, &ExportOptions::default());
    let count = |s: &str| text.matches(s).count();
    let plates: usize = geom.connections.values().map(|g| g.plates.len()).sum();
    let bolts: usize = geom.connections.values().map(|g| g.bolt_count()).sum();
    let anchors: usize =
        geom.connections.values().filter(|g| g.kind == ConnectionKind::BasePlate).map(|g| g.bolt_count()).sum();
    assert_eq!(count("IFCELEMENTASSEMBLY("), 5);
    assert_eq!(count("IFCRELCONNECTSELEMENTS("), 5);
    assert_eq!(count("IFCPLATE("), plates);
    assert_eq!(count("IFCMECHANICALFASTENER("), bolts);
    let anchor_lines =
        text.lines().filter(|l| l.contains("IFCMECHANICALFASTENER(") && l.contains(".ANCHORBOLT.")).count();
    assert_eq!(anchor_lines, anchors);
    assert_eq!(count("IFCPLATETYPE("), 1, "one plate type per connection type");
    assert_eq!(count("IFCMECHANICALFASTENERTYPE("), 2, "bolts and anchor bolts");
    for kind in ["'base_plate'", "'end_plate'", "'hanger'"] {
        assert!(text.contains(kind), "the kind as ObjectType: {kind}");
    }
}

#[test]
fn connections_come_back_without_a_native_round_trip() {
    let text = export(&connections_project(), &ExportOptions::default());
    let (imported, _) = import(&text, Project::from_template()).unwrap();
    assert_eq!(imported.elements_of(Category::Connection).count(), 0);
    assert_eq!(imported.elements_of(Category::Column).count(), 2, "the members come back");
}

/// Writes `fixtures/connections.ccad` from [`connections_project`]
/// (`cargo nextest run -p cc_ifc --test main write_connections_fixture --run-ignored only`).
#[test]
#[ignore]
fn write_connections_fixture() {
    let text = cc_core::to_ron(&connections_project()).unwrap();
    std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/connections.ccad"), text).unwrap();
}
```

(`count("IFCPLATE(")` does not match `IFCPLATETYPE(`. The anchor check assumes the STEP writer puts one entity per line; if it does not, count `.ANCHORBOLT.` and subtract the one in the fastener type.)

- [ ] **Step 2: Run them to see them fail.** `cargo nextest run -p cc_ifc --test main connections`. Expected: FAIL (`IFCELEMENTASSEMBLY(` count 0).

- [ ] **Step 3: Write the export** in `crates/cc_ifc/src/export.rs` (after `write_rebar`):

```rust
    /// Connections: an `IfcElementAssembly` per connection (`USERDEFINED`,
    /// its kind's code as `ObjectType`) aggregating an `IfcPlate` per plate
    /// (boxes as extruded rectangles) and an `IfcMechanicalFastener` per
    /// bolt (extruded circles, its washers too; `ANCHORBOLT` under base
    /// plates, else `BOLT`), tied to its member by an
    /// `IfcRelConnectsElements`. Connections that built nothing are left out.
    fn write_connections(&mut self) {
        let project = self.project;
        let geom = self.geom;
        let n = Value::Null;
        let material = cc_geom::connection::steel_material(project).map(str::to_string);
        for g in geom.connections.values().filter(|g| g.warning.is_none()) {
            let Some(e) = project.element(g.element) else { continue };
            let Some(storey) = self.storey_placement(e.level) else { continue };
            let level_z = project.level_elevation(e.level);
            let placement = self.local_placement(Some(storey), Frame::at(DVec3::ZERO));
            let gid = self.guid_of(e.uid, &format!("element-{}", e.id));
            let assembly = self.w.add(
                "IFCELEMENTASSEMBLY",
                args![
                    gid,
                    n.clone(),
                    project.type_name(e.type_id),
                    opt_str(&e.comments),
                    g.kind.code(),
                    placement,
                    n.clone(),
                    opt_str(&e.mark),
                    Value::Enum("NOTDEFINED"),
                    Value::Enum("USERDEFINED")
                ],
            );
            self.products.insert(e.id, assembly);
            self.register(e, assembly);
            let style = self.material_style(material.as_deref().unwrap_or("Steel"));
            let mut parts = Vec::new();
            for (i, plate) in g.plates.iter().enumerate() {
                let items: Vec<Id> = plate.blocks.iter().map(|b| self.block_solid(b, level_z)).collect();
                for &item in &items {
                    self.styled(item, style);
                }
                let local = self.local_placement(Some(placement), Frame::at(DVec3::ZERO));
                let shape = self.shape(items, None);
                let gid = self.guid(&format!("connection-plate-{}-{i}", e.id));
                let part = self.w.add(
                    "IFCPLATE",
                    args![gid, n.clone(), PLATE, n.clone(), n.clone(), local, shape, n.clone(), Value::Enum("NOTDEFINED")],
                );
                self.connection_part_type(e.type_id, PLATE, "NOTDEFINED", part);
                parts.push(part);
            }
            let (name, predefined) =
                if g.kind == ConnectionKind::BasePlate { (ANCHOR_BOLT, "ANCHORBOLT") } else { (BOLT, "BOLT") };
            for (i, bolt) in g.bolts.iter().enumerate() {
                let items: Vec<Id> = std::iter::once(&bolt.shank)
                    .chain(&bolt.washers)
                    .filter_map(|r| self.rod_solid(r, level_z))
                    .collect();
                if items.is_empty() {
                    continue;
                }
                for &item in &items {
                    self.styled(item, style);
                }
                let local = self.local_placement(Some(placement), Frame::at(DVec3::ZERO));
                let shape = self.shape(items, None);
                let gid = self.guid(&format!("connection-bolt-{}-{i}", e.id));
                let part = self.w.add(
                    "IFCMECHANICALFASTENER",
                    args![
                        gid,
                        n.clone(),
                        name,
                        n.clone(),
                        n.clone(),
                        local,
                        shape,
                        n.clone(),
                        bolt.shank.diameter,
                        bolt.shank.length(),
                        Value::Enum(predefined)
                    ],
                );
                self.connection_part_type(e.type_id, name, predefined, part);
                parts.push(part);
            }
            if let Some(material) = &material {
                let m = self.material(material);
                self.associate_material(&format!("mat-connection-{}", e.id), parts.clone(), m);
            }
            if !parts.is_empty() {
                self.aggregate(&format!("agg-connection-{}", e.id), assembly, parts);
            }
            if let Some(&member) = self.products.get(&g.member) {
                let gid = self.guid(&format!("connects-{}", e.id));
                self.w.add(
                    "IFCRELCONNECTSELEMENTS",
                    args![gid, n.clone(), n.clone(), n.clone(), n.clone(), member, assembly],
                );
            }
        }
    }

    /// A connection's box as a rectangle extruded along its third axis, in
    /// the coordinates of a placement at the level's elevation.
    fn block_solid(&mut self, b: &cc_geom::connection::Block, level_z: f64) -> Id {
        let z = b.z();
        let position2 = self.w.add("IFCAXIS2PLACEMENT2D", args![self.origin2, Value::Null]);
        let profile = self
            .w
            .add("IFCRECTANGLEPROFILEDEF", args![Value::Enum("AREA"), Value::Null, position2, b.size.x, b.size.y]);
        let origin = b.center - z * (b.size.z / 2.0) - DVec3::Z * level_z;
        let position = self.placement3(Frame { origin, x: b.x, z });
        self.w.add("IFCEXTRUDEDAREASOLID", args![profile, position, self.dir_z, b.size.z])
    }

    /// A bolt or washer as a circle extruded along it (`None` when it has no length).
    fn rod_solid(&mut self, r: &cc_geom::connection::Rod, level_z: f64) -> Option<Id> {
        let axis = (r.b - r.a).try_normalize()?;
        let profile =
            self.w.add("IFCCIRCLEPROFILEDEF", args![Value::Enum("AREA"), Value::Null, Value::Null, r.diameter / 2.0]);
        let frame = Frame { origin: r.a - DVec3::Z * level_z, x: axis.any_orthonormal_vector(), z: axis };
        let position = self.placement3(frame);
        Some(self.w.add("IFCEXTRUDEDAREASOLID", args![profile, position, self.dir_z, r.length()]))
    }

    /// The `IfcPlateType` or `IfcMechanicalFastenerType` of a connection
    /// type's parts named `name` (written once), with `part` among its objects.
    fn connection_part_type(&mut self, type_id: ElementTypeId, name: &'static str, predefined: &'static str, part: Id) {
        let key = (type_id, name);
        if !self.member_types.contains_key(&key) {
            let n = Value::Null;
            let gid = self.guid(&format!("connection-type-{type_id}-{name}"));
            let ty_name = format!("{} - {name}", self.project.type_name(type_id));
            let mut attrs = args![
                gid,
                n.clone(),
                ty_name,
                n.clone(),
                n.clone(),
                n.clone(),
                n.clone(),
                n.clone(),
                n.clone(),
                Value::Enum(predefined)
            ];
            // IFC4 ADD2 TC1: a fastener type also has NominalDiameter and NominalLength.
            let entity = if name == PLATE {
                "IFCPLATETYPE"
            } else {
                attrs.extend(args![n.clone(), n]);
                "IFCMECHANICALFASTENERTYPE"
            };
            let t = self.w.add(entity, attrs);
            self.member_types.insert(key, (t, Vec::new()));
        }
        if let Some((_, objects)) = self.member_types.get_mut(&key) {
            objects.push(part);
        }
    }
```

with module-level constants near the other names:

```rust
/// Names of a connection's parts (and of their types).
const PLATE: &str = "Plate";
const BOLT: &str = "Bolt";
const ANCHOR_BOLT: &str = "Anchor Bolt";
```

and `use cc_core::connection::ConnectionKind;` (or the full path inline). Adapt to the real helpers: `args!` takes `&str`, `String`, `f64` and `Id` (as the curtain wall code does); `opt_str` is the helper `element_args` uses; `Frame` needs `x ⟂ z` (true for both: a block's `x ⟂ x × y`, `any_orthonormal_vector`). The `IfcRelConnectsElements` order is (GlobalId, OwnerHistory, Name, Description, ConnectionGeometry, RelatingElement, RelatedElement). `schema.rs` lists `IFCMECHANICALFASTENERTYPE` with `TypeLayout::Predefined`; this writer adds the two trailing optional attributes itself (12 in all) — if `ifcopenshell.validate` reports a wrong attribute count for it, count the attributes in the IFC4 ADD2 TC1 definition of `IfcMechanicalFastenerType` and match them.

- [ ] **Step 4: Run the tests.** `cargo nextest run -p cc_ifc --test main connections round_trip`. Expected: pass. If the import test fails because the importer refuses an assembly without a body, the fallback is broken, not the export: read `import.rs` for how a product with no `Representation` is handled and report it (do not change the importer in this task beyond skipping such a product if it errors).

- [ ] **Step 5: Write the fixture.** `cargo nextest run -p cc_ifc --test main write_connections_fixture --run-ignored only` (with an older nextest: `--run-ignored ignored-only`). Check `crates/cc_ifc/tests/fixtures/connections.ccad` exists and loads: `cargo run -q -p cc_ifc --example export_ifc -- crates/cc_ifc/tests/fixtures/connections.ccad target/connections.ifc`.

- [ ] **Step 6: CI.** In `.github/workflows/ci.yml`, after the `profiles.ccad` export line add
  `          cargo run --locked -p cc_ifc --example export_ifc -- crates/cc_ifc/tests/fixtures/connections.ccad target/ifc/connections.ifc`
  and after the `profiles.ifc` validate line
  `          python scripts/validate_ifc.py target/ifc/connections.ifc`.
  In `scripts/validate_ifc.py`, append `"IfcPlate", "IfcMechanicalFastener",` to `ELEMENTS` (they now have types and are contained through their assembly or curtain wall). If Python with IfcOpenShell is available locally (`pip install -r scripts/requirements-ifc.txt`), run `python scripts/validate_ifc.py target/connections.ifc` and the curtain fixture (`cargo run -q -p cc_ifc --example export_ifc -- crates/cc_ifc/tests/fixtures/curtains.ccad target/curtains.ifc`, then validate it): both must print `OK`. If the curtain plates fail the new check, report it instead of removing `IfcPlate`; if Python is not available, CI runs it.

- [ ] **Step 7: Clippy, fmt, commit.** `cargo clippy -p cc_ifc --all-targets -- -D warnings`, `cargo fmt --all`, then:

```bash
git add -A
git commit -s -m "Export connections as IFC assemblies of plates and fasteners"
```

---

### Task 5: Connections in the app (cc_app)

**Files:**
- Create: `crates/cc_app/src/connection_ui.rs`, `assets/icons/ui/connection.svg`
- Modify:
  - `crates/cc_app/src/main.rs`: `mod connection_ui;` (alphabetical, after `mod component_sketch;`).
  - `crates/cc_app/src/icons.rs`: `Icon::Connection` after `Brace` in the enum and in `ALL` (its length grows by one), source `Icon::Connection => include_str!("../../../assets/icons/ui/connection.svg"),`.
  - `crates/cc_app/src/commands.rs`: `UiCommand::Connection(cc_core::connection::ConnectionKind)` (doc `/// Place connections of a kind on the selected members, else the active level's.`) after `TypicalRebar`; `all()`, `icon`, `label`, `description`, `engine`, `run` arms (Step 4).
  - `crates/cc_app/src/ribbon.rs`: the *Connections* groups in `structure_tab` (Step 5); `category_icon`: `Category::Connection => Icon::Connection,`.
  - `crates/cc_app/src/properties.rs`: `ElementKind::Connection(c) => crate::connection_ui::props(ui, state, id, c),` (replacing Task 1's `{}`).
  - `assets/i18n/*.ftl` (12): the Task 5 keys.
- Test: `crates/cc_app/src/ui_tests.rs`.

**Interfaces:**
- Consumes: Task 1 (`Connection`, `ConnectionKind::{ALL, key, code, at_column, material}`, `LENGTH`, `COUNT`, `MemberEnd::key`), Task 2 (`ConnectionGeom::{plate_mass, bolt_count, warning}`), Task 3 (`ConnectionsCreate`); `crate::member_host_ui::{finished, length}`; `crate::table::{row, readonly_row, section}`; `crate::engine_edit::element`; `crate::properties::{key, length_drag}`; `crate::actions::run_command`; `AppState::{active_level, selection, session, message}`; `crate::theme::muted`; `cc_views::format::number`.
- Produces: `UiCommand::Connection(ConnectionKind)`; `Icon::Connection`; `connection_ui::{place(state, kind), edit(state, id, field, f), props(ui, state, id, &Connection)}`.

- [ ] **Step 1: Write the failing UI tests** in `ui_tests.rs`:

```rust
/// *Structure → Connections → Base Plate* (steel) places a base plate
/// under each selected column, in one undo step, and says so.
#[test]
fn base_plate_button_places_connections_on_the_selected_columns() {
    use cc_core::connection::ConnectionKind;
    let mut state = sample_state();
    let columns: Vec<cc_core::ElementId> =
        state.project().elements_of(cc_core::Category::Column).map(|e| e.id).collect();
    assert!(!columns.is_empty(), "the sample house has columns");
    state.selection = columns.iter().copied().collect();
    state.ribbon_tab = RibbonTab::Structure;
    state.prefs.structure_material = cc_core::StructuralMaterial::Steel;
    let depth = state.session.doc.undo_depth();
    let mut harness = ribbon_harness(state, 1600.0);
    let label = crate::commands::UiCommand::Connection(ConnectionKind::BasePlate).label(harness.state());
    harness.get_by_role_and_label(Role::Button, &label).click();
    harness.run();
    let state = harness.state();
    let placed: Vec<cc_core::ElementId> = state
        .project()
        .elements
        .values()
        .filter_map(|e| match &e.kind {
            cc_core::ElementKind::Connection(c) => Some(c.member),
            _ => None,
        })
        .collect();
    assert_eq!(placed.len(), columns.len());
    assert!(columns.iter().all(|c| placed.contains(c)));
    assert_eq!(state.session.doc.undo_depth(), depth + 1);
    assert!(state.message.contains(&columns.len().to_string()), "{}", state.message);
}

/// Steel shows base, end and fin plates; timber bolted joints and hangers.
#[test]
fn connection_buttons_follow_the_structure_material() {
    use cc_core::StructuralMaterial as M;
    use cc_core::connection::ConnectionKind;
    for material in [M::Steel, M::Timber] {
        let mut state = sample_state();
        state.ribbon_tab = RibbonTab::Structure;
        state.prefs.structure_material = material;
        let harness = ribbon_harness(state, 1600.0);
        for kind in ConnectionKind::ALL {
            let label = crate::commands::UiCommand::Connection(kind).label(harness.state());
            assert_eq!(harness.query_by_label(&label).is_some(), kind.material() == material, "{kind:?} in {material:?}");
        }
    }
}

/// A connection's rows edit it through `element_set`.
#[test]
fn connection_properties_change_its_bolts() {
    let mut state = sample_state();
    let column = state.project().elements_of(cc_core::Category::Column).map(|e| e.id).next().unwrap();
    let args = serde_json::json!({"kind": "base_plate", "members": [column.0]});
    let out = state.session.execute("connections_create", args).unwrap();
    let id = cc_core::ElementId(out["created"][0].as_u64().unwrap());
    crate::connection_ui::edit(&mut state, id, "rows", |c| c.params.rows = 3);
    let cc_core::ElementKind::Connection(c) = &state.project().element(id).unwrap().kind else { panic!("a connection") };
    assert_eq!(c.params.rows, 3);
    state.selection = [id].into_iter().collect();
    let harness = themed_harness(state, egui::vec2(360.0, 1600.0), egui::Theme::Light, |ui, state| {
        crate::properties::show(ui, state);
    });
    assert!(harness.query_by_label(&t!("prop-bolt-rows")).is_some());
}
```

(If a button is not found at 1600 px because the Steel tab's groups no longer fit, use `ribbon_harness_sized(state, egui::vec2(2400.0, 150.0))` in these tests; do not change the layout for the test. If `crate::properties::show` is named differently, use what `truss_properties_regenerate_it` calls.)

- [ ] **Step 2: Run them to see them fail.** `cargo nextest run -p cc_app connection`. Expected: compile errors (`UiCommand::Connection`, `connection_ui` missing).

- [ ] **Step 3: The icon.** `assets/icons/ui/connection.svg` (a column stub on a base plate with two anchor bolts; strokes inside the 24 grid with a margin):

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M3 14h18v4H3z" fill="#fff" fill-opacity=".2" stroke="none"/><path d="M3 14h18v4H3z"/><path d="M9 14V3M15 14V3"/><path d="M6 12v9M18 12v9" stroke-width="1.25"/></svg>
```

Add `Icon::Connection` as listed under *Files*.

- [ ] **Step 4: The command.** In `commands.rs`:
  - variant (Files); `all()`: `all.extend(cc_core::connection::ConnectionKind::ALL.map(Connection));` right after the line that lists `TypicalRebar`;
  - `icon`: `Connection(_) => Icon::Connection,`;
  - `label`: `Connection(kind) => crate::i18n::tr(kind.key()),`;
  - `description`:

```rust
            Connection(kind) => match kind {
                cc_core::connection::ConnectionKind::BasePlate => t!("cmd-desc-base-plate"),
                cc_core::connection::ConnectionKind::EndPlate => t!("cmd-desc-end-plate"),
                cc_core::connection::ConnectionKind::FinPlate => t!("cmd-desc-fin-plate"),
                cc_core::connection::ConnectionKind::TimberBolts => t!("cmd-desc-timber-bolts"),
                cc_core::connection::ConnectionKind::Hanger => t!("cmd-desc-hanger"),
            },
```

  - `engine`: `Connection(_) => "connections_create",`;
  - `run`: `Connection(kind) => crate::connection_ui::place(state, kind),`.

- [ ] **Step 5: The ribbon.** In `ribbon.rs::structure_tab`, add `use cc_core::connection::ConnectionKind as K;` and, right after each material's framing group (after the wall-framing group too, if the base added one there):
  - in `M::Steel`: `group(ui, t!("group-connections"), |ui| stacks(ui, state, &[&[Connection(K::BasePlate), Connection(K::EndPlate), Connection(K::FinPlate)]]));`
  - in `M::Timber`: `group(ui, t!("group-connections"), |ui| stacks(ui, state, &[&[Connection(K::TimberBolts), Connection(K::Hanger)]]));`

- [ ] **Step 6: Write `connection_ui.rs`:**

```rust
//! Connections in the app: the Structure tab's *Connections* buttons, which
//! run `connections_create` on the selected members (else the active
//! level's), and the rows of a selected connection, whose sizes change
//! through `element_set`.

use crate::member_host_ui::{finished, length};
use crate::state::AppState;
use crate::t;
use crate::table::{readonly_row, row, section};
use cc_core::connection::{COUNT, Connection, ConnectionKind, LENGTH};
use cc_core::{Category, ElementId, ElementKind};
use cc_engine::Ref;
use cc_engine::commands::connection::ConnectionsCreate;
use egui::{DragValue, Ui};

/// Place connections of `kind` with their default sizes: on the selected
/// columns (base plates) or beams, else on the active level's; the result
/// goes to the status bar.
pub fn place(state: &mut AppState, kind: ConnectionKind) {
    let project = state.project();
    let wanted = if kind.at_column() { Category::Column } else { Category::Beam };
    let members: Vec<u64> = state
        .selection
        .iter()
        .filter(|id| project.element(**id).is_some_and(|e| e.category() == wanted))
        .map(|id| id.0)
        .collect();
    let level = if members.is_empty() { state.active_level().map(|l| Ref::Id(l.0)) } else { None };
    let command = ConnectionsCreate { kind: kind.code().into(), members, level, params: None };
    if let Some(out) = crate::actions::run_command(state, command) {
        let count = out["created"].as_array().map_or(0, Vec::len);
        let skipped = out["skipped"].as_u64().unwrap_or(0);
        state.message = t!("msg-connections-placed", "count" => count, "skipped" => skipped);
    }
}

/// Change connection `id` as `f` changes a copy of it (`element_set`);
/// edits under one `field` merge while dragging.
pub fn edit(state: &mut AppState, id: ElementId, field: &str, f: impl FnOnce(&mut Connection)) {
    crate::engine_edit::element(state, id, Some(crate::properties::key((id, field))), |e| {
        if let ElementKind::Connection(c) = &mut e.kind {
            f(c);
        }
    });
}

/// A plate size row: 0 reads *auto* (from the member); true when the edit ended.
fn size(ui: &mut Ui, label: &str, v: &mut f64) -> bool {
    let done = finished(&row(ui, label, |ui| {
        ui.horizontal(|ui| {
            let r = ui.add(crate::properties::length_drag(v).range(0.0..=*LENGTH.end()));
            if *v == 0.0 {
                ui.label(crate::theme::muted(t!("connection-size-auto")));
            }
            r
        })
        .inner
    }));
    // Between 0 and the least length: the least length.
    if *v > 0.0 && *v < *LENGTH.start() {
        *v = *LENGTH.start();
    }
    done
}

fn count(ui: &mut Ui, label: &str, v: &mut u32) -> bool {
    finished(&row(ui, label, |ui| ui.add(DragValue::new(v).range(COUNT))))
}

/// The rows of a selected connection.
pub fn props(ui: &mut Ui, state: &mut AppState, id: ElementId, connection: &Connection) {
    let mut c = connection.clone();
    let mut done = false;
    let member = state
        .project()
        .element(c.member)
        .map(|m| if m.mark.is_empty() { m.id.to_string() } else { m.mark.clone() })
        .unwrap_or_default();
    let built = state.session.geom().connections.get(&id).cloned();
    section(ui, "connection", t!("section-connection"), |ui| {
        readonly_row(ui, &t!("prop-connection-kind"), crate::i18n::tr(c.kind.key()));
        readonly_row(ui, &t!("prop-host"), member);
        readonly_row(ui, &t!("prop-member-end"), crate::i18n::tr(c.end.key()));
        let p = &mut c.params;
        if c.kind != ConnectionKind::TimberBolts {
            done |= length(ui, &t!("prop-plate-thickness"), &mut p.thickness, LENGTH);
            done |= size(ui, &t!("prop-plate-width"), &mut p.width);
            done |= size(ui, &t!("prop-plate-length"), &mut p.length);
        } else {
            done |= length(ui, &t!("prop-thickness"), &mut p.thickness, LENGTH);
        }
        done |= length(ui, &t!("col-bolt-diameter"), &mut p.bolt_diameter, LENGTH);
        done |= count(ui, &t!("prop-bolt-rows"), &mut p.rows);
        done |= count(ui, &t!("prop-bolt-columns"), &mut p.columns);
        done |= length(ui, &t!("prop-bolt-spacing"), &mut p.spacing, LENGTH);
        done |= length(ui, &t!("prop-edge-distance"), &mut p.edge, LENGTH);
        if c.kind == ConnectionKind::BasePlate {
            done |= length(ui, &t!("prop-anchor-length"), &mut p.anchor_length, LENGTH);
        }
        if let Some(g) = &built {
            readonly_row(ui, &t!("col-plate-mass"), format!("{} kg", cc_views::format::number(g.plate_mass(), 2)));
            readonly_row(ui, &t!("col-bolt-count"), g.bolt_count().to_string());
            if g.warning.is_some() {
                ui.label(crate::theme::muted(t!("warn-connection-end")));
            }
        }
    });
    if c != *connection {
        edit(state, id, "connection", move |x| *x = c);
    }
    if done {
        state.session.doc.break_merge();
    }
}
```

Adapt to the real API: `crate::i18n::tr` and `crate::theme::muted` are what `truss_ui.rs` uses; `state.session.geom()` is current in the GUI (it syncs once a frame); `ElementId`'s `Display` gives `E12`. The section closure borrows `c` mutably through `p` and reads `c.kind`/`c.end` before it: if the borrow checker objects, copy `let (kind, end) = (c.kind, c.end);` before the section and use them inside.

- [ ] **Step 7: i18n.** Add the Task 5 keys of the table and the long-text keys (`msg-connections-placed`, `warn-connection-end`, `cmd-desc-base-plate`, `cmd-desc-end-plate`, `cmd-desc-fin-plate`, `cmd-desc-timber-bolts`, `cmd-desc-hanger`) to the 12 files under `# Connections`.

- [ ] **Step 8: Run the tests.** `cargo check --tests -p cc_app`, then `cargo nextest run -p cc_app connection structure_material i18n icons ribbon palette`. Expected: pass (`every_icon_rasterizes` covers the new icon; the ribbon test checks every button has a label).

- [ ] **Step 9: Clippy, fmt, commit.** `cargo clippy -p cc_app --all-targets -- -D warnings`, `cargo fmt --all`, then:

```bash
git add -A
git commit -s -m "Place connections from the Structure tab and edit them in the palette"
```

---

### Task 6: Documentation and full check

**Files:**
- Modify:
  - `AGENTS.md`: a new **Connections** bullet after the trusses one (after wall framing if the base has it): `cc_core::connection` (`ElementKind::Connection(Connection { member, end: MemberEnd (Base, Start, End), kind: ConnectionKind (BasePlate, EndPlate, FinPlate, TimberBolts, Hanger), params: ConnectionParams })`, category `Connection`, a `TypeData::Connection {}` type created with the first one; lengths `LENGTH` 1 mm to 2 m, a plate's `width`/`length` 0 = from the section plus `edge`; rows and columns `COUNT` 1 to 10; refused by commands, reset by `repair_connections`, which keeps one connection per member end, the lowest id; removed with its member (`remove_element` → `Project::connections_of`), following its phases and option (`phase::phase_host`), never copied (`copy_set`), no placement of its own); `cc_geom::connection` (built after the beams into `ModelGeometry::connections`: `ConnectionGeom` with `Plate`s of oriented `Block`s and `Bolt`s of `Rod`s with washers, plate volume and mass at 7850 kg/m³, bolt count and length; `suits` (end plates and bolted joints at `BeamEnd::Column`, fin plates at columns or beams, hangers at beams); a `ConnectionWarning` and no parts otherwise; one steel solid per connection of category `Connection`, in the project's first metal); plans draw it where its member stands (`plan::draw_connections`); `cc_engine::commands::connection` (`connections_create`: `kind`, `members` or the level's, partial `params`; skips ends that have one; one undo step; `{created, skipped}`; `element_set` refuses a new member, end or kind; `element_get` `derived.plate_volume`, `plate_mass`, `bolt_count`, `bolt_diameter`, `bolt_length`, `warning`); schedule fields `ConnectionKind`, `PlateMass`, `BoltCount`, `BoltDiameter`; IFC export (`IfcElementAssembly` `USERDEFINED` with the kind's code as `ObjectType`, aggregating an `IfcPlate` per plate and an `IfcMechanicalFastener` per bolt, `ANCHORBOLT` under base plates else `BOLT`, typed per connection type; `IfcRelConnectsElements` from the member; import keeps the parts by the existing fallback); the app (*Structure → Connections*: steel Base Plate, End Plate, Fin Plate; timber Bolted Joint, Hanger; `UiCommand::Connection(kind)`, `connection_ui::place` on the selected members or the active level's; `connection_ui::props`). Also add `connections.ccad` to the IFC validation fixture list in the *Commands* section's CI sentence.
  - `docs/interface.md`, `docs/interface.pt-BR.md`: in the Structure tab paragraph, steel also shows *Connections* (Base Plate, End Plate, Fin Plate) and timber *Connections* (Bolted Joint, Hanger); then a paragraph: each button places connections with default sizes on the selected columns (base plates) or beams, else on the active level's, at the ends that suit the kind (end plates and bolted joints where a beam stops at a column, fin plates at a column or a beam, hangers where a beam meets a beam), skipping ends that have one, and reports how many in the status bar; plates and bolts follow the member, go with it when it is deleted and are not copied with it; the palette shows the kind, the member and its end, the plate thickness, width and length (*auto*: from the section), bolt diameter, rows, columns, spacing, edge distance and (base plates) anchor length, with the plate mass and bolt count; a connection whose end no longer suits it draws nothing and says so.
  - `docs/automation.md`, `docs/automacao.md`: a short paragraph after the `truss_create` one (prose in each file's language) with `{"command": "connections_create", "args": {"kind": "end_plate", "members": [12, 13], "params": {"rows": 3, "bolt_diameter": 0.02}}}` and `{"command": "element_set", "args": {"id": 42, "params": {"params": {"thickness": 0.02}}}}`, and the schedule fields.
  - `CHANGELOG.md` under `## [Unreleased]` → `### Added`: steel and timber connections (base plates with anchor bolts, end plates, fin plates, bolted timber joints with washers, hangers) at column bases and beam ends, sized from the member's section and following it; *Structure → Connections* in the steel and timber groups; `connections_create`; plate mass, bolt count and diameter in schedules; IFC export as element assemblies of plates and mechanical fasteners.
- [ ] **Step 1: Write the docs** above, concise and in the style of the neighboring entries.
- [ ] **Step 2: Full check.**
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo nextest run --workspace`
  - `cargo test --workspace --doc`

  Expected: all green; `docs/commands.md` matches `cargo run -q -p cc_cli -- reference`.
- [ ] **Step 3: Commit.**

```bash
git add -A
git commit -s -m "Document steel and timber connections"
```

---

## Self-review

- Spec coverage:
  - `Connection` element, category `Connection`, `TypeData::Connection {}`, `member`/`MemberEnd`/kinds/params, bounds, removal with the member, one per end, not copied: Task 1.
  - Geometry per kind from the member's current geometry (columns: position, rotation, base; beams: `BeamGeom` end, direction, resolved face, section box), solids of category `Connection` in steel, `ModelGeometry::connections` with plate volume, bolt count and length, warnings for a missing member or unsuitable end: Task 2.
  - Plans draw the solids; sections and 3D views draw every solid already; visibility by category (`Category::ALL`/`MODEL`): Tasks 1 and 2.
  - `connections_create` (kind, members default, partial params, suitable ends, skips existing, one undo step, `{created, skipped}`), `element_get`/`element_set` with refusals and `derived` (volume, mass at 7850, bolt count and diameter), schedule fields: Task 3.
  - IFC assembly (`USERDEFINED`, kind in `ObjectType`), `IfcPlate` per plate, `IfcMechanicalFastener` per bolt (`ANCHORBOLT`/`BOLT`), boxes and extruded circles, `IfcRelConnectsElements`, import fallback, validator and CI fixture: Task 4.
  - Ribbon groups per material (steel: Base Plates, End Plates, Fin Plates; timber: Bolted Joints, Hangers), selected members else the active level's, status bar, palette rows, 12 languages, one shared icon: Task 5.
  - Tests listed in the spec: cc_geom (base plate size and position, anchors rows × columns, end plate normal on a turned beam, fin plate length, timber bolt length, hanger parts, moved column, missing member) Task 2; engine (base plates on every column then skipped, end plates only at column ends, one undo step, `element_set` refusals, repair of a second connection on one end, deleting a column) Task 3 (repair also in Task 1); IFC (assembly with plates and fasteners, validator) Task 4; schedule (plate mass = volume × 7850) Task 3; app (Base Plates button on the selected columns) Task 5.
  - Docs and full check: Task 6.
- Names used across tasks: `cc_core::connection::{Connection, ConnectionKind, ConnectionParams, MemberEnd, LENGTH, COUNT, host_type}`, `ConnectionKind::{ALL, code, from_code, key, at_column, material, defaults}`, `MemberEnd::{ALL, code, key}`, `ConnectionParams::{check, reset_out_of_range}`, `Connection::{new, fits}`, `Project::{connections_of, connection_at, repair_connections}`; `cc_geom::connection::{Block, Rod, Bolt, Plate, ConnectionGeom, ConnectionWarning, beam_end_join, suits, steel_material, solid, connection_geom, build_connections, BOLT_EXTRA, WASHER_RATIO, FIN_HEIGHT, HANGER_SIDE, STEEL_COLOR}`, `ConnectionGeom::{plate_volume, plate_mass, bolt_count, bolt_length, faces}`, `ModelGeometry::connections`; `cc_engine::commands::connection::{ConnectionsCreate, new_connection_type, params_arg, check, derived}`; `ScheduleField::{ConnectionKind, PlateMass, BoltCount, BoltDiameter}`; exporter `write_connections`, `block_solid`, `rod_solid`, `connection_part_type`; `UiCommand::Connection`, `Icon::Connection`, `connection_ui::{place, edit, props}`.
- Decisions where the spec was open: listed under Global Constraints.
