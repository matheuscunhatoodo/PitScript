# PitScript — publicação no GitHub

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publicar o projeto existente em `matheuscunhatoodo/PitScript`, público, conforme autorização do usuário em 07/10/2026.

**Architecture:** Publicação do checkout atual, mantendo APIs, identidade instalada, arquitetura e status de candidata da versão 0.1.0. Recursos nativos e modelos existentes acompanham o código; build, instaladores, toolchains, bancos, logs e gravações locais ficam excluídos.

**Tech Stack:** Git, GitHub, Tauri 2, React/TypeScript, Rust, SQLite, WASAPI, whisper.cpp e sherpa-onnx.

**Spec:** Solicitação do usuário nesta conversa; `AGENTS.md`, `docs/ARCHITECTURE.md` e `docs/RELEASE_REPORT.md`.

## Global Constraints

- Repositório chamado `PitScript`, visibilidade pública, conta autenticada `matheuscunhatoodo`.
- Nenhum dado de reunião pode sair do computador sem aprovação explícita; excluir dados locais do envio.
- Nenhuma funcionalidade nova nem declaração de MVP pronto.
- Não publicar instaladores como release estável nesta tarefa.
- Manter licenças e atribuições de recursos de terceiros; não escolher licença do código original sem autorização.

## Review Focus

- Gravações, SQLite, logs, credenciais e perfis de teste: verificar ausência no índice Git.
- Modelo/recursos nativos: manter arquivos necessários e licenças; confirmar tamanho abaixo do limite por arquivo.
- Conta/repositório existentes: verificar destino antes de criar; não substituir histórico remoto.
- Identidade de commit: usar identidade GitHub e e-mail noreply, sem publicar e-mail corporativo.
- Status da release: README registra pendências de Windows limpo/offline e gravação longa.

## Task 1: Preparação e verificação

**Files:** `.gitignore`, `README.md`, `docs/RELEASE_REPORT.md` e este plano.

- [x] Revisar documentação/código atual, arquivos candidatos e autenticação.
- [x] Ajustar README para PitScript e excluir explicitamente dados locais/segredos dos candidatos.
- [x] Remover referências ao caminho pessoal do relatório público.
- [x] Executar build/testes/lint/frontend, fmt/Clippy/testes Rust e revisão do índice; preservar resultados manuais pendentes.

## Task 2: Publicação

**Files:** metadados Git locais; repositório remoto público.

- [ ] Criar commit inicial em `main` com arquivos revisados e identidade GitHub noreply.
- [ ] Criar repositório público vazio `matheuscunhatoodo/PitScript`, sem auto-init nem licença presumida.
- [ ] Configurar `origin` e enviar `main`, sem force push.
- [ ] Confirmar visibilidade, branch padrão, HEAD remoto e arquivos essenciais; informar URL e pendências.

## Evidências da preparação

- 161 arquivos revisados; nenhum dado local/segredo identificado nos candidatos. Os 37 recursos de build foram preservados. Maior arquivo: 59.707.625 bytes, abaixo do [limite por arquivo do GitHub](https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-large-files-on-github).
- `npm run lint`, `npm test` (quatro aprovados), `npm run format:check`, `npm run build` aprovados em 07/10/2026.
- `cargo fmt --check`, `cargo clippy --offline --all-targets -- -D warnings` aprovados. Suite Rust final: 127 aprovados, zero falhas, cinco testes manuais ignorados; 28,38 s.
- A primeira execução Rust teve erros de permissão no TEMP do sandbox. O mesmo teste e a suíte inteira passaram com TEMP/TMP em pasta gravável do projeto, sem modificar o código. Logs do ambiente/copy de cache não fazem parte da publicação.
- Revisão independente: nenhum bloqueio confirmado em privacidade, recursos ou documentação. Observação menor pendente: documentar uma versão específica de Node; instruções atuais dizem Node.js com npm.
- A release continua candidata, com validações manuais críticas registradas no checklist existente.
