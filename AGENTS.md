# AGENTS.md

## Projeto

Este repositório contém o **Meeting Recorder**, um aplicativo desktop Windows 10/11 para gravação local de reuniões e transcrição offline.

O produto deve priorizar:

1. baixo consumo durante a reunião;
2. privacidade;
3. estabilidade;
4. arquitetura modular;
5. funcionamento offline;
6. distribuição simples por instalador.

Leia `docs/ARCHITECTURE.md` antes de realizar alterações arquiteturais ou criar novos módulos.

---

## Stack obrigatória

### Frontend
- Tauri 2
- React
- TypeScript
- Vite

### Backend / Core
- Rust

### Persistência
- SQLite
- Preferência inicial: `rusqlite`

### Áudio
- WASAPI
- WASAPI Loopback para áudio do sistema

### Transcrição
- `whisper.cpp`
- Modelo Whisper multilíngue quantizado
- Modelo padrão inicial: Base Multilingual Q5

### Vídeo
- Windows Graphics Capture
- Media Foundation
- H.264
- Aceleração de hardware quando disponível

### Distribuição
- Tauri bundler
- NSIS `.exe`
- MSI opcional

---

## Regras arquiteturais

### 1. O aplicativo é local-first

Não adicionar:

- APIs externas;
- serviços de nuvem;
- telemetria externa;
- login;
- upload automático;
- serviços de transcrição online.

Sem aprovação explícita, nenhum dado da reunião pode sair do computador.

---

### 2. Não executar IA durante a reunião

Durante uma gravação:

- não iniciar Whisper;
- não gerar resumo;
- não executar LLM;
- não fazer processamento pesado.

O fluxo correto é:

```text
gravar
↓
finalizar
↓
preparar áudio
↓
transcrever
```

---

### 3. Frontend não controla dispositivos diretamente

O React deve conversar com comandos Tauri.

Fluxo:

```text
React
↓
Tauri Command
↓
Rust Core
↓
Módulo nativo
```

Não colocar regras de negócio importantes no frontend.

---

### 4. MeetingManager é o orquestrador

O frontend deve iniciar e finalizar reuniões por meio do `MeetingManager`.

Responsabilidades esperadas:

```text
start_meeting()
stop_meeting()
get_recording_state()
```

O `MeetingManager` coordena:

- banco;
- storage;
- microfone;
- áudio do sistema;
- vídeo;
- transcrição pós-reunião.

---

### 5. Áudio e vídeo não ficam no SQLite

SQLite deve armazenar:

- metadados;
- caminhos;
- transcrição;
- status.

Nunca armazenar arquivos grandes como BLOB sem uma decisão arquitetural explícita.

---

### 6. Falhas parciais não devem destruir a reunião

Exemplo:

Se o microfone falhar, mas o áudio do sistema continuar:

- continuar capturando o áudio disponível;
- registrar erro;
- atualizar status;
- preservar os arquivos válidos.

Prioridade do produto:

```text
CAPTURAR
↓
NÃO PERDER
↓
SALVAR
↓
TRANSCREVER
```

---

## Módulos esperados

Backend Rust:

```text
audio/
transcription/
video/
database/
storage/
meeting/
commands/
```

Responsabilidades:

### `audio/`
- microfone;
- WASAPI Loopback;
- mixagem/preparação.

### `transcription/`
- integração com whisper.cpp;
- progresso;
- cancelamento.

### `video/`
- Windows Graphics Capture;
- encoder H.264;
- seleção de janela/monitor.

### `database/`
- schema;
- migrations;
- repository.

### `storage/`
- diretórios;
- caminhos;
- arquivos da reunião.

### `meeting/`
- orquestração do ciclo de vida.

### `commands/`
- ponte entre frontend e backend.

---

## Estrutura local de dados

Usar diretórios do usuário do Windows.

Nunca hardcode caminhos contendo nome de usuário.

Estrutura alvo:

```text
%LOCALAPPDATA%\MeetingRecorder\

database/
recordings/
models/
logs/
```

Cada reunião:

```text
recordings/{meeting_id}/
```

Possíveis arquivos:

```text
microphone.wav
system.wav
merged.wav
video.mp4
transcript.txt
```

---

## Desempenho

Durante gravação sem vídeo:

- priorizar baixo uso de CPU;
- não bloquear UI;
- não rodar Whisper;
- usar tarefas/threads apropriadas.

Nunca introduzir processamento em tempo real sem justificar e medir.

Metas de desempenho são metas, não garantias.

---

## Dependências

Antes de adicionar uma nova dependência:

1. verifique se a plataforma ou stack atual já resolve;
2. explique por que a dependência é necessária;
3. evite bibliotecas grandes para tarefas simples;
4. prefira APIs nativas do Windows para captura.

Evitar FFmpeg completo como dependência padrão do produto sem decisão explícita.

---

## Qualidade de código

Antes de concluir uma tarefa Rust:

```bash
cargo fmt
cargo clippy
cargo test
```

Antes de concluir alteração no frontend:

```bash
npm run build
```

Quando aplicável, execute também os testes específicos do módulo alterado.

Não declare uma tarefa concluída sem executar validações relevantes.

---

## Forma de trabalhar

Antes de modificar código:

1. leia este `AGENTS.md`;
2. leia `docs/ARCHITECTURE.md`;
3. analise o código atual;
4. preserve a arquitetura existente;
5. faça a menor alteração possível;
6. evite refatorações não solicitadas.

Ao concluir uma tarefa, informe:

- arquivos alterados;
- decisões técnicas;
- testes executados;
- riscos;
- pendências.

---

## Escopo de versões

### 0.1.0

Prioridade:

- app Windows;
- instalador;
- SQLite;
- microfone;
- áudio do sistema;
- histórico;
- whisper.cpp;
- transcrição local;
- player;
- exportação TXT;
- system tray.

### 0.2.0

Adicionar:

- gravação opcional de monitor;
- gravação opcional de janela;
- Windows Graphics Capture;
- H.264.

---

## Fora do MVP

Não desenvolver sem solicitação explícita:

- login;
- cloud;
- sync;
- resumo via LLM;
- chatbot;
- diarização avançada;
- extensão Chrome;
- transcrição em tempo real;
- integrações Meet/Teams;
- edição de vídeo.

---

## Fonte de verdade

Para decisões arquiteturais, consulte:

```text
docs/ARCHITECTURE.md
```

Para roadmap e sequência de implementação, consulte:

```text
docs/IMPLEMENTATION_PLAN.md
```

Se uma tarefa conflitar com a arquitetura documentada, não altere silenciosamente a arquitetura.

Explique o conflito antes de implementar.
