# Meeting Recorder 0.1.0 — relatório de empacotamento

Data: **05/10/2026**. Plataforma: Windows x64. Estado: **candidata para testes; MVP ainda não aprovado**. Nenhuma funcionalidade nova, API externa ou publicação foi acrescentada nesta etapa.

## 1. Artefatos gerados

Pasta local, relativa à raiz do checkout: `releases/0.1.0/`. Os instaladores são artefatos de build e não acompanham o código no GitHub.

| Arquivo | Tamanho | SHA-256 |
| --- | ---: | --- |
| `MeetingRecorder-Setup.exe` | 305.047.921 bytes / 290,92 MiB | `2538426700f7fe231fe880ad36e8bc6945af33a5822d46f5a9d905b92e7e2d5c` |
| `MeetingRecorder-0.1.0-x64.msi` | 311.435.264 bytes / 297,01 MiB | `a7179aa71a255d4e88b5dc755b518cbf9b94e1b274d2772c89792047110dc822` |
| `SHA256SUMS.txt` | Hashes dos instaladores e do inventário | — |
| `resources-manifest.json` | 37 recursos com caminho relativo, tamanho e hash | — |

Originais recém-gerados pelo Tauri:

- `src-tauri/target/release/bundle/nsis/Meeting Recorder_0.1.0_x64-setup.exe`.
- `src-tauri/target/release/bundle/msi/Meeting Recorder_0.1.0_x64_pt-BR.msi`.
- Executável de produção: `src-tauri/target/release/MeetingRecorder.exe`.

Os instaladores são cópias dos bundles novos, não do instalador antigo de 30/09. **Não estão assinados** com certificado do produto. Os binários Microsoft redistribuídos conservam sua assinatura original. Nada foi publicado e nenhuma tag Git foi criada. Artefatos grandes de `releases/` são gerados localmente e ignorados pelo Git.

## 2. Conteúdo e decisões

- Tauri production build, frontend embutido e nome `MeetingRecorder.exe`; não depende de Vite/localhost.
- Identificador `com.meetingrecorder.app`, nome do produto e versão preservados. O fallback para recursos do checkout é permitido somente em debug: produção procura recursos junto ao executável instalado.
- NSIS x64 para usuário atual, português brasileiro/inglês; MSI x64 em português brasileiro, por máquina. MSI já fazia parte dos alvos anteriores e foi mantido explicitamente.
- NSIS usa o diretório padrão do Tauri `%LOCALAPPDATA%\Meeting Recorder` (com espaço), configurável no instalador. Difere do destino sugerido `Local\Programs\MeetingRecorder` no roadmap, sem mudar a estrutura dos dados. SQLite/reuniões/configurações continuam em `%LOCALAPPDATA%\MeetingRecorder` (sem espaço), separado da instalação.
- NSIS inclui Menu Iniciar, opção de atalho desktop e uninstaller do Tauri. WiX inclui atalhos e remoção por Windows Installer. Os scripts gerados removem recursos e atalhos; não contêm exclusão de `%LOCALAPPDATA%\MeetingRecorder`. A opção NSIS de apagar dados de WebView é relativa a `com.meetingrecorder.app`, não às reuniões. Esse comportamento ainda precisa ser executado em máquina limpa.
- WebView2 **Evergreen offline installer x64 embutido**, 212.272.848 bytes. Assinatura Microsoft válida; SHA-256 `f6df8e4bc857786ff641cd01da1449169eaf8236c936ced485ea61685ba4da40`. Instalação automática quando ausente está configurada nos dois formatos. O executável redistribuído é do instalador Microsoft; seus arquivos não são removidos como runtime compartilhado na desinstalação do produto.
- whisper.cpp **b5130 x64 CPU**, com variantes CPU oficiais; **Base Multilingual Q5**, `ggml-base-q5_1.bin`, 59.707.625 bytes, idioma padrão `pt`. Modelo SHA-256 `422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898`.
- Recursos já existentes de diarização: sherpa-onnx **1.13.8**, ONNX Runtime **1.28.2**, segmentação int8 1.540.506 bytes e WeSpeaker ResNet34 LM 26.530.550 bytes. Não houve ampliação da diarização nesta etapa.
- Visual C++/OpenMP **14.44.35211.0**: cinco DLLs adjacentes a cada processador (`msvcp140`, `msvcp140_1`, `vcruntime140`, `vcruntime140_1`, `vcomp140`), copiadas dos redistribuíveis originais de VS 2022/v143. O bundler também inclui CRT na raiz. APIs do Windows/UCRT/Media Foundation continuam sendo componentes do Windows 10/11.
- Licenças/atribuições existentes acompanham os recursos; adicionados avisos dos runtimes e licença MIT do modelo Whisper. A preparação usa PowerShell apenas no build, com política restrita ao processo.

Nenhuma dependência Rust/npm nova. O usuário final não precisa instalar Node, npm, Rust, Cargo, Python ou Whisper/modelos separadamente. A primeira geração do instalador pode usar rede para ferramentas e WebView2 oficiais; isso não é um serviço de reunião.

Referências de empacotamento: [Tauri Windows Installer](https://v2.tauri.app/distribute/windows-installer/), [configuração Tauri](https://v2.tauri.app/reference/config/) e [redistribuição Visual C++](https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files?view=msvc-170).

## 3. Arquivos alterados nesta etapa

- `src-tauri/tauri.conf.json`: NSIS/MSI, nome de binário, recursos, WebView2 offline, CRT e preparação do build.
- `src-tauri/src/lib.rs`: fallback de recursos restrito a debug.
- `scripts/prepare-release.ps1`: preparação dos redistribuíveis, verificação de recursos necessários e build frontend; exige VS 2022/v143 na máquina de build.
- `src-tauri/resources/{whisper,diarization}/`: cinco DLLs de runtime em cada pasta.
- `src-tauri/resources/RUNTIME_NOTICES.md`, `src-tauri/resources/whisper/LICENSE-model.txt`.
- `README.md`, `.gitignore`, `docs/RELEASE_PLAN.md`, `docs/RELEASE_CHECKLIST.md`, este relatório.

As correções anteriores da Fase 15 em `Cargo.toml`, `meeting/manager.rs` e `transcription/engine.rs` estão incorporadas e descritas em [STABILIZATION_REPORT.md](STABILIZATION_REPORT.md): recuperação de panic em release, estado de falha da transcrição e retry da preparação. APIs/frontend/schema não foram alterados pelo empacotamento.

## 4. Validações executadas

| Validação | Resultado |
| --- | --- |
| `cargo fmt` e `cargo fmt --check` | Aprovados |
| `cargo clippy --offline --all-targets -- -D warnings` | Aprovado no código final, sem novos erros/avisos Clippy |
| `cargo test --offline` | **127 aprovados, zero falhas, cinco testes manuais opt-in ignorados**; binário/doc tests aprovados; 50,59 s |
| `npm run lint`, `npm test` | Aprovados; quatro testes frontend |
| `npm run format:check` | Aprovado |
| `npm run build` | Aprovado; 29 módulos, JS 254,75 kB / 78,35 kB gzip |
| `cargo build --offline --release --features tauri/custom-protocol` | Aprovado; frontend de produção embutido |
| `npm run tauri -- build --ci` | Aprovado; **dois bundles NSIS/MSI** novos |
| Dependências PE dos processadores | 67 imports C++/OpenMP, zero DLL necessária ausente na pasta correspondente |
| MSI extração administrativa (`msiexec /a /qn`) | Exit 0; extraiu o pacote em `.tooling/release-msi-extract` sem substituir instalação existente |
| Integridade do payload extraído | **37/37 recursos** idênticos por SHA-256; 118.048.957 bytes de recursos |
| Whisper extraído do pacote | Fixture inglês de 11 s, duas threads, exit 0, TXT esperado; 4,33 s no smoke final |
| sherpa extraído do pacote | Fixture de 16 s, duas threads, dois grupos automáticos, exit 0; 2,57 s no smoke final |
| Carregamento de runtime | DLLs VC140/OpenMP/ONNX/Whisper carregadas da pasta extraída; `msvcp_win.dll` é componente do próprio Windows |
| Inicialização e reabertura do executável extraído | `application_ready` em dois inícios, SQLite schema v4/integrity `ok`; reunião sintética e TXT UTF-8 com acentos preservados |
| Inspeção NSIS/WiX novos | Recursos/modelos, WebView2 offline, atalhos e ações de remoção presentes; MSI upgrade code estável `a5ea0871-4f19-512e-8e0b-7f8685d1456a` |
| Revisão independente | Nenhum defeito bloqueante confirmado no empacotamento; pré-requisito v143 documentado |

Os processadores e o aplicativo extraído foram iniciados com PATH restrito às pastas do Windows, sem Node/Cargo/Python no PATH. Python foi usado **apenas pelo teste de desenvolvimento** para inspecionar/semear o SQLite sintético; não participa da execução do produto nem está no pacote. O teste de reabertura verifica armazenamento, não interação visual no histórico.

A diferença de três bytes entre o executável de build e o extraído do MSI corresponde ao marcador Tauri `__TAURI_BUNDLE_TYPE_VAR_UNK` → `MSI`, aplicado pelo bundler. Os recursos conferem exatamente. O aviso de linker contém somente a criação de biblioteca/objeto MSVC; o aviso sobre identificador `.app` refere-se a macOS. Mantivemos o identificador para preservar compatibilidade Windows.

Evidências locais ignoradas pelo Git: `.tooling/release-{build,tauri-bundle,tests-final,clippy-final,format}.txt`, `release-runtime-dependencies.json`, `release-msi-extract.txt`, `release-smoke-evidence/{payload,native,native-modules,startup-0,startup-1}.json` e perfil SQLite sintético. Os arquivos do perfil real não foram usados.

## 5. Limites e pendências de aceite

Uma instalação anterior 0.1.0 existe no perfil real, com binário antigo `meeting-recorder.exe`. O pré-check interrompeu a instalação silenciosa **antes de qualquer substituição**. Não executamos upgrade/desinstalação desse produto. A leitura restrita inicial do registro não mostrava essa entrada; a verificação fora do sandbox confirmou a instalação anterior. Não houve rejeição de aprovação automática.

- **Instalação/desinstalação funcional e atalhos ainda pendentes em Windows limpo**, para NSIS e MSI. Extração e inspeção dos scripts não equivalem à instalação real.
- **WebView2 ausente, instalação e fluxo completo com rede desconectada ainda pendentes.** Nesta máquina os runtimes do sistema já existem; o PATH restrito e os módulos locais não provam o cenário completo de máquina limpa.
- **Gravação contínua de 60 minutos, CPU/RAM/leaks/drift e UI nativa ainda pendentes.** As medições curtas de inferência não são benchmark do consumo durante reunião.
- Fluxo manual instalado mic+sistema → parar → preparar → transcrever → player/exportar → sair → reabrir ainda precisa ser assinado. O teste integrado automatizado com Whisper real passou, mas usa captura simulada e fixtures.
- Desconexões físicas, tray/fechamento/minimização, pouco disco real, cancelamento na UI e captura de janela permanecem no [relatório de estabilização](STABILIZATION_REPORT.md). Por escolha do usuário, os testes manuais serão feitos por ele; ainda não recebemos resultados.
- Recuperação automática de captura após crash continua pendência conhecida; recuperação de transcrição existe. Nenhum novo recurso de recuperação foi acrescentado nesta etapa.
- Assinatura do produto e decisão de publicação ainda pendentes; artefatos atuais são para validação local.

## 6. Checklist de máquina limpa

Executar e registrar [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md). Resumo obrigatório:

1. Windows 10/11 x64 limpo, usuário padrão, sem SDKs; desconectar a rede e verificar SHA-256.
2. Instalar NSIS; confirmar instalação automática do runtime quando ausente e abrir pelos atalhos.
3. Gravar mic+sistema, finalizar, transcrever, reproduzir e exportar TXT em português; confirmar áudio/estado/texto.
4. Sair pelo tray e reabrir; verificar histórico/configurações. Repetir cancelamento, minimização e falhas físicas previstas.
5. Gravar **60 minutos** e medir CPU/RAM/continuidade/drift; validar IA somente após terminar.
6. Desinstalar, verificar remoção de produto/atalhos e preservação de reuniões; reinstalar e reabrir dados.
7. Repetir MSI em snapshot separado; anexar logs, ambiente/hash e resultados dos dois formatos.

**A Definition of Done do MVP não foi atendida integralmente. A release 0.1.0 não é declarada pronta.**

## 7. Critérios individuais desta etapa

| Critério | Estado |
| --- | --- |
| Production build Tauri | Atendido: build e executável de produção aprovados |
| NSIS e `MeetingRecorder-Setup.exe` | Atendido: gerado, copiado, hash verificado |
| MSI já previsto | Atendido: gerado e extração administrativa aprovada |
| Runtime necessário incluído | Atendido no pacote: WebView2 offline e CRT/OpenMP; instalação em Windows sem runtimes pendente |
| whisper.cpp e Base Multilingual Q5 incluídos | Atendido: recursos extraídos íntegros e execução real aprovada |
| Usuário sem Node/npm/Rust/Cargo/Python/Whisper manual | Pacote preparado para esse requisito; execução sem SDKs no PATH aprovada; Windows limpo ainda pendente |
| Atalhos | Configurados nos dois formatos; criação/abertura/remoção reais pendentes |
| Uninstall funcional | Ações de remoção inspecionadas; execução real pendente |
| Fluxo completo offline e reabertura instalada | Testes integrados e reabertura sintética aprovados; validação instalada/manual pendente |
| Gravação de uma hora estável | Pendente crítico |
| MVP 0.1.0 pronto | **Não atendido** enquanto os gates críticos permanecerem abertos |
