# Fase 12 — Configurações

## Implementação

Somente os oito campos desta fase foram adicionados. O frontend chama `get_settings` e `save_settings`; o MeetingManager valida, coordena o lifecycle e aplica as preferências. SQLite schema v4 acrescenta `app_settings` (JSON validado, singleton) e `recording_locations` (pasta original por reunião, FK em cascata). Não há novas dependências, downloads automáticos ou serviços externos.

| Campo | Padrão | Valores aceitos |
| --- | --- | --- |
| Microfone | Padrão do Windows | ID de dispositivo ou automático |
| Saída | Padrão do Windows | ID de dispositivo ou automático |
| Modelo | Base Multilingual Q5 | Tiny/Base/Small multilíngues Q5_1 instalados |
| Idioma | Português (`pt`) | pt/en/es/fr/de/it/ja/zh/auto |
| Máximo de threads | Metade das CPUs, entre 1 e 4 | 1–16, limitado também pelas CPUs disponíveis |
| Resolução | 720p | 480p (854×480), 720p, 1080p |
| FPS | 15 | 5/10/15/30 |
| Gravações | `%LOCALAPPDATA%\MeetingRecorder\recordings` | Pasta local absoluta e gravável |

As preferências de dispositivo são aplicadas quando `start_meeting` não recebe seleção explícita. Se o dispositivo salvo não existir, o backend usa o padrão do Windows ou o primeiro ativo, registra o fallback e envia aviso em `MeetingRecordingState.warnings`. Um fallback bem-sucedido não transforma a gravação em falha parcial. A tela de configurações e a tela de nova reunião exibem os avisos. Sem nenhum dispositivo ativo, a fonte falha como antes, preservando outras fontes.

Whisper e diarização recebem o modelo/idioma/limite no início de cada trabalho. A diarização solicitada manualmente usa o idioma atual sem alterar a metadata/texto da transcrição tradicional anterior. Vídeo recebe um perfil imutável ao iniciar. Salvar é bloqueado durante gravação ou pós-processamento. As APIs anteriores permanecem; `warnings` é uma adição à resposta de gravação.

## Diretório e compatibilidade

Banco, logs e modelos permanecem na estrutura original. Trocar o diretório afeta somente reuniões criadas depois da alteração; não move nem apaga arquivos antigos. A localização de cada reunião é restaurada antes de recuperar trabalhos interrompidos. Reuniões anteriores à migration usam o diretório original.

O StorageManager rejeita caminhos relativos, `..`, rede/UNC, namespaces de dispositivo, links/junctions/reparse points, pastas internas/ancestrais do aplicativo e pastas dentro de uma reunião existente. Faz uma prova de escrita/flush antes de salvar. O SQLite é atualizado antes de trocar o diretório efetivo em memória; falha no banco preserva a configuração anterior, embora possa deixar uma pasta candidata vazia.

Se o destino estiver indisponível na inicialização, novas reuniões usam o diretório padrão e a interface informa o fallback. Reuniões antigas mantêm suas localizações, voltando a ficar acessíveis quando o destino retornar. Preferências inválidas usam padrões seguros com aviso, sem sobrescrever automaticamente o valor salvo. Uma transcrição interrompida com pasta inacessível não impede abrir o app: preserva texto existente no SQLite, ou marca a transcrição como falha quando não há texto recuperável.

## Modelos locais

Os arquivos reconhecidos são `ggml-tiny-q5_1.bin`, `ggml-base-q5_1.bin` e `ggml-small-q5_1.bin`. Podem estar em `%LOCALAPPDATA%\MeetingRecorder\models` ou nos recursos Whisper existentes. Somente opções disponíveis localmente são habilitadas; Base continua sendo o padrão mesmo numa instalação sem o modelo. O arquivo precisa ser regular e não vazio; incompatibilidade/corrupção é tratada pelo Whisper ao carregar, mantendo as gravações. Esta fase não adiciona modelos ao bundle nem verifica a qualidade de Tiny/Small.

## Testes automatizados

Os testes desta fase cobrem defaults e entradas inválidas; persistência e rejeição sem sobrescrita; JSON inválido/parcial; upgrade v3→v4 mantendo reuniões e segmentos; diretório antigo/novo, exportação protegida e exclusão; caminhos com caixa diferente no Windows, UNC e namespaces; destino indisponível; falha simulada no commit; bloqueio durante gravação/transcrição; escolha explícita versus preferência e fallback; transporte dos avisos; modelo selecionado, idioma e threads; diarização explícita; recuperação com destino indisponível e com/sem texto; perfis, proporção e clock do vídeo.

O teste `settings_and_both_recording_roots_survive_separate_processes` cria preferências e reuniões em um processo e abre o core em outro, com `LOCALAPPDATA` isolado. Confere todos os oito campos, os dois diretórios, WAVs válidos autorizados para playback e exclusão da nova reunião preservando a antiga. Não altera dados reais do usuário. O pequeno arquivo Tiny dessa fixture verifica descoberta do modelo, sem executar inferência nele. A suíte existente continua exercitando Whisper Base e sherpa reais.

Com Rust no PATH:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo clippy --offline --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --offline --manifest-path src-tauri/Cargo.toml
cargo build --offline --manifest-path src-tauri/Cargo.toml
npm run lint
npm test
npm run build
```

Os testes de captura nativa são opt-in e ficam ignorados na suíte comum. Para conferir a propagação de um perfil de vídeo, o teste existente aceita `MEETING_RECORDER_VIDEO_TEST_RESOLUTION` e `MEETING_RECORDER_VIDEO_TEST_FPS`. A taxa média no MP4 pode ser menor que o FPS escolhido, pois inclui o frame inicial prolongado e frames omitidos; timestamps seguem o relógio para preservar a sincronização.

## Reprodução manual na aplicação

### Arquivos da fase

- Core e persistência: `src-tauri/src/settings/mod.rs`, `database/settings.rs`, `database/locations.rs`, `database/migrations.rs`, `database/mod.rs`, `storage/mod.rs`, `meeting/manager.rs`.
- Aplicação das preferências: `transcription/mod.rs`, `transcription/engine.rs`, `transcription/whisper.rs`, `video/mod.rs`, `video/capture.rs`, `video/encoder.rs`, `video/native_tests.rs` (sob `src-tauri/src`).
- Ponte Tauri: `src-tauri/src/commands/settings.rs`, `commands/mod.rs`, `lib.rs`.
- Frontend: `src/services/settings.ts`, `src/services/meeting.ts`, `src/pages/SettingsPage.tsx`, `src/pages/NewRecordingPage.tsx`, `src/styles/app.css`.
- Documentação: `README.md`, `docs/ARCHITECTURE.md`, `docs/SETTINGS_PLAN.md`, `docs/SETTINGS_TESTING.md`.

O checkout está inteiramente sem baseline de commit; a lista identifica os arquivos trabalhados nesta fase.

### Passos

1. Abra `npm run tauri dev` e entre em **Configurações**.
2. Altere idioma, máximo de threads e perfil de vídeo; salve e confirme a mensagem de persistência.
3. Saia pelo tray, abra novamente e confira os mesmos valores.
4. Escolha outra pasta local para gravações. Crie uma reunião nova e confira sua pasta; abra uma reunião anterior e confira que seu áudio/pasta continuam acessíveis.
5. Escolha um dispositivo, desconecte-o e atualize os dispositivos. Confira o aviso e inicie uma reunião com outro dispositivo ativo para validar o fallback.
6. Durante uma gravação ou transcrição, entre em Configurações: alterações devem ficar bloqueadas. Após terminar, use **Atualizar dispositivos e estado**.
7. Tente diretório relativo, pasta interna e máximo de threads fora de 1–16: salvar deve falhar, mantendo a configuração anterior.

## Validações e pendências

A revisão independente encontrou três problemas: recuperação com pasta externa inacessível, aviso de fallback ausente na captura e idioma antigo na diarização explícita. Corrigidos com testes de regressão que falharam antes da correção e passaram depois.

Resultados em 02/10/2026:

| Validação | Resultado |
| --- | --- |
| `cargo fmt` | Executado; formatação conferida |
| `cargo clippy --all-targets -- -D warnings` | Aprovado, sem avisos |
| `cargo test` | 110 aprovados, 0 falhas, 5 testes opt-in ignorados; binário e doc-tests aprovados |
| `cargo build` | Aprovado; linker MSVC emitiu mensagem informativa sobre criação de `.lib/.exp` |
| `npm run lint` | Aprovado |
| `npm test` | 4 aprovados |
| `npm run build` | Aprovado, TypeScript e Vite |
| Prettier nos cinco arquivos frontend alterados | Aprovado |
| Persistência entre processos | Aprovada, incluindo os oito campos e playback nos diretórios antigo/novo |
| Captura nativa 480p/10 FPS + áudio | Aprovada, MP4 H.264 decodificado pelo Media Foundation |

No teste nativo com 6 segundos de medição, o arquivo total incluiu 9,10 segundos de timeline (incluindo inicialização), 62 frames, 854×480, encoder por hardware e 1.300.509 bytes. Diferenças: 52 ms para microfone e 47 ms para sistema. CPU média: 5,21% de um core, ou 0,65% da máquina com oito processadores lógicos; pico de working set: 137,74 MiB. A taxa média foi 6,813 FPS porque o primeiro frame cobre o tempo de inicialização; o limitador/timestamps usam o perfil de 10 FPS. Essa amostra curta do core não garante desempenho de toda a aplicação ou de outros perfis/hardwares. O primeiro teste foi bloqueado pelo sandbox no microfone; a execução autorizada com acesso aos dispositivos passou.

### Critérios de aceite

- Áudio: preferências de microfone/saída persistentes e fallback testado; seleção explícita anterior preservada.
- Transcrição: modelo multilíngue local, idioma e limite de threads aplicados; nenhum processamento de IA durante gravação.
- Vídeo: resolução/FPS persistentes, propagados ao encoder; defaults 720p/15 FPS preservados.
- Armazenamento: diretório validado, criado e usado nas novas reuniões; reuniões antigas continuam nas pastas originais.
- Defaults, validação e persistência local: atendidos, com testes de corrupção, upgrade, commit rejeitado e reinicialização.
- Compatibilidade: migrations e dados anteriores preservados; comandos atuais mantidos e avisos adicionados sem mudar o sucesso da captura.
- Escopo: somente oito preferências, sem retenção ou trabalho da Fase 13; nenhuma dependência adicionada.

A validação visual interativa desta fase permanece pendente: a aba isolada do navegador interno ficou sem responder a comandos; o Playwright CLI também não estava disponível no cache offline. A fixture usava apenas dados sintéticos e IPC simulado, e não contou como teste aprovado. Persistência foi validada por reinicialização do core em processos separados; o fluxo visual de fechar/reabrir a janela deve ser conferido pelos passos acima. Permissões físicas de pastas e remoção de hardware variam por máquina; falhas de banco/pasta/dispositivo têm cobertura automatizada, mas não houve remoção física de disco ou teste de ACL/junction nesta fase.

Fase 13 não foi iniciada. As pendências manuais de janela Brave e aplicativo minimizado da Fase 11 continuam em `VIDEO_TESTING.md`.
