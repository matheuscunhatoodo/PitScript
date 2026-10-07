# Fase 11 — Gravação de tela

> Execução inline com superpowers:executing-plans, testes e revisão independente final.

**Objetivo:** gravar opcionalmente janela ou monitor em MP4 H.264 local, mantendo áudio, MeetingManager e tray.

**Fonte de verdade:** AGENTS.md, ARCHITECTURE seções 5, 8, 12, 13, 20 e FASE 11 do IMPLEMENTATION_PLAN; solicitação atual autoriza implementação e testes. Sem Fase 12.

## Desenho

- `video/targets.rs`: enumerar janelas visíveis e monitores; IDs opacos, validação no backend.
- `video/capture.rs`: WGC CreateFreeThreaded, duas superfícies, worker sem dependência da janela React; recuperar mudança de tamanho, detectar fonte fechada.
- `video/encoder.rs`: D3D11 Video Processor redimensiona e converte BGRA para NV12 na GPU; Media Foundation Sink Writer grava H.264 1280×720, 15 FPS, 2 Mbps, MP4. Habilitar transforms de hardware e DXGI Device Manager; consultar encoder escolhido e informar fallback. Finalize e sync antes de marcar arquivo válido.
- `video/mod.rs`: configuração, estado, worker, clock monotônico, validação leve do container e testes. Quando vídeo está desligado, não criar device, frame pool, MF runtime ou worker.
- MeetingManager coordena criação, início, stop de todas as fontes antes de joins, persistência de video_path/duração/status e falhas parciais. Áudio continua separado em WAV; MP4 desta fase contém apenas vídeo. Reprodução conjunta/mux de áudio não faz parte do pedido.
- React envia seleção ao start_meeting e recebe estado; comando adicional lista fontes. Qualidade inicial fixa 720p/15 FPS. Não criar tela de configurações global (Fase 12).
- Preservar fontes válidas em erro. Revisar o guard de saída existente para não ignorar falha de finalização de arquivos. Saída normal ainda preserva capturas com falhas parciais.

## Verificação

- [x] RED/GREEN: configuração opcional, proporção/letterbox, clock/frames perdidos, validação MP4, erro WGC, timeout após resize e flush de WAV. Proteção contra sobrescrita comprovada no teste nativo.
- [x] WGC/D3D11/MF reais para monitor; MP4 decodificado até EOF com Source Reader.
- [x] Integração: vídeo desligado sem worker, falha de vídeo mantendo WAV, vídeo isolado e stop/shutdown, compatibilidade de chamadas antigas.
- [x] Frontend: toggle/seleção/estado e build/lint/testes.
- [x] Monitor e áudio simultâneo; CPU/RAM do core e diferença de duração medidos. Hardware confirmado e tamanho registrado.
- [ ] Brave com página sintética (substituição autorizada do Chrome), app minimizado, reprodução visual/pulsos, consumo do aplicativo completo e teste prolongado.
- [x] cargo fmt, clippy, test, build; npm run build, lint, test; relatório em VIDEO_TESTING.md.
- [ ] Revisão independente final completa: revisão interrompida por limite de uso; três problemas já apontados foram corrigidos e revalidados.

## Limites

WGC requer Windows 10 1903+ (interop Win32) e componentes gráficos/Media Foundation disponíveis. Minimizar o recorder não interrompe captura. Minimizar a **janela fonte**, conteúdo protegido e desktop seguro podem suspender frames; conservar último frame e informar essa limitação. Fechar a fonte finaliza o MP4 disponível e mantém áudio ativo. Encoder/dispositivo/disco podem falhar; preservar arquivos, reportar erro e nunca substituir gravação existente.

Baseline do checkout ainda sem commit: trabalhar na pasta autorizada, sem criar worktree que omitiria todo o código. Sem commit de baseline/publicação.
