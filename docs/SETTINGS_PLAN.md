# Fase 12 — Configurações persistentes

> Execução inline com writing-plans, executing-plans, TDD e revisão final independente. Fonte: AGENTS.md, ARCHITECTURE.md, FASE 12 e solicitação atual. Não avançar para Fase 13.

## Desenho autorizado

Preferências locais no SQLite, validation no core Rust e comandos get_settings/save_settings. A tela existente de configurações deixa de ser demonstração. Valores: dispositivos padrão do Windows; Base Multilingual Q5; pt; metade dos processadores até 4 threads por padrão; 720p/15 FPS; recordings sob LOCALAPPDATA. Limites explícitos: threads 1–16 (execução também limitada pelos processadores disponíveis), resolução 480p/720p/1080p e FPS 5/10/15/30. Idiomas curados pt/en/es/fr/de/it/ja/zh/auto. Modelos multilíngues Q5 tiny/base/small presentes em models ou recursos, sem downloads.

Persistir somente os oito campos solicitados. Seleção explícita de dispositivo em start_meeting preserva comportamento anterior; preferência ausente usa padrão Windows e preferência indisponível faz fallback. A UI informa o fallback. Configuração salva afeta futuras operações; gravação/processamento ativo bloqueia alterações.

Storage continua dono de todos os caminhos. Diretório configurável é local, absoluto, regular, gravável, sem links/reparse points, fora dos diretórios internos database/models/logs. Não mover arquivos antigos. Migration 4 acrescenta app_settings e recording_locations; a pasta original de cada reunião fica registrada e é restaurada antes de recuperar transcrição. Reuniões antigas sem localização usam a pasta recordings original. Database/models/logs permanecem no local anterior. Em destino temporariamente indisponível, iniciar o app, informar fallback para recordings padrão e preservar as localizações das reuniões antigas.

Whisper recebe snapshot de modelo/idioma/limite no início do processamento. APIs Tauri atuais preservadas. Modelo/idioma utilizados ficam no registro da reunião. Diarização pós-transcrição usa o mesmo runner e limite, sem adicionar opções de diarização. Encoder recebe resolução/FPS imutáveis no início da captura, com clock e proporção ajustados.

## Tarefas

- [x] Settings core/repository e migration: RED de validação e persistência; defaults e upgrade preservando meetings/segments; corrupção recuperável sem sobrescrever preferências automaticamente.
- [x] Storage: pastas novas configuráveis, localização fixada por reunião, reload/delete/playback/exclusão em diretórios antigos e novos; testes de caminhos inválidos, internos, UNC, namespaces e arquivo onde deveria haver diretório. Proteção de links validada por leitura de código; teste físico de junction/ACL pendente.
- [x] MeetingManager: getter/saver com lifecycle guard; dispositivos com fallback; settings aplicadas a novos jobs. Testes de gravação/processamento bloqueando alterações e explícito versus configurado.
- [x] Whisper/video: testes de idioma/modelo/threads efetivos e metadata, opções disponíveis localmente, clamping e configurações de resolução/FPS/proporção; captura nativa 480p/10 FPS aprovada.
- [x] React/services/settings commands: carga paralela e tratamento de erros parciais de enumeração; feedback persistência/fallback; somente campos do escopo. Build/lint/testes aprovados; interação visual pendente.
- [x] fmt, clippy all-targets, cargo test/build, npm lint/test/build; persistência em processos distintos; documentação e revisão independente final.

## Review focus

Revisar troca de root sem repontar reuniões antigas; segurança de exclusão/export com root externo; falhas de disco/DB e ordering commit/runtime; carregar preferências inválidas sem crash; snapshotted model/language/threads incluindo diarização; configurações de vídeo realmente propagadas; compatibilidade de comandos antigos e gates de lifecycle. Não concluir a Fase 11 nem adicionar retenção/exportações da Fase 13.

## Ledger

Ruling: manter checkout atual porque todo o baseline ainda está sem commit; worktree omitiria o produto e commit de baseline não está solicitado. Custo: sem diff Git contra baseline; revisão usa arquivos e testes desta fase.

Final review: phase12_review, contexto independente, gpt-6.1-sol (Astra indisponível por limite no trabalho anterior). Três achados Important, todos corrigidos em uma passagem com regressões RED→GREEN: pasta externa indisponível durante recuperação; aviso de fallback de dispositivo; idioma da diarização explícita. Suíte final: 110 aprovados/5 ignorados; frontend 4 aprovados. Nenhum minor adiado pelo revisor.

Ruling: a verificação visual ficou pendente após timeout do navegador interno e falta do Playwright CLI no cache; não instalar outra ferramenta só para essa checagem. Custo: interação da tela requer reprodução manual documentada. Testes do core, build/lint e persistência entre processos aprovados.

Ruling: não simular remoção física de disco nem alterar ACLs do usuário; exercitar indisponibilidade/commit rejeitado com fixtures isoladas e conferir proteção de links por código. Custo: comportamento de permissões e junctions reais ainda precisa de teste manual nessa máquina.

Validações: cargo fmt; clippy --all-targets -D warnings sem avisos; cargo test 110/110, 5 opt-in ignorados; cargo build aprovado (mensagem informativa MSVC .lib/.exp); npm lint/test/build e Prettier aprovados. Captura opt-in adicional aprovada: 854×480, limite 10 FPS, H.264 por hardware, delta microfone 52 ms/sistema 47 ms, CPU máquina 0,65%, RAM pico 137,74 MiB. Evidências locais ignoradas pelo Git em .tooling/settings-*.txt; resultado/reprodução em SETTINGS_TESTING.md. Fase 13 não iniciada.
