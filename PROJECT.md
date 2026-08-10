---
name: dolphin_atividades
language: pt-BR
runtime: go
entrypoint: main.go
description: "Serviço Go que processa atividades armazenadas em Oracle e as sincroniza com DealerCRM. Documento formatado para consumo por IAs: metadados, estrutura, comandos, modelos de dados e decisões arquiteturais."
---

**Visão Geral**
- **Projeto**: Serviço de integração para ler linhas de atividade em Oracle e enviar para DealerCRM.
- **Público-alvo**: agentes de software e IAs que precisam entender, analisar ou automatizar modificações no código.

**Metadados (machine-readable)**
```yaml
name: dolphin_atividades
language: go
entrypoint: main.go
build: go build -o pdv.exe .
run: go run . USER/PASS@HOST:PORT/SERVICE
db: oracle (driver: godror)
http_client: shared *http.Client with timeouts
mode: long-running poller (ticker loop)
```

**Propósito e comportamento principal**
- Polling contínuo: um loop com ticker (padrão ~5s) que chama `processPendingActivities`.
- Para cada atividade pendente: buscar metadados, possivelmente buscar arquivos, subir anexos, montar payload e POST/PATCH no DealerCRM, atualizar status no Oracle.
- Política de retries: `UpdateStatusRetry` e contador de tentativas; valor máximo tratado (ex.: 5) como sentinel.

**Arquitetura e Componentes**
- `main.go`: inicializa, abre DB, inicia ticker.
- `internal/bd/`: lógica de acesso a dados, modelos (`modelo.go`) e utilitários (`utilidades.go`).
- `internal/fila/fila.go`: abstração de fila de processamento.
- `internal/sync/`: construção de payloads e lógica de sincronização (`card.go`, `dados.go`).
- `http/`: cliente HTTP compartilhado (`client.go`), modelos e utilitários.
- `utilidades/`: funções auxiliares como progresso e utilitários gerais.
- `oracle/`: scripts SQL e exemplos de DDL/fixtures.

**Convencões importantes**
- Tipos SQL: uso extensivo de `sql.NullString`, `sql.NullInt64`, `sql.NullTime`; tratar essas estruturas como fontes canônicas para campos nulos.
- Reutilizar `*http.Client`: não criar uma instância por requisição.
- Decisão POST vs PATCH: presença/validade de `ActivityGuid` ou `ActivityCode` determina criar ou atualizar.
- Validar `ContextGuid` antes de construir payloads (não enviar quando ausente).

**Como compilar e executar (exemplos)**
```
# Buscar dependências
go mod tidy

# Compilar binário
go build -o pdv.exe .

# Executar (requere string de conexão Oracle)
go run . USER/PASS@HOST:PORT/SERVICE

# Exemplos de modo manual (fixtures JSON em docs/manual)
go run . manual build-activity -fixture .\docs\manual\build_activity.json
go run . manual post-activity -fixture .\docs\manual\post_activity.json
```

**Entradas e Saídas (modelos de dados principais)**
- `Activity` (tabela Oracle): campos principais incluem identificadores, `ContextGuid`, `ActivityGuid`, `ActivityCode`, timestamps, status e referências a arquivos.
- `ActivityFile`: metadados de anexos (nome, tipo, conteúdo binário ou referência).
- Payload para DealerCRM: JSON com campos opcionais controlados por ponteiros/omitempty; helpers `nullStringPtr` / `nullInt64Ptr` controlam presença/omissão.

**Fluxo por atividade (resumido em passos)**
1. Ler atividades pendentes do Oracle.
2. Se houver arquivos, buscar `ActivityFile` e efetuar upload(s) de anexos.
3. Montar `ActivityCompleteRequest` (payload) com os campos necessários.
4. Decidir POST (criar) ou PATCH (atualizar) conforme chave.
5. Atualizar status e contador de retries no Oracle (UPDATE ... RETURNING RETRIES INTO ...).

**Dependências / Pré-requisitos de ambiente**
- Go toolchain compatível (versão conforme `go.mod`).
- Acesso a Oracle (string `USER/PASS@HOST:PORT/SERVICE`).
- Variáveis/segredos: client_id/client_secret/origin estão em `defaultAPIConfig` (mover para env em produção).

**Modo Manual e Fixtures**
- A pasta `docs/manual` contém fixtures JSON usados por comandos `manual` para testar funções isoladas sem depender do loop.
- Exemplos úteis: `get-token`, `post-activity`, `post-attachment`, `build_activity`.

**Arquivos e locais chave**
- [main.go](main.go#L1) — ponto de entrada.
- [internal/bd/modelo.go](internal/bd/modelo.go#L1) — modelos de DB.
- [http/client.go](http/client.go#L1) — cliente HTTP compartilhado.
- [oracle](oracle/) — scripts SQL e DDL.
- [docs/manual](docs/manual/) — fixtures para testes manuais.

**Observações para IAs / Pontos de atenção ao automatizar**
- Tratar cuidadosamente `sql.Null*` ao gerar código ou refatorar: substituir por tipos não-nulos requer regras de migração e validação.
- Não criar `http.Client` por requisição; ao gerar testes, mockar transporte em `http.Client` compartilhado.
- Se automatizar secreções, mover `defaultAPIConfig` para variáveis de ambiente ou secret manager.
- Ao sugerir mudanças em SQL, validar compatibilidade com Oracle/godror e manter o padrão `RETURNING` usado para contadores.

**Checklist para contribuidores (rápido)**
- [ ] Validar `ContextGuid` antes de enviar payload.
- [ ] Reusar `http.Client` em testes e produção.
- [ ] Usar fixtures de `docs/manual` para testes manuais.
- [ ] Evitar hardcoding de segredos.

**Próximos passos sugeridos (para automação)**
1. Adicionar `README.md` em `docs/` com exemplos curtos de uso via fixtures.
2. Externalizar configurações sensíveis para `env` e adicionar `config.example`.
3. Criar testes unitários para builders de payloads e para a camada `internal/sync`.

**Licença e notas finais**
- Este arquivo foi gerado para compreensão programática (IA). Utilize-o como base para análises estáticas, geração de documentação adicional e pipelines de CI.
