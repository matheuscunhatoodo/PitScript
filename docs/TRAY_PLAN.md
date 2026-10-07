# Fase 10 — System Tray

Escopo vinculante: Fase 10 do IMPLEMENTATION_PLAN, seção 20 da arquitetura e solicitação do usuário. Somente tray/lifecycle, sem vídeo/Fase 11. Execução inline com testes e revisão final independente.

## Desenho

Tray em Rust/Tauri 2: abrir, visualizar estado, finalizar gravação e sair. Ícone vermelho durante captura, tooltip/menu com estado/duração; polling leve em memória a cada segundo. Fechar a janela mantém WebView/MeetingManager vivos e oculta a janela; abrir restaura e dá foco. Minimizar mantém comportamento normal do Windows.

Sair durante captura pede confirmação nativa com opção padrão de cancelar. Confirmar encerra captura pelo MeetingManager, faz flush/persistência/preparação e encerra workers de transcrição antes de autorizar saída. Não inicia Whisper ao sair. Durante processamento sem captura, pede confirmação para cancelar com preservação dos resultados já salvos. Callbacks do event loop não executam trabalho pesado. Saída fica bloqueada até shutdown bem sucedido; novas capturas/jobs ficam bloqueados sob o mesmo lock de lifecycle. Erro de finalização mantém aplicativo aberto para diagnóstico/tentativa.

Não adicionar plugin de tray/dialog: habilitar feature `tray-icon` do Tauri; confirmação via MessageBox Windows já disponível na dependência `windows`. Ícones RGBA simples gerados em Rust (estado, sem novos assets/dependências).

## Tarefas

- [ ] Shutdown no core: teste recusa sem confirmação, gravação continua, confirmação salva WAV/SQLite, novos jobs recusados; cancelamento/join de transcrição e snapshot leve.
- [ ] Tray/lifecycle: ações nativas, atualização de estado, ocultar/restaurar, guard de saída e confirmação; testes de política/formatação/integração quando ambiente permitir.
- [ ] Sincronizar frontend após parada pelo tray com evento de lifecycle existente/aditivo; build e testes pertinentes.
- [ ] Fmt/clippy/test/build, teste nativo de minimizar/restaurar/fechar/parar/saída, revisão independente e relatório.

## Decisões

- Usar o checkout autorizado: baseline inteiro ainda sem commits, worktree omitiria o código existente. Sem commit de baseline ou publicação.
- Confirmar saída preserva WAV/textos concluídos e cancela inferência. Transcrição pendente pode ser retomada explicitamente após reabrir. Custo: processamento automático da reunião encerrada ao sair não é executado.
- Polling de estado deve usar memória e não consultar SQLite nem executar captura/processamento; eventos frontend limitados a mudanças de lifecycle.

## Registro

- Documentação e código inspecionados: backend não possui interceptação de CloseRequested/ExitRequested ou shutdown explícito; Drop só sinaliza/junta captura. É necessário finalizar o registro antes de permitir saída do runtime.
