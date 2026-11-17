# Modelagem de componentes

*English: [component-modeling.md](component-modeling.md).*

Componentes (mobiliário, aparelhos, marcenaria, portas, janelas…) são definições paramétricas: parâmetros com nome e peças cujas coordenadas são expressões. O **ambiente de modelagem de componentes** dá forma a eles visualmente, e cada passo também é um comando do motor. Assim, um componente pode ser construído por um script, pela linha de comando `ccad` ou por MCP tão bem quanto à mão.

## O ambiente

*Editar componente* (*Arquitetura → Modelo*, ou *Editar componente…* nas propriedades de um tipo de componente) abre um espaço de trabalho próprio. As vistas do projeto dão lugar às vistas **Planta**, **Frente**, **Lateral** e **3D** do componente. O ribbon mostra a aba **Criar**, e o painel à esquerda mostra os tipos do componente, seus parâmetros e as propriedades do que está selecionado. *Concluir* mantém as mudanças; *Cancelar edição* desfaz tudo o que foi feito desde que o ambiente abriu. *Novo componente* (*Arquitetura → Modelo*) pede um nome e o tipo do componente (mobiliário, marcenaria, louças e metais, iluminação, elétrica, genérico, porta ou janela) e mostra os parâmetros com que esse tipo começa: largura, profundidade e altura na maioria, um diâmetro na iluminação, a largura, a altura e o peitoril da abertura em portas e janelas; um componente genérico começa sem nenhum, e os parâmetros são criados por você. *Criar* faz o componente e o abre aqui; *Cancelar edição* o remove, e *Concluir* escolhe a ferramenta que o posiciona. Ferramentas, cliques e teclas estão em [interface.pt-BR.md](interface.pt-BR.md#ambiente-de-modelagem-de-componentes).

## Como o modelo funciona

Tudo é expressão do componente (`cc_core::expr`): não há resolvedor de restrições. Três ideias se apoiam nisso.

- **Planos de referência** (`ComponentDef::ref_planes`) são planos com nome, perpendiculares a X (esquerda/direita), Y (frente/fundo) ou Z (alturas), numa posição dada por uma expressão. Três planos de origem estão sempre lá, em zero: `CenterX`, `CenterY` e `Base`. Os nomes dos planos valem em qualquer expressão do componente, então planos podem ler parâmetros e uns aos outros, em qualquer ordem; ciclos e nomes desconhecidos são apontados.
- **Prender** uma peça a um plano é escrever o nome do plano na expressão da peça (`x = Left`, `início = Under`, `fim = Top`). A peça passa a acompanhar o plano aonde quer que os parâmetros o levem.
- **Cotas** entre planos paralelos (`dimensions`) mostram a distância entre eles. Rotular uma cota com um parâmetro reescreve o plano que se move como o outro mais ou menos o parâmetro (`Right = Left + Largura`), e o parâmetro passa a controlá-lo. **Igualdades** (`equals`, EQ) escrevem o plano do meio como `(a + b) / 2`.

Como tudo fica gravado como expressão, os `.ccomp` e projetos antigos abrem sem mudança, e os campos novos são omitidos quando vazios.

## Peças

| Forma | Feita de | `kind` no motor |
|---|---|---|
| Extrusão | um contorno fechado no plano de planta, frente ou lateral, de `start` a `end` ao longo da normal do plano | `extrusion` |
| Revolução | um contorno fechado girado `angle` graus em torno da linha u = `axis` do seu plano | `revolution` |
| Varredura | uma seção fechada (u à esquerda do caminho, v na normal) levada por um caminho aberto de linhas e arcos, em meia-esquadria nos cantos | `sweep` |
| Transição | um contorno de base em `start` unido a um de topo em `end`, os dois reamostrados por igual para formas diferentes se alinharem | `blend` |

Qualquer peça pode ser um **vazio**, recortado das peças sólidas que ele cruza. Pontos de contorno podem ter um `radius`, que faz do segmento até o próximo ponto um arco (positivo bojando para fora). Caixas, cilindros, extrusões em planta e malhas importadas de definições antigas continuam funcionando.

## Comandos

Todos recebem `type`, qualquer tipo do componente (por id ou nome). Uma mudança vale para todos os tipos do componente num só passo de desfazer, e cada tipo mantém seus próprios valores de parâmetros.

| Comando | Faz |
|---|---|
| `component_get` | a definição, o valor de cada plano, os valores das cotas, os problemas e os tipos com seus valores |
| `component_param_add` / `_set` / `_delete` | parâmetros; renomear chega a toda expressão e cota; um em uso não pode ser apagado |
| `component_ref_plane_add` / `_set` / `_delete` | planos de referência (os de origem não se movem nem saem) |
| `component_ref_line_add` / `_delete` | linhas de referência num plano de esboço |
| `component_dimension_add` / `_set` / `_delete` | cotas entre planos, seu rótulo, valor e posição |
| `component_equal_add` / `_delete` | igualdades (EQ) |
| `component_piece_add` / `_set` / `_delete` | peças de esboço: extrusão, revolução, varredura, transição, sólida ou vazio |

[`examples/component_bench.ccmacro`](../examples/component_bench.ccmacro) modela um banco paramétrico só com esses comandos: `ccad --save-as banco.ccad script examples/component_bench.ccmacro`. A referência completa está em [commands.md](commands.md).

## Ainda não

- **Componentes aninhados:** uma peça que é outro componente, com os parâmetros ligados. Isso exige que as peças sejam resolvidas junto com as outras definições do projeto.
- **Visibilidade por vista ou nível de detalhe:** peças que aparecem só em algumas vistas ou níveis de detalhe. As vistas ainda não têm nível de detalhe.
- **Restrições dentro do esboço** (paralelo, tangente, segmentos iguais): os pontos são expressões presas a planos, não resolvidas.
- **Arrastar faces e planos nas vistas:** por enquanto as edições se fazem com as ferramentas e nas propriedades.
