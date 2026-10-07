# Fase 11 — testes de vídeo

## Estado em 2026-10-02

Implementação e validações automáticas concluídas. **A fase ainda tem pendências de validação manual:** janela do Brave (substituição do Chrome autorizada pelo usuário), aplicativo minimizado/restaurado, reprodução visual e desempenho do aplicativo completo com WebView. Não avançamos para a Fase 12.

O Computer Use encerrou a automação porque não conseguiu verificar a URL do Brave no Windows. Nenhuma ação de interface foi executada depois desse bloqueio. Os testes de captura abaixo usam diretamente o backend nativo, sem automatizar a interface. A revisão independente também foi interrompida pelo limite de uso; os três problemas que ela havia apontado foram corrigidos.

## Implementação

- `video/targets.rs`: EnumWindows/EnumDisplayMonitors, IDs validados novamente ao iniciar, janela ou monitor completo.
- `video/capture.rs`: Windows Graphics Capture `CreateFreeThreaded`, duas superfícies, worker próprio; detectar fechamento/falha da fonte e recriar frame pool após redimensionamento.
- `video/encoder.rs`: D3D11 Video Processor converte/redimensiona BGRA para NV12 na GPU, preservando proporção. Media Foundation Sink Writer gera H.264, 1280×720, 15 FPS, 2 Mbps, MP4. Hardware transforms e DXGI Device Manager habilitados; fallback na inicialização. Backpressure padrão do Sink Writer evita fila sem limite de texturas.
- `MeetingManager`: cria pasta/registro, inicia fontes opcionais, sinaliza stop de todas antes dos joins, finaliza/sincroniza arquivos, persiste `video_path`, duração e falhas parciais. Sem vídeo habilitado, não inicia runtime MF, GPU, frame pool ou thread de vídeo.
- Relógio monotônico comum para áudio/vídeo quando vídeo está habilitado. Os WAVs recebem silêncio inicial correspondente à inicialização de cada dispositivo; não descartamos áudio válido. Capturas antigas sem vídeo mantêm a API e o comportamento anteriores.
- React utiliza `list_video_sources`, `start_meeting`, `stop_meeting`, `get_recording_state` e evento `video-recording-status`; não acessa handles/arquivos diretamente.
- A saída normal verifica headers/flush de WAV e finalização do MP4 antes de encerrar recursos.

**Dependências:** nenhum crate/npm novo, FFmpeg, Python, modelo ou serviço externo. Somente features adicionais do crate `windows` já existente para WGC, D3D11/DXGI, Media Foundation e medição de memória nos testes. `build.rs` incorpora Common Controls v6 também nos executáveis de testes, corrigindo `STATUS_ENTRYPOINT_NOT_FOUND` para `TaskDialogIndirect`; mantém os recursos Tauri sem manifesto duplicado.

## Validações realizadas

### Arquivos desta fase

- Backend: `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/commands/{mod.rs,video.rs}`, `src-tauri/src/video/{mod.rs,targets.rs,capture.rs,encoder.rs,native_tests.rs}`, `src-tauri/src/meeting/manager.rs` e `src-tauri/src/audio/{microphone.rs,loopback.rs}`.
- Frontend: `src/services/meeting.ts` e `src/pages/NewRecordingPage.tsx`.
- Documentação/fixture: `README.md`, `docs/VIDEO_PLAN.md`, `docs/VIDEO_TESTING.md` e `docs/fixtures/video-test.html`.

O checkout já estava inteiramente sem arquivos rastreados; não houve commit do baseline nem publicação.

### Comandos

| Validação | Resultado |
| --- | --- |
| `cargo fmt` | Executado |
| `cargo clippy --all-targets -- -D warnings` | Passou |
| `cargo test` | 90 passaram, 0 falharam, 5 testes manuais ignorados por padrão |
| `cargo build` | Passou, debug, sem gerar instalador |
| `npm run lint` | Passou |
| `npm test` | 4 passaram |
| `npm run build` | Passou |
| Captura real de monitor + microfone + loopback | Passou, teste opt-in executado separadamente |

Há mensagens da ferramenta sobre canonicalização do diretório de usuário e uma mensagem informativa do linker MSVC ao gerar a import library, apresentada pelo Rust como warning no build. Não houve falha de build; Clippy passou com warnings negados.

Testes de regressão tiveram falha observada antes da correção e passaram depois: erro real do WGC versus pool vazio; reinício do timeout após resize; rejeição de WAV com header não finalizado. Outros testes cobrem proporção/letterbox, timestamps com frames perdidos, container incompleto, vídeo opcional, reunião só com vídeo, falha de vídeo preservando áudio e persistência após stop. O teste de orquestração usa container simulado; o teste nativo valida o codec real.

### Medição real de monitor

20 segundos de medição, monitor completo, microfone padrão e loopback padrão simultâneos. Processo de teste do core Rust, **sem React/WebView**; estes números não são o consumo total do aplicativo.

| Medida | Resultado |
| --- | --- |
| Encoder | Media Foundation H.264, atributo de hardware presente |
| Frames decodificados | 300 |
| Frames pulados durante captura | 4; timestamps preservam o tempo decorrido |
| Duração MP4 | 22.399 ms, incluindo inicialização das fontes |
| Duração microfone | 22.159 ms |
| Duração sistema | 22.298 ms |
| Diferença vídeo/microfone | 240 ms |
| Diferença vídeo/sistema | 101 ms |
| CPU média, normalizada pelos 8 processadores disponíveis | 0,89% |
| CPU equivalente a um núcleo | 7,10% |
| Pico de working set do processo | 146,08 MiB |
| Tamanho MP4 | 4.749.552 bytes |

Media Foundation Source Reader confirmou H.264, 1280×720, decodificação até EOF e timestamps monotônicos. Finalize e sync_all terminaram sem erro. A tentativa de iniciar outra gravação no mesmo caminho foi rejeitada e o tamanho do arquivo permaneceu inalterado.

Artefatos locais: `.tooling/video/monitor-1790964845111/{video.mp4,microphone.wav,system.wav,test.log}`. Logs das validações em `.tooling/video/final-{clippy,test,build}.txt` e `monitor-native-final.txt`; essa pasta é ignorada pelo Git.

A primeira captura já decodificava MP4 válido, mas falhou por diferença de 1.709 ms com o microfone. O alinhamento ao relógio da reunião corrigiu esse caso. Os 240/101 ms atuais comprovam diferença de duração, não sincronização labial para todos os dispositivos; o teste de pulsos abaixo ainda precisa de conferência manual. O teste de 20 segundos também não substitui um teste prolongado.

## Reproduzir na interface

1. No diretório do projeto, configure Rust conforme o README e execute `npm run tauri dev`.
2. Abra `docs/fixtures/video-test.html` no Brave/Chrome. É uma página local com animação e tom; clique **Iniciar som de teste** e mantenha a janela fonte visível.
3. Na tela de nova gravação, habilite microfone, áudio do sistema e **Gravar tela**. Clique em atualizar fontes e escolha a janela `MeetingRecorder video test`.
4. Inicie, fale uma frase, deixe gravar 30–60 segundos. Minimize **Meeting Recorder**, continue a animação/som e restaure. Verifique que o estado permanece gravando.
5. Finalize pelo aplicativo ou pelo tray. Abra a pasta no detalhe da reunião. Reproduza `video.mp4` em um player local e os WAVs separadamente; confirme movimento, proporção, cores, continuidade e duração. Compare os pulsos visuais/sonoros no mesmo instante dos arquivos.
6. Repita selecionando monitor completo. Feche a janela fonte durante outra gravação e confirme que o erro é exibido, o MP4 anterior continua válido e os WAVs continuam até finalizar.
7. Repita com vídeo desligado e confira que não aparece worker/estado de vídeo nem `video.mp4` na nova reunião. Anote CPU/RAM do aplicativo e subprocessos WebView pelo Gerenciador de Tarefas durante pelo menos um minuto.

O MP4 desta fase contém **somente vídeo**. Microfone e sistema permanecem WAV separados; não há multiplexação de áudio ou reprodução sincronizada embutida do MP4 nesta fase.

## Testes nativos opt-in

Execute numa sessão Windows interativa com acesso aos dispositivos (o sandbox do executor negou acesso ao áudio; o teste autorizado fora dele passou). Use os comandos a partir da raiz e a configuração Rust do README:

```powershell
$env:MEETING_RECORDER_VIDEO_TEST_SOURCE = 'monitor'
$env:MEETING_RECORDER_VIDEO_TEST_SECONDS = '20'
cargo test --manifest-path src-tauri/Cargo.toml --lib video::native_tests::native_capture_and_decode_with_simultaneous_audio -- --ignored --nocapture
```

Para janela, abra antes a fixture no Brave/Chrome:

```powershell
$env:MEETING_RECORDER_VIDEO_TEST_SOURCE = 'brave'
$env:MEETING_RECORDER_VIDEO_TEST_SECONDS = '60'
cargo test --manifest-path src-tauri/Cargo.toml --lib video::native_tests::native_capture_and_decode_with_simultaneous_audio -- --ignored --nocapture
```

Para validar um MP4 produzido pela interface:

```powershell
$env:MEETING_RECORDER_VIDEO_DECODE_PATH = Read-Host 'Caminho do video.mp4 da reunião'
cargo test --manifest-path src-tauri/Cargo.toml --lib video::native_tests::decode_existing_app_mp4 -- --ignored --nocapture
```

Os testes criam diretórios únicos e nunca sobrescrevem gravações existentes. Nenhum teste comum captura a tela automaticamente.

## Critérios de aceite

| Critério | Evidência / pendência |
| --- | --- |
| Janela do Chrome | Seleção/captura implementadas. Chrome ausente; usuário autorizou Brave. Validação da janela ainda pendente. |
| Monitor completo | Comprovado com WGC e MP4 decodificado. |
| Continuar com aplicativo minimizado | Worker independente da janela implementado; teste manual ainda pendente. |
| Baixo consumo | Core com vídeo+áudio medido; consumo do aplicativo completo e teste prolongado pendentes. |
| Encoder hardware quando disponível | Habilitado e confirmado no teste de monitor; fallback implementado, não testado em máquina sem encoder hardware. |
| Sincronização aceitável | Diferenças de duração de 240/101 ms no monitor; conferência de pulsos/Brave pendente. |
| Vídeo opcional | Teste de orquestração confirma que o starter não é chamado quando desativado. |
| Preservar gravações válidas em falha | Testes de falha parcial/flush e arquivos separados; encerramento real da janela fonte ainda pendente. |

## Limitações

WGC Win32 requer Windows 10 1903+ e os componentes gráficos/Media Foundation. Não há captura de aba específica. Conteúdo protegido/desktop seguro pode não ser capturável. Minimizar a **janela fonte** pode suspender a entrega de frames; o último frame é mantido enquanto há captura válida. Inicialização também reutiliza o primeiro frame para o intervalo anterior à sua chegada, evitando deslocar o vídeo inteiro. Fechamento da fonte/falha do dispositivo encerra o worker, finaliza o que for possível e informa erro; áudio pode continuar. Falha do disco/Finalize pode impedir um MP4 reproduzível, mas o arquivo não é apagado nem substituído.

Não há IA durante a gravação. Configurações globais, instalador e qualquer outra fase não foram implementados nesta tarefa.
