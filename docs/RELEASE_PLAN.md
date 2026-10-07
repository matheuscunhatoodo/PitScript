# Release 0.1.0 — execução

Escopo autorizado: empacotar o código existente para Windows x64, validar produção/NSIS/MSI, recursos locais, atalhos e desinstalação. Sem novas funcionalidades. A versão permanece candidata até atender toda a Definition of Done do MVP.

## Etapas

1. Revisar arquitetura, instalador/build/DoD do roadmap e dependências PE dos executáveis/DLLs.
2. Configurar NSIS/MSI explícitos, WebView2 offline e runtimes locais necessários a Whisper/sherpa. Manter identidade e dados de instalações existentes.
3. Executar frontend, fmt/Clippy/testes Rust, release e bundle. Copiar NSIS para o nome público `MeetingRecorder-Setup.exe`, com hashes e inventário de recursos.
4. Testar instalação/desinstalação em diretório de teste e recursos instalados; registrar limites da máquina de desenvolvimento.
5. Documentar artefatos, resultados e checklist obrigatório em Windows limpo/offline; revisar o conjunto de mudanças.

## Registro

- Antes da etapa adicional, a estabilização corrigiu panic da transcrição, o perfil release incompatível com recuperação de panic e retry da preparação após falha de arquivo. A validação atual aprovou 127 testes Rust, zero falhas e cinco capturas manuais opt-in ignoradas. Os testes manuais ficaram a cargo do usuário, por sua escolha, sem resultados recebidos até agora.
- NSIS antigo de 30/09 é histórico e não representa o código/modelos/runtimes atuais.
- O Tauri CLI/crates locais suportam `offlineInstaller` e `bundleVCRuntime`; não é necessário acrescentar plugin ou dependência de produto.
- Inspeção PE: Whisper requer também `vcomp140.dll` (OpenMP); sherpa/ONNX requerem `msvcp140_1.dll`. DLLs na raiz do aplicativo não resolvem sozinhas as dependências dos executáveis em subpastas. O build prepara cópias locais junto aos processadores.
- A execução do script de preparação usa política somente no processo PowerShell de build; a política persistente do Windows não é alterada. Na máquina final o script/PowerShell/Node não são necessários para executar o aplicativo.
- Instalação local não equivale a Windows limpo. Gravação de uma hora e validações físicas pendentes impedem chamar a versão de pronta.
- Build/release/bundle final aprovados: NSIS 290,92 MiB e MSI 297,01 MiB. Cópias públicas em `releases/0.1.0`, hashes e 37 recursos inventariados.
- Clippy final aprovado; suite Rust 127/0/5, frontend quatro testes/build/lint/format aprovados. Revisão independente sem defeito bloqueante confirmado.
- Pré-check de instalação interrompido ao detectar produto antigo no registro real. Nenhuma substituição/desinstalação foi feita. MSI extraído administrativamente (exit 0), 37 recursos conferidos por hash, Whisper/sherpa executados do payload com DLLs locais, aplicação extraída inicializada/reaberta com SQLite/TXT sintéticos preservados.
- Etapas automatizadas encerradas; instalação/desinstalação e atalhos reais, máquina limpa/offline, gravação de 60 minutos e demais testes manuais são gates abertos. Evidências e critérios individuais: [RELEASE_REPORT.md](RELEASE_REPORT.md).
