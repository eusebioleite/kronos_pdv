# Design: ERP-CRM Synchronization Workflow

## Contexto

O sistema kronos_pdv sincroniza pedidos de venda do Oracle ERP para o DealerCRM via API HTTP. A sincronizacao e orientada a eventos: uma trigger no Oracle (na tabela de programacoes) registra o order_code afetado em uma fila (inventario.cards). O Rust le essa fila periodicamente e reconcilia o estado do CRM com o estado atual do Oracle.

## Decisoes de Design

### 1. Sem distincao entre pedido Fechado (F) e Aberto (A)

**Decisao:** A distincao entre ORDER_KIND = 'F' e 'A' foi removida completamente da logica de agrupamento.

**Justificativa:** Apos analise de dados reais, verificou-se que pedidos fechados tambem possuem programacoes com datas de entrega distintas (ex: pedido 0000476 com entregas em 2026-03-17 e 2026-03-18). Portanto, o tipo de pedido nao e um criterio valido de agrupamento.

### 2. Regra Universal de Agrupamento: order_code + schedule_date

Um card no CRM representa exatamente um conjunto de itens de um pedido a serem entregues em uma data especifica.

Chave de identidade do card (imutavel):
  ActivityFunctionalRequirements = "ERP-{order_code}-{YYYY-MM-DD}"
  Exemplo: ERP-0000476-2026-03-17

Campo auxiliar de rastreabilidade:
  ActivityBusinessRule = "{schedule_code1},{schedule_code2},..."
  Exemplo: 4764,4765

### 3. Sincronizacao Declarativa (Reconciliacao)

Para cada order_code na fila:
1. Busca estado desejado no Oracle (programacoes ativas)
2. Constroi cards desejados, agrupando por schedule_date
3. Busca estado atual no MySQL: WHERE ActivityFunctionalRequirements LIKE 'ERP-{order_code}-%'
4. Reconcilia: match=PATCH, novo=POST, sobra=DELETE
5. Atualiza status da fila

### 4. Preservacao de Estado do CRM

PATCH nao inclui workflow_stages_code (coluna do Kanban). Structs separados: ActivityCreate (POST) e ActivityUpdate (PATCH parcial).

### 5. Trigger Unica no Oracle

Trigger em f_prgven (INSERT/UPDATE/DELETE) escreve order_code na fila com status SINCRONIZAR. Nao ha mais ATUALIZAR vs EXCLUIR.

## Modulos Afetados

| Modulo               | Mudanca                                                                 |
|----------------------|-------------------------------------------------------------------------|
| repository::sync     | Rewrite: remover logica PDV-A/F, novo build_desired_cards, reconcile   |
| api::mod             | Novo ActivityCreate (POST) e ActivityUpdate (PATCH parcial sem coluna) |
| dealercrm::mod       | Adicionar ActivityBusinessRule; query LIKE 'ERP-{order_code}-%'        |
| repository::queue    | Simplificar: apenas status SINCRONIZAR                                  |
| Oracle Trigger       | Unica trigger em f_prgven -> SINCRONIZAR em inventario.cards           |
