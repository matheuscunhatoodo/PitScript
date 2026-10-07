# Meeting Recorder Local — Plano de Implementação com Codex

## 1. Objetivo do projeto

Criar um aplicativo desktop para Windows 10/11 capaz de:

- Gravar reuniões localmente.
- Capturar o microfone do usuário.
- Capturar o áudio reproduzido pelo Windows.
- Opcionalmente gravar uma janela ou monitor.
- Realizar transcrição 100% local após o término da reunião.
- Armazenar histórico, metadados e transcrições localmente.
- Exportar transcrições.
- Funcionar sem login, conta ou serviço em nuvem.
- Ser distribuído por meio de instalador `.exe` e/ou `.msi`.

O aplicativo deve priorizar:

1. Baixo consumo durante a reunião.
2. Privacidade.
3. Arquitetura simples e modular.
4. Facilidade de instalação.
5. Boa manutenção futura.
6. Possibilidade de expansão sem reescrever o projeto.

---

# 2. Princípios técnicos

## 2.1. Regra principal de desempenho

Durante a reunião:

- Não executar Whisper.
- Não executar IA.
- Não gerar resumo.
- Não fazer processamento pesado.
- Apenas capturar, sincronizar e persistir áudio/vídeo.

Após a reunião:

- Iniciar a transcrição local.
- Atualizar progresso.
- Salvar resultado no SQLite.
- Liberar os recursos após o processamento.

---

## 2.2. Privacidade

O MVP deverá ser 100% local.

Nenhum conteúdo da reunião deverá ser enviado para:

- APIs externas.
- Serviços de nuvem.
- Serviços de transcrição.
- Serviços de armazenamento.

Os seguintes arquivos permanecerão no computador:

- áudio;
- vídeo;
- transcrição;
- banco SQLite;
- logs.

---

# 3. Stack recomendada

## Interface

- Tauri 2
- React
- TypeScript
- Vite

## Backend / Core

- Rust

## Banco de dados

- SQLite

Sugestão de biblioteca Rust:

- `rusqlite`

ou

- `sqlx` com SQLite

Para o MVP, prefira `rusqlite` por simplicidade.

## Áudio

Windows:

- WASAPI

Objetivos:

- Capturar microfone.
- Capturar áudio do sistema via WASAPI Loopback.

## Transcrição

- whisper.cpp
- Modelo Whisper multilíngue quantizado.

Modelo inicial recomendado:

- Base Multilingual Q5

Não utilizar modelos `.en`, pois o aplicativo deverá suportar português.

## Captura de tela

- Windows Graphics Capture

## Codificação de vídeo

Preferencial:

- Windows Media Foundation
- H.264 com aceleração por hardware quando disponível.

## Instalador

Tauri deverá gerar:

- `.exe` via NSIS
- opcionalmente `.msi`

---

# 4. Escopo do MVP

A versão `0.1.0` deverá conter:

- Aplicativo Windows.
- Instalador.
- Tela inicial.
- Histórico de reuniões.
- Nova gravação.
- Nome da reunião.
- Captura de microfone.
- Captura de áudio do computador.
- Iniciar gravação.
- Parar gravação.
- Armazenar áudio localmente.
- Processar transcrição após finalizar.
- Exibir progresso da transcrição.
- Salvar transcrição.
- Visualizar transcrição.
- Reproduzir áudio.
- Excluir reunião.
- Exportar transcrição em `.txt`.
- Funcionar offline.

A gravação de tela poderá entrar na versão `0.2.0`.

---

# 5. Arquitetura geral

```text
┌──────────────────────────────────────┐
│       Tauri + React + TypeScript     │
│                                      │
│  Interface                           │
│  Histórico                           │
│  Nova reunião                        │
│  Configurações                       │
│  Status / progresso                  │
└───────────────────┬──────────────────┘
                    │
              Tauri Commands
                    │
                    ▼
┌──────────────────────────────────────┐
│              Rust Core               │
│                                      │
│  MeetingManager                      │
│  AudioRecorder                       │
│  ScreenRecorder                      │
│  TranscriptionEngine                 │
│  StorageManager                      │
│  Database                            │
└──────┬─────────┬─────────┬──────────┘
       │         │         │
       ▼         ▼         ▼
    WASAPI    SQLite   whisper.cpp

       │
       ▼
Windows Graphics Capture
Media Foundation
```

---

# 6. Estrutura de pastas sugerida

```text
meeting-recorder/
│
├── src/
│   ├── app/
│   ├── components/
│   ├── pages/
│   ├── hooks/
│   ├── services/
│   ├── types/
│   └── styles/
│
├── src-tauri/
│   ├── src/
│   │   ├── audio/
│   │   │   ├── mod.rs
│   │   │   ├── microphone.rs
│   │   │   ├── loopback.rs
│   │   │   └── mixer.rs
│   │   │
│   │   ├── transcription/
│   │   │   ├── mod.rs
│   │   │   └── whisper.rs
│   │   │
│   │   ├── video/
│   │   │   ├── mod.rs
│   │   │   ├── capture.rs
│   │   │   └── encoder.rs
│   │   │
│   │   ├── database/
│   │   │   ├── mod.rs
│   │   │   ├── migrations.rs
│   │   │   └── meetings.rs
│   │   │
│   │   ├── storage/
│   │   │   ├── mod.rs
│   │   │   └── paths.rs
│   │   │
│   │   ├── meeting/
│   │   │   ├── mod.rs
│   │   │   └── manager.rs
│   │   │
│   │   ├── commands/
│   │   │   ├── mod.rs
│   │   │   ├── meeting.rs
│   │   │   └── settings.rs
│   │   │
│   │   └── main.rs
│   │
│   ├── binaries/
│   │   └── whisper/
│   │
│   ├── models/
│   │   └── ggml-base-q5_1.bin
│   │
│   └── tauri.conf.json
│
├── docs/
│   ├── architecture.md
│   ├── database.md
│   └── release.md
│
├── tests/
│
├── README.md
├── CHANGELOG.md
└── package.json
```

---

# 7. Estrutura de dados local

Diretório principal sugerido:

```text
%LOCALAPPDATA%\MeetingRecorder\
```

Estrutura:

```text
MeetingRecorder/
│
├── database/
│   └── meeting-recorder.db
│
├── recordings/
│   ├── {meeting_id}/
│   │   ├── microphone.wav
│   │   ├── system.wav
│   │   ├── merged.wav
│   │   ├── video.mp4
│   │   └── transcript.txt
│
├── models/
│
└── logs/
```

---

# 8. Modelo inicial do banco

## Tabela `meetings`

```sql
CREATE TABLE meetings (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    duration_seconds INTEGER DEFAULT 0,

    microphone_enabled INTEGER NOT NULL DEFAULT 1,
    system_audio_enabled INTEGER NOT NULL DEFAULT 1,
    video_enabled INTEGER NOT NULL DEFAULT 0,

    microphone_path TEXT,
    system_audio_path TEXT,
    merged_audio_path TEXT,
    video_path TEXT,

    transcription TEXT,
    transcription_status TEXT NOT NULL DEFAULT 'pending',
    transcription_model TEXT,
    language TEXT DEFAULT 'pt',

    status TEXT NOT NULL DEFAULT 'created',

    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

## Status sugeridos

### `status`

- `created`
- `recording`
- `processing`
- `completed`
- `failed`

### `transcription_status`

- `pending`
- `processing`
- `completed`
- `failed`

---

# 9. Roadmap de implementação

---

# FASE 0 — Preparação

## Objetivo

Criar a base do projeto sem implementar captura de áudio.

## Tarefas

### 0.1

Criar projeto Tauri 2 com React + TypeScript.

### 0.2

Configurar:

- ESLint
- Prettier
- Rustfmt
- Clippy

### 0.3

Adicionar estrutura modular.

### 0.4

Criar README inicial.

### 0.5

Criar `.gitignore`.

### 0.6

Inicializar Git.

## Critérios de aceite

- `npm run dev` inicia a interface.
- Tauri abre corretamente.
- `cargo check` não apresenta erros.
- `cargo clippy` não apresenta erros críticos.
- Projeto compila em Windows.

## Prompt sugerido para Codex

```text
Crie a estrutura inicial de um aplicativo desktop Windows usando Tauri 2, React, TypeScript e Rust.

Requisitos:
- React + TypeScript no frontend.
- Rust no backend.
- Organização modular.
- Não implementar ainda gravação de áudio.
- Criar as pastas base descritas no plano.
- Configurar ESLint, Prettier, rustfmt e clippy.
- Adicionar README com instruções para desenvolvimento.
- Garanta que o projeto compile antes de encerrar.
```

---

# FASE 1 — Interface inicial

## Objetivo

Construir a interface antes de conectar funcionalidades nativas.

## Telas

### Home

Deve conter:

- botão `Nova gravação`;
- campo de pesquisa;
- lista de reuniões;
- data;
- duração;
- status;
- indicação de áudio/vídeo.

### Nova gravação

Campos:

```text
Nome da reunião

[ Reunião de Produto ]

Áudio

[x] Microfone
[x] Áudio do computador

Vídeo

[ ] Gravar tela

[ Iniciar gravação ]
```

### Reunião

Campos:

- título;
- data;
- duração;
- player;
- transcrição;
- excluir;
- exportar.

### Configurações

Inicialmente:

- idioma;
- modelo Whisper;
- diretório das gravações.

## Critérios de aceite

- Navegação funcional.
- Nenhuma lógica nativa necessária.
- Interface responsiva.
- Estado mockado para demonstração.

## Prompt Codex

```text
Implemente apenas a interface do Meeting Recorder.

Crie:
- tela Home;
- tela Nova Gravação;
- tela de detalhes da reunião;
- tela Configurações.

Use dados mockados.
Não implemente áudio, SQLite ou Whisper ainda.

Priorize:
- interface limpa;
- poucos elementos;
- aparência de aplicativo desktop;
- componentes reutilizáveis;
- TypeScript estrito.

Ao finalizar, valide que o frontend compila.
```

---

# FASE 2 — SQLite e persistência

## Objetivo

Criar persistência local.

## Tarefas

### 2.1

Adicionar SQLite.

### 2.2

Criar inicialização automática do banco.

### 2.3

Criar tabela `meetings`.

### 2.4

Implementar:

- criar reunião;
- listar reuniões;
- buscar reunião;
- atualizar reunião;
- excluir reunião.

### 2.5

Criar comandos Tauri.

Exemplos:

```rust
create_meeting()
list_meetings()
get_meeting()
delete_meeting()
update_meeting()
```

## Critérios de aceite

- Dados continuam disponíveis após reiniciar aplicativo.
- Não guardar áudio/vídeo como BLOB.
- SQLite deve armazenar somente metadados e textos.
- Banco criado automaticamente.

## Prompt Codex

```text
Implemente persistência SQLite no backend Rust.

Use rusqlite.

Crie:
- inicialização do banco;
- migration da tabela meetings;
- repository de reuniões;
- comandos Tauri para CRUD.

O banco deve ficar no diretório de dados locais da aplicação.

Não armazene arquivos de áudio ou vídeo no banco.
Armazene apenas os caminhos.

Crie testes para as operações CRUD.
Execute os testes e corrija os problemas antes de finalizar.
```

---

# FASE 3 — Storage Manager

## Objetivo

Padronizar onde arquivos são gravados.

## Tarefas

Criar automaticamente:

```text
%LOCALAPPDATA%\MeetingRecorder\
```

e:

```text
database/
recordings/
models/
logs/
```

Cada reunião:

```text
recordings/{meeting_id}/
```

## Funções

```rust
create_meeting_directory()
get_microphone_path()
get_system_audio_path()
get_merged_audio_path()
get_video_path()
get_transcript_path()
delete_meeting_files()
```

## Critérios de aceite

- Nenhum caminho absoluto hardcoded.
- Funcionar para qualquer usuário Windows.
- Exclusão de reunião remove arquivos associados.

---

# FASE 4 — Captura de microfone

## Objetivo

Gravar apenas o microfone.

## Requisitos

- WASAPI.
- Listar dispositivos.
- Permitir selecionar entrada.
- Iniciar.
- Pausar não é necessário no MVP.
- Parar.
- Persistir arquivo.

Formato temporário recomendado:

```text
PCM WAV
Mono
16 kHz ou 48 kHz
16-bit
```

Inicialmente é aceitável armazenar WAV.

Depois poderá ser comprimido.

## Critérios de aceite

- Gravar 30 minutos sem erro.
- Arquivo reproduzível.
- Não travar interface.
- Uso de CPU baixo.
- Parar corretamente.
- Nenhum arquivo corrompido.

## Prompt Codex

```text
Implemente captura de microfone no backend Rust para Windows utilizando WASAPI.

Requisitos:
- listar dispositivos de entrada;
- selecionar dispositivo;
- iniciar gravação;
- parar gravação;
- salvar em WAV;
- execução fora da thread principal;
- comunicação de status com o frontend;
- tratamento de erro caso dispositivo seja removido.

Não implemente áudio do sistema ainda.

Crie um teste manual documentado e valide uma gravação mínima.
```

---

# FASE 5 — Captura do áudio do sistema

## Objetivo

Capturar o som reproduzido pelo Windows.

## Tecnologia

WASAPI Loopback.

## Requisitos

- Listar dispositivos de saída.
- Selecionar dispositivo.
- Capturar áudio do computador.
- Rodar simultaneamente ao microfone.
- Salvar separadamente.

Estrutura:

```text
microphone.wav
system.wav
```

## Critérios de aceite

Testar:

- YouTube;
- Google Meet;
- Microsoft Teams;
- áudio do navegador.

Os dois arquivos devem:

- iniciar praticamente juntos;
- manter duração semelhante;
- não apresentar perda de áudio.

## Prompt Codex

```text
Implemente WASAPI Loopback no Meeting Recorder.

Objetivo:
capturar o áudio reproduzido pelo dispositivo de saída do Windows.

Requisitos:
- listar dispositivos de saída;
- selecionar dispositivo;
- iniciar/parar captura;
- salvar system.wav;
- permitir execução simultânea com microphone.wav;
- evitar bloqueio da UI;
- registrar erros em log.

Não implemente mixagem ainda.
```

---

# FASE 6 — Meeting Manager

## Objetivo

Criar uma única camada que controle a reunião.

## Responsabilidades

```text
start_meeting()
stop_meeting()
get_recording_state()
```

Ao iniciar:

1. Criar registro no banco.
2. Criar pasta.
3. Iniciar microfone.
4. Iniciar loopback.
5. Atualizar status para `recording`.

Ao encerrar:

1. Parar áudio.
2. Atualizar duração.
3. Atualizar status.
4. Preparar transcrição.

## Critérios de aceite

O frontend não deve controlar diretamente WASAPI.

Fluxo:

```text
Frontend
   ↓
MeetingManager
   ↓
AudioRecorder
```

---

# FASE 7 — Preparação do áudio para Whisper

## Objetivo

Gerar um arquivo compatível com Whisper.

Formato recomendado:

```text
PCM
16 kHz
Mono
16-bit
```

Arquivo:

```text
merged.wav
```

## Estratégia

Combinar:

```text
microphone.wav
+
system.wav
```

O resultado não deverá clipar.

Aplicar normalização simples se necessário.

## Critérios de aceite

- merged.wav reproduz ambos os lados.
- sincronização aceitável.
- formato suportado por whisper.cpp.

---

# FASE 8 — whisper.cpp

## Objetivo

Transcrição 100% local.

## Modelo

Inicial:

```text
Whisper Base Multilingual Q5
```

## Estrutura

```text
TranscriptionEngine
│
├── initialize()
├── transcribe()
├── get_progress()
└── cancel()
```

## Fluxo

```text
stop meeting
      ↓
prepare merged.wav
      ↓
status = processing
      ↓
whisper.cpp
      ↓
transcription
      ↓
SQLite
      ↓
status = completed
```

## Requisitos

- Não executar durante a gravação.
- Permitir cancelar.
- Exibir progresso.
- Não congelar UI.
- Configurar número de threads.
- Idioma padrão `pt`.

## Critérios de aceite

Testar com:

- reunião de 5 minutos;
- reunião de 30 minutos;
- português;
- ruído moderado;
- múltiplas vozes.

## Prompt Codex

```text
Integre whisper.cpp ao backend Rust do Meeting Recorder.

Requisitos:
- execução completamente local;
- utilizar modelo Base Multilingual Q5;
- idioma padrão português;
- iniciar apenas após a gravação terminar;
- expor progresso para o frontend;
- permitir cancelamento;
- salvar transcrição no SQLite;
- salvar também transcript.txt;
- limitar threads para evitar uso excessivo do computador.

Não implemente resumo por IA.
```

---

# FASE 9 — Player e transcrição

## Objetivo

Exibir reunião finalizada.

Tela:

```text
Reunião Produto
29/09/2026
52 min

[ ▶ áudio ]

Transcrição

00:00
Matheus:
...

00:15
Participante:
...
```

No MVP não é obrigatório identificar participantes.

Pode ser:

```text
00:00
Bom dia pessoal...
```

## Recursos

- buscar texto;
- copiar;
- exportar TXT;
- abrir pasta da reunião.

---

# FASE 9.1 — Diarização de participantes

Extensão autorizada explicitamente em 02/10/2026, após a Fase 9. Executar somente após finalizar a reunião, sem identificação real de pessoas.

- Preservar Whisper tradicional de `merged.wav` no SQLite e em `transcript.txt` antes da identificação.
- Preparar fontes individuais em PCM mono 16-bit/16 kHz e obter palavras/timestamps com whisper.cpp.
- Identificar o microfone como `Você`; processar somente o sistema com sherpa-onnx nativo CPU (pyannote 3.0 int8 + WeSpeaker ResNet34 LM).
- Alinhar palavras/turnos na linha do tempo da reunião; usar `Participante N` e `Participantes` em trechos incertos/sobrepostos.
- Migration aditiva para segmentos e estado independente da diarização. Sem embeddings individuais persistidos.
- Worker Rust com progresso, cancelamento, limite de threads e exclusão entre captura e processamento.
- Exibir segmentos, pesquisa/cópia/TXT na tela existente. Falhas e reuniões antigas conservam o texto tradicional; processamento de antigas somente por ação explícita.

Aceite: testes de uma/duas/três vozes, local+remoto, alternância, pequena sobreposição, falha e reunião antiga; persistência, cancelamento, fmt/clippy/test/build. Procedimentos e limites: [DIARIZATION_TESTING.md](DIARIZATION_TESTING.md). Sem Fase 9.2 ou avanço para tray.

---

# FASE 10 — System Tray

## Objetivo

Permitir minimizar o aplicativo.

Tray:

```text
Meeting Recorder

● Gravando 00:32:14

Abrir
Finalizar gravação
Sair
```

## Requisitos

- fechar janela não deve necessariamente encerrar aplicação;
- impedir encerramento acidental durante gravação;
- mostrar estado.

---

# FASE 11 — Gravação de tela

Versão alvo:

```text
0.2.0
```

## Objetivo

Gravação opcional.

## Fontes

- monitor;
- janela.

Não implementar captura de aba específica do navegador inicialmente.

## Tecnologia

```text
Windows Graphics Capture
```

## Fluxo

```text
Selecionar janela/monitor
        ↓
Windows Graphics Capture
        ↓
frames
        ↓
Media Foundation
        ↓
H264
        ↓
video.mp4
```

## Configurações

```text
Gravar tela

[ ] Desativado

Caso ativado:

Fonte:
[ Chrome - Google Meet ▼ ]

Qualidade:
[ 720p ▼ ]

FPS:
[ 15 ▼ ]
```

### Padrão recomendado

Para reuniões:

```text
720p
15 FPS
H.264
```

Isso é suficiente para slides, sistemas e compartilhamento de tela sem gerar arquivos enormes.

## Critérios de aceite

- Capturar janela do Chrome.
- Capturar monitor.
- Continuar mesmo com app minimizado.
- Baixo uso de CPU.
- Usar hardware encoder quando disponível.
- Vídeo sincronizado de forma aceitável com áudio.

---

# FASE 12 — Configurações

Adicionar:

## Áudio

- microfone padrão;
- saída padrão.

## Transcrição

- modelo;
- idioma;
- quantidade máxima de threads.

## Vídeo

- resolução;
- FPS.

## Armazenamento

- diretório;
- retenção opcional futura.

---

# FASE 13 — Exportação

MVP:

- `.txt`

Depois:

- `.srt`
- `.vtt`
- `.json`

## SRT

Estrutura:

```text
1
00:00:00,000 --> 00:00:05,000
Bom dia pessoal.
```

---

# FASE 14 — Logs

Criar logs locais.

Exemplo:

```text
logs/meeting-recorder.log
```

Registrar:

- inicialização;
- dispositivos;
- início de gravação;
- parada;
- erros WASAPI;
- início/fim de transcrição;
- erro Whisper;
- erro de arquivos.

Não registrar o conteúdo da transcrição nos logs.

---

# FASE 15 — Testes

## Unitários

Testar:

- repository SQLite;
- storage;
- path handling;
- cálculo de duração;
- status.

## Integração

Testar:

```text
start
↓
record
↓
stop
↓
prepare audio
↓
transcribe
↓
save
```

## Testes manuais obrigatórios

### Caso 1

Reunião 5 minutos.

### Caso 2

Reunião 60 minutos.

### Caso 3

Microfone desconectado.

### Caso 4

Fone Bluetooth desconectado.

### Caso 5

Aplicativo minimizado.

### Caso 6

Fechar janela durante gravação.

### Caso 7

Pouco espaço em disco.

### Caso 8

Transcrição cancelada.

### Caso 9

Reiniciar app com transcrição pendente.

### Caso 10

Gravação de tela com Chrome.

---

# 10. Otimização

Nunca otimizar prematuramente.

Medir:

- CPU;
- RAM;
- tamanho de arquivo;
- duração;
- tempo de transcrição.

Objetivo durante gravação sem vídeo:

```text
CPU: baixo
RAM: idealmente < 200 MB
```

Esses valores são metas, não garantias.

Com vídeo:

o consumo dependerá do encoder e GPU.

---

# 11. Gerenciamento de erros

Nenhuma falha deverá destruir a reunião inteira.

Exemplo:

Se microfone falhar:

```text
microphone = failed
system audio = recording
```

Continuar o que for possível.

Salvar estado no banco.

Exemplo:

```text
status = failed_partial
```

---

# 12. Recuperação após crash

Implementar posteriormente.

Ao abrir o aplicativo:

```text
SELECT *
FROM meetings
WHERE status = 'recording'
```

Caso exista:

```text
Encontramos uma gravação interrompida.

[ Recuperar ]
[ Excluir ]
```

---

# 13. Instalador

## Objetivo

Usuário deverá receber:

```text
MeetingRecorder-Setup.exe
```

E poder instalar sem:

- Node;
- Rust;
- Python;
- Git;
- Whisper manual;
- banco manual.

## Conteúdo do pacote

```text
MeetingRecorder.exe
Whisper runtime
Modelo Whisper
Dependências necessárias
```

## Tauri

Configurar:

```text
bundle.targets
```

para:

- NSIS
- MSI opcional.

## Instalador NSIS

Desejado:

```text
Meeting Recorder

[ Instalar ]

Destino:
C:\Users\<USER>\AppData\Local\Programs\MeetingRecorder
```

Opções:

```text
[x] Criar atalho
[x] Menu iniciar
```

---

# 14. Modelo Whisper no instalador

Existem duas opções.

## Opção A — Embutido

Instalador inclui:

```text
ggml-base-q5_1.bin
```

Vantagem:

- offline desde a instalação.

Desvantagem:

- instalador maior.

### Escolha para o MVP

Usar modelo embutido.

---

# 15. Build de produção

## Frontend

```bash
npm run build
```

## Rust

```bash
cargo build --release
```

## Tauri

```bash
npm run tauri build
```

Resultado esperado:

```text
src-tauri/target/release/bundle/
```

com:

```text
nsis/
msi/
```

---

# 16. Versionamento

Usar SemVer.

Exemplo:

```text
0.1.0
```

Primeira versão funcional:

- áudio;
- transcrição;
- histórico.

```text
0.2.0
```

Adicionar gravação de tela.

```text
0.3.0
```

Adicionar melhorias avançadas.

---

# 17. Git

Sugestão de branches:

```text
main
develop
feature/audio-microphone
feature/audio-loopback
feature/whisper
feature/screen-capture
```

Commits pequenos.

Exemplo:

```text
feat(audio): add microphone capture
feat(audio): add WASAPI loopback
feat(db): create meetings repository
feat(transcription): integrate whisper.cpp
```

---

# 18. Estratégia recomendada para uso do Codex

Não pedir ao Codex:

```text
Crie todo o Meeting Recorder.
```

Isso aumenta muito o risco de arquitetura ruim.

Sempre trabalhar em tarefas pequenas.

## Boa tarefa

```text
Implemente apenas o StorageManager.

Antes de alterar código:
1. analise a estrutura atual;
2. descreva quais arquivos serão modificados;
3. implemente;
4. execute testes;
5. execute cargo clippy;
6. explique o que mudou.
```

---

# 19. Prompt padrão para todas as tarefas

Utilizar este prefixo:

```text
Você está trabalhando no projeto Meeting Recorder.

Antes de implementar:

1. Leia README.md.
2. Leia docs/architecture.md.
3. Analise o código existente.
4. Não reescreva módulos sem necessidade.
5. Faça a menor alteração possível para atender a tarefa.
6. Não adicione dependências sem justificar.
7. Não quebre APIs existentes.
8. Adicione testes quando aplicável.
9. Execute os testes relevantes.
10. Execute cargo fmt e cargo clippy.
11. Execute o build do frontend quando alterar React.
12. Ao finalizar, informe:
   - arquivos alterados;
   - decisões tomadas;
   - testes executados;
   - riscos ou pendências.

Tarefa:

[DESCREVER TAREFA]
```

---

# 20. Checkpoints obrigatórios

Não avançar para a próxima fase se a anterior estiver instável.

## Checkpoint A

Depois da Fase 3:

```text
✓ App
✓ UI
✓ SQLite
✓ Storage
```

Criar tag:

```text
v0.0.1
```

## Checkpoint B

Depois da Fase 6:

```text
✓ Microfone
✓ Áudio do sistema
✓ Meeting Manager
```

Tag:

```text
v0.0.2
```

## Checkpoint C

Depois da Fase 8:

```text
✓ Whisper
✓ Transcrição
```

Tag:

```text
v0.0.3
```

## Checkpoint D

Primeiro instalador funcional:

```text
v0.1.0
```

## Checkpoint E

Tela:

```text
v0.2.0
```

---

# 21. Definition of Done

Uma tarefa só estará pronta quando:

- código implementado;
- aplicação compilar;
- sem erros novos do Clippy;
- testes relevantes passando;
- cenário manual testado quando necessário;
- erros tratados;
- nenhuma dependência desnecessária;
- documentação atualizada quando necessário.

---

# 22. Definition of Done — MVP 0.1.0

O MVP estará pronto quando for possível:

```text
instalar
↓
abrir
↓
Nova gravação
↓
nomear reunião
↓
gravar microfone + sistema
↓
encerrar
↓
transcrever localmente
↓
visualizar texto
↓
fechar aplicativo
↓
abrir novamente
↓
reunião continua no histórico
```

Também:

- nenhuma internet necessária;
- nenhuma conta;
- nenhum serviço externo;
- instalador funcional;
- desinstalação funcional;
- gravação de uma hora estável.

---

# 23. Definition of Done — 0.2.0

Fluxo adicional:

```text
Nova gravação
↓
Ativar "Gravar tela"
↓
Selecionar janela do Chrome
↓
Iniciar
↓
Áudio + vídeo
↓
Finalizar
↓
MP4 reproduzível
↓
Transcrição local
```

---

# 24. Funcionalidades fora do MVP

Não desenvolver inicialmente:

- identificação automática perfeita de participantes;
- resumos com IA;
- chatbot sobre reunião;
- sincronização em nuvem;
- login;
- contas de equipe;
- compartilhamento;
- calendário;
- integração Meet;
- integração Teams;
- extensão Chrome;
- transcrição em tempo real;
- edição avançada de vídeo.

Esses recursos devem ser avaliados somente após estabilidade do core.

---

# 25. Possíveis evoluções futuras

## 0.3.x

- identificação avançada de participantes (a identificação simples foi autorizada na Fase 9.1);
- SRT;
- VTT;
- busca avançada.

## 0.4.x

Modelo LLM local para:

- resumo;
- decisões;
- tarefas;
- próximos passos.

## 0.5.x

Extensão auxiliar Chrome.

Possibilidades:

- detectar reuniões;
- identificar nome da aba;
- iniciar gravação;
- contexto da reunião.

## 1.0

Versão estável para distribuição geral.

---

# 26. Ordem recomendada para iniciar no Codex

Executar exatamente nesta sequência:

```text
1. Estrutura Tauri
2. UI
3. SQLite
4. StorageManager
5. Microfone
6. WASAPI Loopback
7. MeetingManager
8. Preparação/mixagem de áudio
9. whisper.cpp
10. Player/transcrição
11. Tray
12. Instalador
13. Testes longos
14. Release 0.1.0
15. Windows Graphics Capture
16. Encoder H264
17. Release 0.2.0
```

---

# 27. Primeira tarefa para enviar ao Codex

```text
Quero iniciar o desenvolvimento do Meeting Recorder.

Objetivo:
Aplicativo desktop Windows 100% local para gravação e posterior transcrição de reuniões.

Stack obrigatória:
- Tauri 2
- React
- TypeScript
- Rust

Arquitetura futura:
- SQLite
- WASAPI para microfone e áudio do sistema
- whisper.cpp
- Windows Graphics Capture
- Media Foundation
- instalador NSIS/MSI

Nesta primeira tarefa NÃO implemente áudio, banco, Whisper ou vídeo.

Faça somente:

1. crie o projeto;
2. configure Tauri 2 + React + TypeScript;
3. crie uma arquitetura de pastas modular;
4. configure ESLint e Prettier;
5. configure rustfmt/clippy;
6. crie um README inicial;
7. crie docs/architecture.md;
8. implemente uma tela inicial mínima;
9. execute os builds e validações possíveis;
10. corrija erros antes de concluir.

Não adicione funcionalidades que não foram solicitadas.

Ao finalizar, apresente:
- estrutura criada;
- arquivos principais;
- comandos para executar o projeto;
- testes/builds realizados.
```

---

# 28. Observação importante sobre consentimento

A aplicação deverá deixar claro que está gravando.

Sugestão visual:

```text
🔴 Gravando reunião
```

É recomendável que o usuário responsável pelo aplicativo obtenha o consentimento adequado dos participantes conforme políticas internas, contratos e legislação aplicável.

---

# 29. Prioridade arquitetural

Quando houver dúvida entre:

```text
mais funcionalidades
```

e:

```text
mais estabilidade
```

escolher:

```text
mais estabilidade
```

O núcleo do produto é:

```text
CAPTURAR
↓
NÃO PERDER
↓
SALVAR
↓
TRANSCREVER
```

Todo o restante é secundário.

---

# 30. Resumo final

Arquitetura alvo:

```text
React / TypeScript
        ↓
      Tauri
        ↓
       Rust
  ┌─────┼───────────┐
  ↓     ↓           ↓
WASAPI SQLite whisper.cpp
  ↓
Windows Graphics Capture
  ↓
Media Foundation
```

Distribuição:

```text
MeetingRecorder-Setup.exe
```

Funcionamento:

```text
100% LOCAL
100% OFFLINE
SEM LOGIN
SEM CLOUD
SEM IA DURANTE A REUNIÃO
```

Primeiro objetivo:

```text
Meeting Recorder 0.1.0
Áudio + Transcrição + Histórico + Instalador
```

Segundo objetivo:

```text
Meeting Recorder 0.2.0
Gravação opcional de janela/monitor
```
