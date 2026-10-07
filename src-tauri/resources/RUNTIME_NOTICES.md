# Runtimes e recursos distribuídos

- Microsoft Visual C++ 2015–2022 Runtime x64 e OpenMP: DLLs redistribuíveis originais do Visual Studio Build Tools, sem alterações. Preparadas no build a partir de `VC/Redist/MSVC`, nunca copiadas do Windows/System32. Os arquivos continuam sujeitos aos Microsoft Software License Terms e à lista de código redistribuível do Visual Studio. A versão/hash exatos ficam no inventário da release.
  - https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files
  - https://visualstudio.microsoft.com/license-terms/
- Microsoft Edge WebView2 Evergreen Runtime: instalador offline oficial incluído pelo Tauri. Instalado somente quando necessário. Não remover runtimes compartilhados ao desinstalar o Meeting Recorder.
  - https://developer.microsoft.com/en-us/microsoft-edge/webview2/
- whisper.cpp e modelo Base Multilingual Q5: recursos locais em `resources/whisper/`; licença MIT do whisper.cpp acompanha os binários.
- sherpa-onnx, ONNX Runtime, pyannote e WeSpeaker: recursos existentes em `resources/diarization/`, com licenças, atribuição e avisos próprios. Mantidos para as funções já implementadas; nenhuma inferência externa.

O runtime C++ local deve ser atualizado e republicado junto ao aplicativo quando necessário. Não há instalação manual de Node, Rust, Python ou processadores de áudio pelo usuário final.
