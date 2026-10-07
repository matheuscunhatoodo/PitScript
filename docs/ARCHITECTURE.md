# Meeting Recorder — Architecture

## 1. Visão geral

O Meeting Recorder é um aplicativo desktop Windows 10/11 destinado a gravar reuniões localmente e gerar transcrições offline após o encerramento da gravação.

A arquitetura foi escolhida com quatro prioridades:

1. baixo consumo durante a reunião;
2. privacidade e funcionamento offline;
3. confiabilidade da captura;
4. facilidade de distribuição.

A aplicação não deve depender de serviços externos para seu funcionamento principal.

---

# 2. Objetivos arquiteturais

## 2.1. Leve durante a reunião

Enquanto uma reunião estiver sendo gravada, o aplicativo deverá executar apenas o necessário para:

- capturar microfone;
- capturar áudio do sistema;
- opcionalmente capturar vídeo;
- gravar arquivos;
- atualizar estado mínimo da interface.

Não executar:

- Whisper;
- LLM;
- sumarização;
- diarização pesada;
- pós-processamento intensivo.

---

## 2.2. Processamento após a reunião

Fluxo principal:

```text
INICIAR REUNIÃO
      ↓
CAPTURAR ÁUDIO
      ↓
CAPTURAR VÍDEO OPCIONAL
      ↓
SALVAR LOCALMENTE
      ↓
FINALIZAR REUNIÃO
      ↓
PREPARAR ÁUDIO
      ↓
WHISPER.CPP
      ↓
SALVAR TRANSCRIÇÃO
```

---

# 3. Stack

## Frontend

```text
Tauri 2
React
TypeScript
Vite
```

O frontend deve se limitar a:

- UI;
- navegação;
- estado de apresentação;
- chamadas aos comandos Tauri;
- exibição de progresso.

---

## Backend

```text
Rust
```

Rust será responsável por:

- captura nativa;
- ciclo da reunião;
- banco;
- arquivos;
- transcrição;
- gerenciamento de recursos.

---

## Banco

```text
SQLite
```

Biblioteca inicial:

```text
rusqlite
```

O banco armazenará:

- metadados;
- status;
- caminhos;
- transcrição;
- configurações.

Não armazenará áudio ou vídeo como BLOB.

---

# 4. Diagrama geral

```text
┌─────────────────────────────────────────────┐
│              React / TypeScript             │
│                                             │
│ Home                                        │
│ Nova gravação                               │
│ Detalhe da reunião                          │
│ Configurações                               │
│ Tray / Status                               │
└─────────────────────┬───────────────────────┘
                      │
                Tauri Commands
                      │
                      ▼
┌─────────────────────────────────────────────┐
│                  Rust Core                  │
│                                             │
│ MeetingManager                              │
│ StorageManager                              │
│ Database                                    │
│ AudioRecorder                               │
│ TranscriptionEngine                         │
│ ScreenRecorder                              │
└────────┬──────────┬──────────┬─────────────┘
         │          │          │
         ▼          ▼          ▼
      WASAPI      SQLite    whisper.cpp
         │
         ▼
 Windows Graphics Capture
         │
         ▼
 Media Foundation / H.264
```

---

# 5. Responsabilidades dos módulos

## 5.1. `meeting`

É a camada de orquestração.

Não deve implementar detalhes de WASAPI, SQLite ou Whisper.

Interface conceitual:

```rust
start_meeting(...)
stop_meeting(...)
get_recording_state(...)
```

Ao iniciar:

```text
1. validar configuração;
2. criar ID;
3. criar pasta;
4. criar registro no banco;
5. iniciar microfone;
6. iniciar loopback;
7. iniciar vídeo se habilitado;
8. atualizar status.
```

Ao finalizar:

```text
1. parar captura;
2. garantir flush dos arquivos;
3. calcular duração;
4. atualizar banco;
5. preparar áudio;
6. iniciar transcrição;
```

---

## 5.2. `audio`

Submódulos sugeridos:

```text
audio/
├── microphone.rs
├── loopback.rs
├── mixer.rs
└── mod.rs
```

### Microfone

Tecnologia:

```text
WASAPI Capture
```

Responsabilidades:

- enumerar dispositivos;
- iniciar;
- parar;
- informar erro;
- persistir áudio.

### Sistema

Tecnologia:

```text
WASAPI Loopback
```

Responsabilidades:

- enumerar dispositivos de saída;
- capturar áudio reproduzido;
- persistir separadamente.

### Mixer

Usado após a reunião.

Entrada:

```text
microphone.wav
system.wav
```

Saída:

```text
merged.wav
```

Formato para Whisper:

```text
PCM
16 kHz
mono
16-bit
```

---

# 6. Estratégia de áudio

Durante o MVP, manter as fontes separadas:

```text
microphone.wav
system.wav
```

Vantagens:

- depuração;
- recuperação;
- diagnóstico;
- possibilidade de processar fontes separadamente no futuro.

Após finalizar:

```text
microphone.wav
+
system.wav
↓
merged.wav
```

O `merged.wav` será utilizado pelo Whisper.

---

# 7. Transcrição

## Engine

```text
whisper.cpp
```

Modelo padrão:

```text
Whisper Base Multilingual Q5
```

Idioma inicial:

```text
pt
```

O modelo deve ser multilíngue.

Nunca utilizar uma variante `.en` como modelo padrão.

---

## Ciclo da transcrição

```text
meeting stopped
↓
merged.wav ready
↓
transcription_status = processing
↓
whisper.cpp
↓
progress events
↓
text result
↓
SQLite
↓
transcript.txt
↓
transcription_status = completed
```

---

## Threads

O número de threads deve ser configurável.

O aplicativo deve evitar consumir 100% da CPU por padrão.

Objetivo:

- manter o computador responsivo;
- permitir configuração futura entre velocidade e consumo.

---

# 8. Vídeo

Vídeo é opcional.

Se desativado, nenhum pipeline de vídeo deve permanecer ativo.

Tecnologia:

```text
Windows Graphics Capture
```

Fontes:

- monitor;
- janela.

Não implementar captura de aba do navegador no MVP.

---

## Encoder

Preferência:

```text
Media Foundation
H.264
```

Priorizar hardware encoder quando disponível.

Configuração inicial sugerida para reuniões:

```text
720p
15 FPS
H.264
```

Razão:

- leitura de telas;
- slides;
- compartilhamento;
- tamanho moderado;
- menor custo que 1080p/30.

---

# 9. Persistência

## Diretório

Utilizar diretório local apropriado do Windows.

Estrutura conceitual:

```text
%LOCALAPPDATA%\MeetingRecorder\
```

Conteúdo:

```text
database/
recordings/
models/
logs/
```

---

## Pasta da reunião

```text
recordings/{meeting_id}/
```

Arquivos possíveis:

```text
microphone.wav
system.wav
merged.wav
video.mp4
transcript.txt
```

---

# 10. SQLite

Tabela inicial:

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

---

# 11. Estados

## Meeting

```text
created
recording
processing
completed
failed
failed_partial
```

## Transcrição

```text
pending
processing
completed
failed
cancelled
```

---

# 12. Política de falhas

A aplicação deve favorecer recuperação.

Exemplo:

```text
microfone falhou
system audio continua
```

Resultado esperado:

```text
não interromper o que continua válido;
registrar falha;
persistir arquivo existente;
atualizar status para failed_partial.
```

Nunca apagar automaticamente dados válidos por causa de uma falha parcial.

---

# 13. Concorrência

Operações que não podem bloquear a UI:

- WASAPI;
- gravação de arquivos;
- transcrição;
- captura de vídeo;
- encoding.

Essas tarefas devem utilizar threads/tasks adequadas no backend.

O frontend deve receber apenas:

- estado;
- progresso;
- eventos;
- erros sanitizados.

---

# 14. Comunicação frontend/backend

O frontend não deve acessar WASAPI, banco ou arquivos diretamente.

Exemplos de comandos:

```text
create_meeting
start_meeting
stop_meeting
list_meetings
get_meeting
delete_meeting
list_input_devices
list_output_devices
transcribe_meeting
cancel_transcription
export_transcript
```

Eventos:

```text
recording-status
recording-duration
transcription-progress
transcription-finished
recording-error
```

---

# 15. Estrutura do backend

```text
src-tauri/src/
│
├── audio/
│   ├── mod.rs
│   ├── microphone.rs
│   ├── loopback.rs
│   └── mixer.rs
│
├── transcription/
│   ├── mod.rs
│   └── whisper.rs
│
├── video/
│   ├── mod.rs
│   ├── capture.rs
│   └── encoder.rs
│
├── database/
│   ├── mod.rs
│   ├── migrations.rs
│   └── meetings.rs
│
├── storage/
│   ├── mod.rs
│   └── paths.rs
│
├── meeting/
│   ├── mod.rs
│   └── manager.rs
│
├── commands/
│   ├── mod.rs
│   ├── meeting.rs
│   └── settings.rs
│
└── main.rs
```

---

# 16. Estrutura do frontend

```text
src/
│
├── app/
├── components/
├── pages/
├── hooks/
├── services/
├── types/
└── styles/
```

`services/` deverá centralizar chamadas Tauri.

Evitar espalhar `invoke()` por componentes.

Exemplo:

```text
services/meeting.ts
services/devices.ts
services/settings.ts
```

---

# 17. Tela Home

Responsabilidades:

- listar reuniões;
- buscar;
- mostrar status;
- iniciar nova gravação.

Não executar consultas SQL diretamente.

---

# 18. Tela Nova Gravação

Campos iniciais:

```text
Nome da reunião

Microfone
[x]

Áudio do computador
[x]

Gravar tela
[ ]
```

Vídeo permanece opcional.

---

# 19. Tela da reunião

Recursos MVP:

- título;
- data;
- duração;
- player de áudio;
- transcrição;
- copiar;
- exportar TXT;
- abrir pasta;
- excluir.

---

# 20. System Tray

Requisitos:

- mostrar estado;
- abrir app;
- finalizar gravação;
- sair.

Se estiver gravando:

- impedir saída acidental;
- solicitar ação segura antes de encerrar processo.

---

# 21. Logs

Arquivo:

```text
logs/meeting-recorder.log
```

Registrar:

- inicialização;
- dispositivos;
- início/parada;
- erros de captura;
- transcrição;
- erros de arquivo.

Não registrar:

- transcrição completa;
- áudio;
- informação sensível desnecessária.

A Fase 14 centraliza a escrita em `storage/diagnostics.rs`, usando somente a biblioteca padrão Rust. O StorageManager mantém a localização original dos logs mesmo quando o diretório de gravações muda. Cada linha UTF-8 contém timestamp Unix em milissegundos, nível, evento e campos técnicos restritos. Identificadores de dispositivos/reuniões aparecem como tokens de correlação; nomes, títulos, caminhos, payloads de panic e saídas dos processos de inferência não são gravados. Erros incluem etapa, categoria e código numérico quando disponível.

A escrita é serializada entre as threads do processo. Rotação mantém o arquivo atual de até 1 MiB e dois backups (`.log.1`, `.log.2`). Logs antigos acima do limite são truncados na próxima escrita; gravações e banco não são afetados. Falhas do logger não retornam ao pipeline: há uma tentativa de aviso sanitizado em stderr, também sem propagação de erro. São registrados eventos de ciclo de vida, não cada frame/pacote nem progresso contínuo. Consulte [LOGGING_TESTING.md](LOGGING_TESTING.md).

---

# 22. Instalador

Produto final:

```text
MeetingRecorder-Setup.exe
```

Gerar via Tauri/NSIS.

Opcional:

```text
.msi
```

O usuário final não deve precisar instalar:

- Node;
- npm;
- Rust;
- Cargo;
- Python;
- Whisper manualmente.

---

# 23. Modelo Whisper

Para comportamento totalmente offline, incluir o modelo no pacote ou instalar junto com o aplicativo.

Escolha do MVP:

```text
modelo incluso
```

Trade-off:

```text
instalador maior
```

em troca de:

```text
funcionamento offline imediato
```

---

# 24. Dependências e binários auxiliares

Antes de empacotar:

- validar licenças;
- validar caminhos;
- validar runtime necessário;
- garantir que o instalador inclua tudo.

O aplicativo instalado deve abrir em uma máquina Windows limpa sem ambiente de desenvolvimento.

---

# 25. Testes obrigatórios

## Unidade

- database;
- storage;
- paths;
- status;
- duração.

## Integração

Fluxo:

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
persist
```

## Manual

Testar no mínimo:

1. 5 minutos;
2. 60 minutos;
3. microfone removido;
4. dispositivo de saída alterado;
5. app minimizado;
6. fechamento durante gravação;
7. pouco disco;
8. cancelamento da transcrição;
9. reabertura do app;
10. gravação de janela do Chrome.

---

# 26. Performance

Não assumir que algo é leve.

Medir:

- CPU;
- RAM;
- I/O;
- tamanho dos arquivos;
- tempo de transcrição.

Meta inicial durante áudio sem vídeo:

```text
RAM idealmente abaixo de 200 MB
CPU baixa e estável
```

É uma meta, não contrato.

---

# 27. Segurança e privacidade

Não enviar conteúdo para fora da máquina.

O aplicativo deve apresentar indicação clara durante a gravação:

```text
🔴 Gravando
```

Também deve existir documentação sobre consentimento e responsabilidade de uso.

---

# 28. Não objetivos do MVP

Não adicionar inicialmente:

- nuvem;
- login;
- times;
- colaboração;
- extensão Chrome;
- resumo por IA;
- LLM;
- diarização sofisticada;
- transcrição em tempo real;
- integração automática com Meet/Teams;
- editor de vídeo.

---

# 29. Versões

## 0.1.0

```text
Áudio
+
Transcrição
+
Histórico
+
Instalador
```

## 0.2.0

```text
Gravação opcional de janela/monitor
```

## Futuro

- identificação avançada de participantes (nomes/biometria continuam fora do escopo);
- SRT/VTT;
- resumo local;
- extensão auxiliar.

---

# 30. Princípio de decisão

Se houver conflito entre:

```text
mais funcionalidades
```

e:

```text
mais estabilidade
```

priorizar:

```text
estabilidade
```

O produto deve ser confiável antes de ser sofisticado.

---

# 31. Extensão Fase 9.1 — locutores por reunião

Autorizada em 02/10/2026. A identificação simples de grupos de locutores passa a existir após a gravação; diarização sofisticada, identidade real e cadastro de voz continuam fora do escopo.

O `MeetingManager` conserva a exclusão global entre captura e pós-processamento. Um worker Rust primeiro salva a transcrição tradicional de `merged.wav` com whisper.cpp (modelo Base Multilingual Q5, idioma padrão `pt`). Em seguida prepara cópias individuais PCM mono 16-bit/16 kHz e transcreve cada fonte com timestamps. Isso permite atribuir exclusivamente a fonte de microfone a `Você`, sem inferir identidade do usuário ou confundir sobreposição entre microfone e sistema.

O `system.wav` preparado passa pela CLI nativa CPU sherpa-onnx v1.13.8: segmentação pyannote 3.0 int8 e WeSpeaker ResNet34 LM, clustering automático. Embeddings são intermediários somente em memória do processo, nunca persistidos. Os binários/modelos são recursos locais incluídos no bundle; nenhum download ou serviço de inferência é usado em runtime. A execução tem no máximo quatro threads ou metade das CPUs disponíveis, conforme o limite existente, e o cancelamento encerra o processo auxiliar.

O alinhamento usa tempos das palavras e dos turnos. Rótulos `Participante N` são numerados por primeira ocorrência dentro da reunião. Cobertura temporal insuficiente, confiança baixa/indisponível, timestamps imprecisos ou sobreposição relevante entre locutores usam `Participantes`. Confiança do clustering é heurística, não probabilidade calibrada. Microfone é uma regra de origem: não distingue pessoas fisicamente próximas ou áudio de caixas captado pelo microfone.

Schema SQLite v3 adiciona `transcript_segments` (texto, rótulo, fonte, timestamps em ms, confiança opcional) e `meeting_diarization` (status/etapa/progresso/erro/modelo). Substituição de segmentos e conclusão são uma única transação. Exclusão da reunião usa FK em cascata. Os contratos de `meetings` e comandos anteriores permanecem compatíveis.

O estado de diarização é independente do sucesso da transcrição. Falha/cancelamento/interrupção mantêm o SQLite/TXT tradicional e as gravações válidas. `transcript.txt` permanece como texto tradicional de segurança; copiar/exportar TXT usa segmentos formatados quando disponíveis, com opção de selecionar texto tradicional na tela. O argumento opcional `traditional` no comando de exportação preserva chamadas anteriores. Sem segmentos, a tela e a exportação usam o texto anterior. Os comandos adicionais `get_meeting_diarization` e `diarize_meeting` consultam resultados e iniciam identificação explícita de reuniões antigas finalizadas. Abrir o detalhe não dispara inferência.

Cópias WAV/JSON temporárias ficam em uma pasta única `postprocess-{pid}-{nonce}` dentro da reunião e são removidas ao finalizar, falhar ou cancelar normalmente. Encerramento abrupto do processo pode deixar essa pasta; os originais continuam intactos. Não há Python/PyTorch, processamento durante captura nem embeddings persistentes. Consulte [DIARIZATION_TESTING.md](DIARIZATION_TESTING.md) para recursos, fontes, testes e limitações.

---

# 32. Configurações — Fase 12

As oito preferências de áudio, transcrição, vídeo e diretório de gravações ficam no SQLite local, schema v4 (`app_settings`). O módulo `settings/` contém defaults e validação; `get_settings`/`save_settings` passam pelo MeetingManager. Salvar é bloqueado enquanto há gravação ou pós-processamento. Padrões: dispositivos Windows, Base Multilingual Q5, português, metade das CPUs limitada a quatro threads, 720p/15 FPS e diretório original. A configuração de threads aceita 1–16, sempre limitada pelas CPUs disponíveis. A seleção de modelos reconhece Tiny/Base/Small multilíngues Q5_1 já instalados; não faz download.

Cada novo processamento recebe um snapshot das preferências. Isso inclui as inferências por fonte da diarização; na execução explícita, o idioma atual não substitui a metadata da transcrição tradicional anterior. A regra de não executar IA durante captura permanece. Dispositivo configurado ausente usa o padrão Windows ou outro ativo e gera aviso visível na captura, sem criar falha parcial se a captura foi bem-sucedida.

O StorageManager continua centralizando os caminhos. Um diretório local configurável afeta apenas novas reuniões; `recording_locations` registra sua pasta e permite abrir, transcrever, exportar e excluir reuniões anteriores após trocar o diretório. Reuniões sem localização registrada usam a pasta original. Database/models/logs não mudam. Destino indisponível na inicialização gera fallback para novas reuniões, mantendo os caminhos antigos; recuperação de transcrição inacessível é tratada por reunião. Preferências corrompidas geram aviso e defaults, sem sobrescrita automática. Detalhes de limites e validação: [SETTINGS_TESTING.md](SETTINGS_TESTING.md).
