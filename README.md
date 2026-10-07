# PitScript

PitScript é o repositório do **Meeting Recorder**, aplicativo Windows para gravar reuniões e transcrever localmente com whisper.cpp.

**Status:** versão 0.1.0 candidata a testes. Instalação/desinstalação em Windows limpo, fluxo completo offline e gravação estável de uma hora ainda aguardam validação manual. Consulte o [relatório de estabilização](docs/STABILIZATION_REPORT.md) e o [checklist da release](docs/RELEASE_CHECKLIST.md).

Aplicativo desktop Windows 10/11 para gravação local de reuniões e transcrição offline. A implementação atual inclui SQLite, storage local, captura WASAPI de microfone e áudio do sistema, MeetingManager, preparação de áudio, whisper.cpp após a reunião, histórico e detalhe com player, pesquisa, cópia, exportação TXT, abertura da pasta local e identificação local de locutores com sherpa-onnx.

A implementação atual também inclui tray e gravação opcional de janela/monitor com Windows Graphics Capture, Media Foundation e H.264 (720p/15 FPS). A Fase 11 foi validada no core com monitor e áudio simultâneo; testes manuais da janela do Brave e do aplicativo minimizado permanecem pendentes. Resultados, consumo medido e reprodução: [docs/VIDEO_TESTING.md](docs/VIDEO_TESTING.md).

A Fase 12 adiciona configurações persistentes no SQLite para dispositivos, modelo/idioma/threads, resolução/FPS e diretório de novas gravações. O padrão continua 720p/15 FPS; reuniões existentes conservam suas pastas. Campos, validação, testes e reprodução: [docs/SETTINGS_TESTING.md](docs/SETTINGS_TESTING.md).

A Fase 13 consolida a exportação TXT em UTF-8, com destino escolhido no diálogo do Windows, nome sugerido a partir do título e validação de nomes de arquivo. O texto é preparado antes do diálogo; cancelamento e falhas de gravação preservam os dados existentes. Comportamento, testes e reprodução: [docs/EXPORT_TESTING.md](docs/EXPORT_TESTING.md).

A Fase 14 adiciona diagnóstico local em `logs/meeting-recorder.log`, com rotação de 1 MiB e dois backups. Registra o ciclo de gravação/transcrição e falhas técnicas, sem gravar texto da reunião, títulos ou caminhos escolhidos. Eventos, testes e limitações: [docs/LOGGING_TESTING.md](docs/LOGGING_TESTING.md).

## Desenvolvimento

Pré-requisitos: Node.js com npm, Rust com Cargo e Clippy, Microsoft C++ Build Tools **2022, toolset v143 x64** (incluindo redistribuíveis CRT/OpenMP) e Microsoft Edge WebView2. Consulte a [documentação de pré-requisitos do Tauri](https://v2.tauri.app/start/prerequisites/).

```powershell
git clone https://github.com/matheuscunhatoodo/PitScript.git
cd PitScript
npm ci
npm run dev
```

`npm run dev` inicia a interface no navegador. Para abrir a janela desktop:

```powershell
npm run tauri dev
```

Nesta máquina, a validação usou uma instalação Rust isolada em `.tooling/` (ignorada pelo Git). Se `cargo` não estiver no PATH e essa pasta existir, configure o terminal antes de executar Tauri ou Cargo:

```powershell
$projectRoot = (Get-Location).Path
$env:CARGO_HOME = Join-Path $projectRoot '.tooling\cargo'
$env:RUSTUP_HOME = Join-Path $projectRoot '.tooling\rustup'
$env:PATH = "$(Join-Path $env:CARGO_HOME 'bin');$env:PATH"
```

## Validação

```powershell
npm run lint
npm test
npm run format:check
npm run build
cd src-tauri
cargo fmt --check
cargo check
cargo clippy
cargo test
```

## Instalador 0.1.0

Os artefatos de teste ficam em `releases/0.1.0/`: `MeetingRecorder-Setup.exe` (NSIS), MSI adicional, hashes SHA-256 e inventário dos recursos. **A versão é candidata; ainda depende dos critérios manuais da Definition of Done.** Consulte o [relatório da release](docs/RELEASE_REPORT.md) e o [checklist para Windows limpo](docs/RELEASE_CHECKLIST.md).

O usuário final executa o instalador; não precisa de Node, npm, Rust, Cargo, Python ou Whisper separado. O pacote inclui WebView2 offline, whisper.cpp, Base Multilingual Q5 e os recursos locais de diarização já existentes. A instalação NSIS é por usuário; reuniões ficam em `%LOCALAPPDATA%\MeetingRecorder`, separadas dos arquivos instalados, e são preservadas na desinstalação padrão.

Para gerar o pacote na máquina de desenvolvimento:

```powershell
npm run tauri -- build --ci
```

O build prepara os redistribuíveis originais do Visual Studio junto aos processadores, compila a interface e gera NSIS/MSI em `src-tauri/target/release/bundle/`. Se usar `VCTOOLS_REDIST_DIR` no build Tauri, aponte para a raiz `VC/Redist/MSVC`, contendo as pastas numéricas de versão. A execução isolada de `scripts/prepare-release.ps1 -VCRedistDirectory` também aceita uma pasta de versão com `x64/Microsoft.VC143.CRT` e `x64/Microsoft.VC143.OpenMP`. A primeira geração pode baixar ferramentas de empacotamento e o instalador oficial do WebView2; não há download de modelos nem API externa durante o uso do Meeting Recorder.

## Organização

- `src/app`, `components`, `pages`, `types`, `styles`: interface. `src/services` centraliza as chamadas Tauri; histórico e detalhe usam reuniões persistidas no SQLite.
- `src-tauri/src/database`: inicialização local do SQLite, migration e repository de reuniões.
- `src-tauri/src/storage`: criação dos diretórios locais, caminhos dos arquivos de reunião e exclusão segura da pasta de uma reunião.
- `src-tauri/src/audio`: captura WASAPI do microfone e escrita de WAV PCM.
- `src-tauri/src/meeting/manager.rs`: ciclo da captura de microfone, estados e persistência de resultado.
- `src-tauri/src/commands/meeting.rs`: comandos Tauri `create_meeting`, `list_meetings`, `get_meeting`, `update_meeting` e `delete_meeting`; criação e exclusão coordenam SQLite e storage no backend.
- `src-tauri/src/video`: seleção de fontes, captura WGC, conversão D3D11 e encoder Media Foundation H.264; iniciados exclusivamente pelo MeetingManager quando habilitados.
- `src-tauri/src/settings` e `database/settings.rs`: preferências validadas e persistentes; comandos `get_settings`/`save_settings` coordenados pelo MeetingManager. `database/locations.rs` preserva a pasta original das reuniões.
- [docs/IMPLEMENTATION_PLAN.md](docs/IMPLEMENTATION_PLAN.md): ordem das fases.

O funcionamento principal planejado é local e offline. Nenhum dado de reunião é enviado para serviços externos.

## Recursos e licenças

Os recursos necessários ao build Windows estão em `src-tauri/resources/`, incluindo o modelo Whisper Base Multilingual Q5, os processadores locais e suas dependências. Eles são recursos redistribuídos de terceiros, com licenças e atribuições próprias: [avisos dos runtimes](src-tauri/resources/RUNTIME_NOTICES.md) e [atribuição da diarização](src-tauri/resources/diarization/ATTRIBUTION.md). A licença do código original deste projeto ainda não foi definida.

Instaladores gerados, toolchains, bancos, logs, perfis de teste e gravações são excluídos pelo `.gitignore`. Os WAVs em `src-tauri/tests/fixtures/` são amostras públicas documentadas para testes; não são gravações de usuários.

A identificação é executada depois da transcrição, com progresso/cancelamento e rótulos `Você`, `Participante N` ou `Participantes` para trechos incertos. Reuniões antigas continuam exibindo o texto tradicional. Modelos, testes e reprodução manual: [docs/DIARIZATION_TESTING.md](docs/DIARIZATION_TESTING.md). As DLLs Visual C++/OpenMP x64 acompanham os processadores no instalador; a verificação em Windows limpo permanece no checklist de release.

O banco é criado automaticamente em `%LOCALAPPDATA%\MeetingRecorder\database\meeting-recorder.db`. Ele armazena metadados, estados, caminhos de arquivos e texto da transcrição; arquivos de áudio e vídeo não são gravados como BLOB.

Na inicialização, o Storage Manager também cria `recordings/`, `models/` e `logs/` sob `%LOCALAPPDATA%\MeetingRecorder`. As pastas individuais ficam em `recordings/{meeting_id}/`. Os arquivos previstos são `microphone.wav`, `system.wav`, `merged.wav`, `video.mp4` e `transcript.txt`. Os comandos Tauri criam a pasta junto com o registro e removem os arquivos da reunião ao excluir seu registro.

O `MeetingManager` expõe os comandos Tauri `start_meeting`, `stop_meeting` e `get_recording_state`. Ele cria registro e pasta, inicia as fontes habilitadas, interrompe ambas antes de aguardar o flush dos WAVs e persiste duração, caminhos e estado final. O frontend envia somente a configuração da reunião. `list_input_devices` e `list_output_devices` permitem consultar dispositivos; sem seleção explícita, o backend usa os padrões do Windows. A captura grava `recordings/{meeting_id}/microphone.wav` e `recordings/{meeting_id}/system.wav` separadamente e emite `recording-status` e `system-recording-status`.

Consulte os testes manuais do [microfone](docs/MICROPHONE_MANUAL_TEST.md), do [áudio do sistema](docs/SYSTEM_AUDIO_MANUAL_TEST.md), da [preparação de áudio](docs/AUDIO_PREPARATION_MANUAL_TEST.md), da [transcrição](docs/WHISPER_TESTING.md) e do [player e transcrição](docs/PLAYER_TRANSCRIPT_TESTING.md). Ao terminar a reunião, o backend gera `merged.wav` em PCM mono 16-bit/16 kHz a partir das fontes válidas e inicia a transcrição local. O texto fica no SQLite e em `transcript.txt`. O player autoriza somente o WAV validado da reunião; exportação TXT e abertura da pasta são executadas no backend.
