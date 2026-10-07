# Release 0.1.0 — checklist em Windows limpo

Status inicial: **candidata para teste, não pronta para publicação**. O checklist deve ser executado e assinado com data, Windows/build, CPU/RAM, dispositivo e hash SHA-256 do instalador. Não marcar itens usando apenas resultados da máquina de desenvolvimento.

## Ambiente

- [ ] VM ou computador Windows 10 x64 suportado, sem Node/npm, Rust/Cargo, Python, Git, Whisper, modelos ou Visual C++ instalados manualmente; usuário padrão.
- [ ] Repetir em Windows 11 x64. Usar snapshots separados para NSIS e MSI, sem instalar um sobre o outro.
- [ ] Incluir uma máquina sem WebView2 preinstalado, quando possível; não desinstalar runtimes compartilhados na máquina de trabalho.
- [ ] Desconectar rede antes da instalação. O WebView2 offline e os recursos locais devem bastar.
- [ ] Registrar espaço livre e memória. Usar somente reuniões de teste consentidas; fones evitam eco entre fontes.

## NSIS

- [ ] Verificar hash de `MeetingRecorder-Setup.exe` contra `SHA256SUMS.txt`.
- [ ] Executar instalador. Registrar avisos de assinatura/SmartScreen; não desativar proteções do Windows para contorná-los.
- [ ] Instalar para o usuário atual em pasta gravável; testar também caminho com espaço/acento. Confirmar nome/versão 0.1.0.
- [ ] WebView2 deve ser instalado automaticamente, offline, quando ausente. Não deve pedir Node/Rust/Python/processadores de áudio.
- [ ] Confirmar `MeetingRecorder.exe`, pastas `resources/whisper` e `resources/diarization`, modelo Base Multilingual Q5 e DLLs de runtime. Comparar com `resources-manifest.json`.
- [ ] Confirmar atalho do Menu Iniciar e opção de atalho da área de trabalho; iniciar por ambos. Nenhum localhost/servidor Vite deve ser necessário.

## Fluxo do MVP

- [ ] Abrir sem rede; banco/pastas em `%LOCALAPPDATA%\MeetingRecorder` devem ser criados automaticamente.
- [ ] Nomear reunião com texto em português. Selecionar mic+sistema, vídeo desligado, padrão Base Multilingual Q5/pt/limite de threads.
- [ ] Gravar cinco minutos com fala local e reprodução/reunião remota consentida. Conferir contador/estado e ausência de Whisper durante captura.
- [ ] Minimizar, restaurar e fechar a janela durante captura: gravação continua no tray. Tentar sair: confirmar proteção contra saída acidental.
- [ ] Finalizar pelo aplicativo e, em outra reunião, pelo tray. WAVs válidos/flush, durações coerentes e `merged.wav` PCM mono 16-bit/16 kHz.
- [ ] Acompanhar transcrição pós-reunião sem congelamento; observar CPU/RAM/threads/tempo. SQLite/TXT e histórico devem conservar texto.
- [ ] Player, título/data/duração, pesquisa literal, copiar com acentos, exportar TXT em destino escolhido e abrir pasta local devem funcionar.
- [ ] Cancelar uma transcrição e tentar novamente. Preservar WAVs e texto já finalizado; estados coerentes, sem subprocesso órfão.
- [ ] Fechar pelo tray, abrir novamente sem rede: reunião/texto/segmentos/configurações permanecem no histórico.
- [ ] **Obrigatório antes de aprovar MVP:** gravação de 60 minutos estável; medir CPU/RAM/handles a cada cinco minutos, continuidade dos WAVs, drift e tempo/memória de IA posterior.
- [ ] Desconectar microfone e saída/Bluetooth em testes separados: fonte saudável continua, erros úteis, arquivos válidos preservados. Conferir fallback na reunião seguinte.
- [ ] Simular pouco espaço somente em volume de teste limitado. Após corrigir espaço/permissão, retry deve preparar/transcrever fontes preservadas.
- [ ] Interromper processamento/reabrir e conferir recuperação. Documentar que recuperação automática de captura após crash ainda não está implementada; não aprovar esse cenário por inferência.
- [ ] Verificar logs locais limitados/rotacionados sem transcrição, títulos/caminhos privados ou áudio. Registrar qualquer erro/arquivo corrompido.

## Desinstalar/reinstalar

- [ ] Encerrar de forma segura pelo tray antes de remover o aplicativo; não forçar término durante gravação.
- [ ] Desinstalar pelas Configurações do Windows ou pelo uninstaller. Executável, recursos, atalhos e entrada do produto devem desaparecer.
- [ ] Confirmar que reuniões/configurações em `%LOCALAPPDATA%\MeetingRecorder` foram preservadas por padrão. A opção Tauri de apagar dados de WebView refere-se a `com.meetingrecorder.app`, não é exclusão de reuniões.
- [ ] Não remover WebView2/Visual C++ compartilhados com outros aplicativos.
- [ ] Reinstalar mesmo instalador, abrir e conferir histórico. Repetir com atualização/same-version sem dados duplicados e sem mudar identificador.

## MSI adicional

- [ ] Usar snapshot limpo distinto; validar instalação pelo `.msi`, permissões/UAC e escolha de pasta.
- [ ] Repetir runtime offline, recursos/atalhos, fluxo curto, reabertura e remoção (`msiexec /x` ou Configurações).
- [ ] Conferir upgrade code estável e preservação dos dados. MSI de máquina não equivale ao NSIS de usuário; registrar diferenças.

## Funcionalidades já existentes além do MVP básico

- [ ] Caso distribuídas no pacote, verificar diarização local+duas/três vozes, alternância/ambiguidade, cancelamento/fallback, sem cadastro de voz/embeddings persistidos.
- [ ] Vídeo opcional: janela do Chrome (Brave autorizado como substituto nesta máquina) e monitor, áudio simultâneo, minimizar/restaurar, MP4 reproduzível, duração/CPU/RAM. Desabilitado não deve iniciar pipeline de vídeo.

## Aprovação

- [ ] Todos os itens críticos do MVP aprovados, incluindo Windows limpo/offline, instalação/desinstalação e uma hora estável.
- [ ] Falhas restantes classificadas e relatório anexado; nenhuma gravação perdida/corrompida ou estado crítico inconsistente.
- [ ] Assinatura/publicação decididas pelo responsável; os artefatos locais não são automaticamente publicados.

Responsável: __________ Data: __________ Sistema/hash: __________ Resultado: __________
