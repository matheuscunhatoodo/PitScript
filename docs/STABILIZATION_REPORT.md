# Fase 15 — testes e estabilização

## 1. Testes realizados

Documentação de arquitetura/roadmap e relatórios das fases 0–14 revisados. Nenhuma funcionalidade nova. Backend, armazenamento/SQLite reais e processadores locais existentes foram exercitados pelos testes. O usuário escolheu executar pessoalmente os testes manuais da interface; a automação da janela foi interrompida e nenhum resultado manual foi recebido até este registro.

- Baseline: 124 testes Rust aprovados, cinco testes de captura opt-in ignorados; quatro testes frontend aprovados; fmt/check, Clippy, lint e build frontend aprovados.
- Regressão de panic do runner: falhou primeiro com estado `processing` após o worker terminar; passou após a correção, com áudio idêntico, estado `failed`, retorno ao estado final da reunião e retry concluído.
- Probe de configuração release em `.tooling/phase15-panic-probe.rs`: compilado com o perfil anterior `panic=abort`, encerrou o processo com código -1073740791; com `panic=unwind`, retornou zero e `worker_failure_contained=true`.
- Novo teste integrado `complete_lifecycle_transcribes_locally_and_reopens_persisted_meeting_after_partial_failure`: start → WAV de fonte → stop/flush → preparação PCM 16 kHz mono 16-bit → Whisper real → atribuição local → SQLite/TXT → reabrir SQLite/storage → exportar TXT UTF-8. Executado tanto normalmente quanto com saída falhando ao iniciar. A captura de dispositivo é substituída por amostra pública de fala; os demais módulos são reais. Não equivale a reunião ao vivo nem desconexão física durante captura.
- Regressão de retry após falha de arquivo na preparação: falhou primeiro com `Prepared meeting audio is unavailable`; depois passou ao remover o impedimento de teste e preparar novamente a mesma reunião. Fonte original permaneceu idêntica, destino inválido não foi substituído automaticamente e `failed_partial` foi conservado.
- Suite completa após as correções: 127 testes Rust aprovados, zero falhas e cinco opt-in ignorados. Evidência: `.tooling/release-tests.txt` (validação retomada na etapa adicional de release).

## 2. Testes aprovados

Repository/migrations/reabertura, caminhos/diretórios/exclusão, duração/estados, fontes parciais, panic de captura, stop/flush de ambas antes de join, exclusão entre captura/IA, conversões/clipping/cancelamento de preparação, Whisper real curto, sherpa real com uma/duas/três amostras públicas, progresso/cancelamento/retry, preservação de áudio/texto, reuniões antigas, exportação com português, configurações/fallback, logging concorrente/rotação/erros e lógica de tray/vídeo coberta na suite. Frontend: quatro testes existentes de busca literal, duração, datas e segmentos para cópia/exportação.

## 3. Falhas encontradas

1. Panic no runner terminava o worker, deixando memória e SQLite com transcrição `processing`, bloqueando nova tentativa.
2. Perfil release `panic=abort` fazia o processo inteiro abortar antes dos tratamentos existentes de panic, inclusive captura e diarização.
3. Falha transitória ao criar `merged.wav` conservava fontes, mas retry de transcrição nunca executava novamente a preparação.

O novo teste integrado teve inicialmente um erro de compilação do próprio teste (rótulo é `String`, não `Option<String>`); foi ajustado e executado novamente. Não era falha do produto.

## 4. Correções realizadas

- `src-tauri/src/transcription/engine.rs`: conter panic do runner e encaminhar pelo tratamento existente de falha/persistência/retry; mensagem útil sem payload privado.
- `src-tauri/Cargo.toml`: release usa `panic=unwind`, compatível com preservação e finalização das fontes saudáveis.
- `src-tauri/src/meeting/manager.rs`: retry refaz preparação ausente no orquestrador, exclusivamente após finalizar e sem outro processamento ativo. Preserva arquivos existentes e APIs.
- Três testes adicionados: panic/retry; fluxo completo real com fonte de fixture; preparação/retry após erro de arquivo.
- Revisão independente somente leitura confirmou os dois últimos achados; correções verificadas pelo autor com regressão e suite. Sem dependência nova, migration ou alteração React.

## 5. Métricas observadas

- Teste integrado real, dois cenários curtos: 20,67 s no teste isolado. Amostra de fala de aproximadamente 11 s; não é benchmark em português ou de reunião longa.
- Suite Rust de 127 testes, incluindo processadores reais: 34,70 s na execução registrada.
- Build frontend de produção: 29 módulos, JS 254,75 kB/78,35 kB gzip, CSS 14,55 kB/4,05 kB gzip. Tempo 794 ms em uma execução; varia com o ambiente.
- Houve uma leitura pontual do processo desktop isolado ocioso: working set 4.419.584 bytes, private bytes 6.258.688, 449 handles/19 threads. Não inclui WebView/subprocessos e não mede CPU média, gravação, crescimento ou leaks. Não usar para afirmar meta de consumo ou ausência de vazamentos.
- Não há benchmark atual representativo de CPU/RAM do aplicativo completo durante gravação. Medições antigas de vídeo continuam identificadas como históricas em `VIDEO_TESTING.md`/`SETTINGS_TESTING.md`.

## 6. Riscos ainda conhecidos

- Recuperação de transcrição interrompida existe; recuperação automática de captura após crash não está implementada (roadmap a prevê posteriormente). Crash/energia pode deixar status e temporários; checkpoints WAV limitam perda, sem garantia contra falha física do disco.
- Erro ao persistir em SQLite indisponível pode deixar estado em disco pendente até recuperação no próximo início; logs locais e áudio preservado ajudam o diagnóstico.
- Não foi comprovada ausência de leaks em execução prolongada; fixtures curtas não substituem uma hora de gravação/IA.
- Qualidade acústica em português, ruído/eco, sobreposição natural e drift prolongado ainda não foi quantificada.
- Destination inválido/corrompido é conservado. O usuário precisa resolver o impedimento (por exemplo mover um arquivo inválido) antes do retry; o programa não apaga a única cópia automaticamente.

## 7. Critérios de aceite ainda não atendidos

| Cenário obrigatório | Estado e procedimento |
| --- | --- |
| Reunião 5 min | Pendente manual: mic+sistema, falar/reproduzir áudio, parar, conferir WAV/merged/TXT/SQLite/player. Teste integrado curto aprovado não substitui este cenário. |
| Reunião 60 min | **Pendente crítico do MVP**: registrar CPU/RAM/handles ao longo da sessão, durações/perdas e tempo de transcrição. |
| Microfone desconectado | Simulação de falha/panic aprovada; remoção física durante captura pendente. Confirmar outra fonte continua e WAV válido é preservado. |
| Mudança/perda da saída/Bluetooth | Fallback e falha ao iniciar aprovados em testes; perda física/troca durante captura pendente. |
| Aplicação minimizada/restaurada | Pendente manual; confirmar contador/arquivos continuam e UI/tray restaura. |
| Fechar durante gravação/saída pelo tray | Lógica/flush/recusa sem confirmação aprovados em testes; interação nativa pendente. Fechar janela não deve encerrar captura; saída deve confirmar e finalizar. |
| Pouco disco | Erro de arquivo e retry simulados aprovados; ENOSPC real/falha parcial física não testados. Fazer em volume de teste limitado, nunca encher disco do usuário. |
| Cancelar transcrição | Tests de engine e processo sherpa aprovados; botão/UI nativa e áudio longo pendentes. WAVs/texto já finalizado devem permanecer. |
| Reabrir dados/transcrição pendente | SQLite/recuperação e reuniões antigas aprovados em testes; sequência instalada/UI pendente. |
| Janela Chrome/Brave | Pendente manual. Brave foi autorizado pelo usuário como substituto. Não declarar captura de janela aprovada pelo antigo teste de monitor. |
| Offline | Modelos/binários locais e testes sem chamadas de serviço aprovados; instalação e fluxo completo numa máquina limpa sem rede ainda pendentes. |
| Instalar/desinstalar Windows limpo | Tratado na etapa adicional de release; permanece gate de aceite até execução nessa máquina. |

O aplicativo **não é declarado pronto**. As correções e validações automatizadas desta fase foram executadas; os critérios manuais críticos continuam abertos. Sem avanço para funcionalidades novas.
