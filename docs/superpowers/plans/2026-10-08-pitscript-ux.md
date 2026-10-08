# PitScript UX Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Aplicar os três mockups aprovados e publicar uma pré-release instalável 0.1.1.
**Architecture:** Estado da gravação compartilhado no App; componentes de player/transcrição e helpers de apresentação. Serviços Tauri existentes; uma extensão de elapsedSeconds no MeetingManager.
**Tech Stack:** React 19, TypeScript, CSS, Tauri 2, Rust; sem dependências de produto novas.
**Spec:** docs/superpowers/specs/2026-10-08-pitscript-ux-design.md

## Global Constraints

Local-first; nenhuma IA durante gravação; nenhum acesso a arquivos/banco/dispositivos em componentes. Não alterar schema, identificador, diretório de dados nem código de captura. Versão 0.1.1; tag v0.1.1-rc.1. Paleta #252722/#FAF8F3/#D5A34D; usar os três mockups aprovados. Autorização explícita do usuário inclui implementar, enviar e publicar; execução inline até concluir.

## Review Focus

1. Estado global na navegação, stop repetido, falhas de refresh: não permitir gravação duplicada/ocultar finalização.
2. Fonte desconectada com outra ativa: preservar captura e avisos; não afirmar que fonte falha captura.
3. Segmentos sobrepostos, pesquisa por rótulo/timestamp e gravações antigas: pesquisa/cópia completas e fallback estável.
4. Seek antes de metadata, final de áudio, URL substituída, play rejeitado: não continuar áudio errado nem posição inválida.
5. Configuração/dispositivo ausente e pós-processamento após navegação: defaults/fallback e bloqueio de nova IA durante captura preservados.

### Task 1: Apresentação e estado global

Files: src/utils/meeting.ts, tests/meeting-presentation.test.mjs, src/hooks/useRecording.ts, src/components/{AppShell,Icon,RecordingBar}.tsx, src/pages/{HomePage,NewRecordingPage,SettingsPage}.tsx, src/app/App.tsx, src/styles/app.css.
Consumes: getRecordingState/startMeeting/stopMeeting/getSettings/listInputDevices/listOutputDevices.
Produces: useRecording(): RecordingController (capture,busy,error,refresh,start,stop), helpers groupMeetingsByDay/meetingProgressLabel/sourceStatusLabel/clampSeek.
- [x] Escrever testes de agrupamento/calendário/falhas e seek; executar npm test (RED).
- [x] Implementar helpers e hook global com refresh serial e limpeza de listeners/timers; estado/conflitos visíveis.
- [x] Aplicar shell/marca/paleta/lista por dia, formulário sem threads com seletores existentes e console/barra da gravação.
- [x] Executar npm test, lint, build; commit da tarefa.

### Task 2: Player e transcrição

Files: src/components/{AudioPlayer,TranscriptView}.tsx, src/pages/MeetingPage.tsx, src/utils/meeting.ts, tests/meeting-presentation.test.mjs, src/styles/app.css.
Consumes: getMeetingAudio/getMeeting/getMeetingDiarization/copyTranscript/exportTranscript, helpers Task 1.
Produces: AudioPlayer({url,onTimeChange,seekRequest}), TranscriptView({segments,text,matches,selectedMatch,onSeek,currentTime}).
- [x] Testar agrupamento de blocos/offsets da busca e escolha de trecho temporal, com sobreposição/fallback (RED).
- [x] Player controlado por HTMLAudioElement; eventos reais e seek clamp, limpeza de mídia ao desmontar.
- [x] Renderizar blocos sem perder pesquisa por timestamp/locutor; copiar/exportar e cancelamento/retry existentes preservados.
- [x] Executar frontend checks e QA de navegação/busca/play/seek/velocidade/fallback; commit.

### Task 3: Contrato do contador, versão e validação

Files: src-tauri/src/meeting/manager.rs, src/services/meeting.ts, src-tauri/{Cargo.toml,Cargo.lock,tauri.conf.json}, package{,-lock}.json, index.html, docs/UX_RELEASE_REPORT.md, README.md.
Consumes: MeetingSession.started: Instant, versão 0.1.0 e pipelines existentes.
Produces: elapsedSeconds: u64 aditivo no get_recording_state; versão 0.1.1 e janela PitScript 1280x800.
- [x] Teste Rust de contador monotônico, idle e duração final (RED); implementar extensão sem mudar persistência.
- [x] Bump coerente da versão/título/tamanho e documentar implementação/limites/QA visual.
- [x] fmt, Clippy, cargo test, frontend lint/test/build/format; production build e Tauri bundle.
- [x] Rever ramo completo com agente independente, corrigir achados importantes com testes; commit.

### Task 4: GitHub e release

Files: docs/UX_RELEASE_REPORT.md e notas da release; artifacts ignorados releases/0.1.1/.
Consumes: commits validados, instaladores novos e modelos/runtimes existentes.
Produces: main atualizado e release pública v0.1.1-rc.1 com assets e hashes.
- [ ] Conferir arquivos staged/ausência de dados privados; merge fast-forward/push autorizados.
- [ ] Extrair MSI e conferir recursos, iniciar binário com perfil sintético; atualizar relatório com evidências.
- [ ] Criar draft, anexar EXE/MSI/manifests/SHA256SUMS, validar hashes antes de publicar prerelease.
- [ ] Verificar release/tag, download anônimo e integridade; relatar link e pendências.
