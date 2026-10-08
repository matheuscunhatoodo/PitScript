# PitScript 0.1.1-rc.1 — UX e release

Data: 08/10/2026. **Candidata para testes; os critérios manuais do MVP continuam pendentes.**

## Alterações

- Interface baseada nas três prévias aprovadas: grafite, fundo claro quente, âmbar e monograma PitScript. Tipografia local, divisórias, lista de reuniões agrupada por data e busca por título.
- Preparação com nome sugerido, seletores de microfone/saída e vídeo opcional. Preferências e fallback continuam no backend.
- Gravação com tempo monotônico real, estado de cada fonte, finalização e barra persistente durante a navegação. Proteção contra comandos duplicados; falhas não escondem a finalização.
- Transcrição em blocos com timestamps clicáveis, pesquisa em texto/locutor/timestamp e navegação entre resultados. Texto tradicional para reuniões antigas ou falha de diarização.
- Player com pausa, seek, ±10 segundos, velocidade e marcação do trecho atual. O áudio anterior é encerrado ao sair da reunião.
- Carregamento, biblioteca vazia, arquivos indisponíveis, processamento/cancelamento e erros têm apresentação consistente; foco de teclado visível e layout adaptável.

Não foram acrescentadas dependências de produto, serviços externos, migrations ou alterações nos algoritmos de captura/inferência. A extensão `elapsedSeconds` é aditiva. Versões npm/Rust/Tauri: **0.1.1**. Janela: **PitScript**, 1280×800. Produto instalado, `com.meetingrecorder.app`, `MeetingRecorder.exe` e `%LOCALAPPDATA%\MeetingRecorder` foram preservados.

### Arquivos principais

- `src/app/App.tsx`, `src/hooks/useRecording.ts`: estado global e navegação.
- `src/components/{AppShell,Icon,RecordingBar,AudioPlayer,TranscriptView}.tsx`, `src/assets/pitscript-mark.png`: apresentação e player.
- `src/pages/{HomePage,NewRecordingPage,MeetingPage,SettingsPage}.tsx`, `src/styles/app.css`: telas e estilos.
- `src/utils/meeting.ts`, `tests/meeting-presentation.test.mjs`: apresentação e testes.
- `src/services/meeting.ts`, `src-tauri/src/meeting/manager.rs`: contador do backend e contrato de apresentação.
- `package{,-lock}.json`, `src-tauri/{Cargo.toml,Cargo.lock,tauri.conf.json}`, `index.html`: versão e título.
- `docs/design/mockups/`, especificação e plano em `docs/superpowers/`, este relatório e `README.md`.

## Validações executadas

| Validação | Resultado |
| --- | --- |
| Testes frontend de apresentação | 9 aprovados; agrupamento por calendário local, falhas, seek, busca/Unicode e segmentos sobrepostos |
| `cargo fmt`, `cargo fmt --check` | Aprovados |
| `cargo clippy --offline --all-targets -- -D warnings` | Aprovado |
| `cargo test --offline` | 128 aprovados, zero falhas; 5 testes físicos opt-in ignorados; 53,99 s |
| QA em Chromium/Brave headless | 16 cenários aprovados; zero erros de página |
| QA visual | Referências 1536×1024; notebook 1280×800 e largura 390×844; sem overflow horizontal |
| `npm run lint`, `npm test`, `npm run format:check`, `npm run build` | Aprovados; 33 módulos, JS 266,56 kB / 81,72 kB gzip no build final |
| Production build / Tauri bundle | Build Rust release aprovado; bundles finais em validação abaixo |

O teste Rust do contador e os novos helpers frontend foram observados falhando antes da implementação e passando depois. A suíte Rust inclui execução real de Whisper/sherpa com fixtures públicas e o ciclo `start → record → stop → prepare → transcribe → persist → reopen` com captura simulada e inferência local real.

[Evidência dos 16 cenários](design/validation/2026-10-08-browser-results.json). Telas implementadas: [biblioteca](design/implemented/01-biblioteca.png), [gravação](design/implemented/02-gravacao.png), [transcrição](design/implemented/03-transcricao.png).

Os 16 cenários de navegador cobrem: biblioteca/busca/vazio; segmentos e navegação da pesquisa; player real com metadata/play/pause/seek/velocidade; copiar acentos/exportar/abrir pasta via serviços; reuniões antigas; diarização indisponível; áudio ausente; padrões e seletores; duplo start/stop; navegação com captura ativa; falha parcial e erro de finalização; erro de início persistente; estado inicial indisponível; biblioteca vazia; layouts menores; preferência ativa/ausente sem override indevido; snapshot de estado atrasado com falha da consulta seguinte. São testes com **IPC sintético e WAV público**, não uma validação de WASAPI ou do diálogo nativo de exportação. O navegador integrado não acessou localhost; foi usado Chromium em perfil isolado, sem controlar a aplicação do usuário.

### Correções encontradas durante QA

- Restaurar a fonte do player após a limpeza de efeitos do React StrictMode.
- Sincronizar imediatamente o seek solicitado por um timestamp com a marcação do texto.
- Manter erro de start/stop após um refresh bem-sucedido.
- Atualizar texto tradicional mesmo se a consulta dos segmentos falhar.
- Informar falha parcial no detalhe, mesmo quando existe transcrição concluída.
- Invalidar snapshots antigos após start/stop para preservar a captura confirmada quando um refresh atrasado chega; teste diferido observado falhando e passando após a correção.
- Conservar o fallback Rust dos dispositivos configurados: a escolha automática envia IDs indefinidos; somente seleção manual envia um override estrito. A opção automática mostra corretamente a preferência existente/ausente.
- Remover pontuação não renderizada do documento de pesquisa, mantendo os offsets exatos e a cópia/exportação originais.
- Tratar rejeições síncronas das consultas de preferências/dispositivos na preparação e configurações.

### Revisão independente

Uma revisão do ramo antes da publicação encontrou dois problemas importantes (preferências/fallback e resposta de estado antiga) e um menor (pontuação de pesquisa invisível). Os três foram corrigidos com reprodução RED→GREEN. Nenhum achado crítico foi encontrado. O teste diferido mantém a captura e a finalização visíveis mesmo quando a consulta corretiva falha.

### Diferenças deliberadas em relação às imagens

- A lista mostra o estado real; o percentual ilustrativo de 47% não é inventado quando `list_meetings` não o fornece. A tela de processamento usa o progresso disponível nas APIs existentes.
- As fontes mostram descrições genéricas, pois o estado atual não informa o nome efetivo do dispositivo após fallback.
- A barra de título é nativa do Windows; não há controles de janela desenhados em CSS. Textura de mockup não foi introduzida.
- A opção de texto tradicional e o aviso sobre locutores estimados foram preservados junto ao documento.

## Reprodução manual

1. Instalar a candidata pelo EXE e abrir PitScript. Uma instalação anterior continua identificada como Meeting Recorder.
2. Conferir reuniões existentes, busca por título e configurações. Iniciar uma reunião curta com microfone e áudio do computador.
3. Navegar até Biblioteca/Configurações; conferir barra, contador e finalizar. Verificar que nenhuma transcrição começa durante a captura.
4. Abrir a reunião salva, reproduzir, pausar, usar ±10 s, alterar velocidade e clicar nos timestamps. Sair da reunião e confirmar que o áudio para.
5. Buscar palavra com acento, locutor e timestamp; navegar entre ocorrências; copiar/exportar TXT em UTF-8 e abrir a pasta.
6. Abrir uma reunião antiga sem segmentos e conferir o texto tradicional. Testar ausência de áudio sem perder a transcrição.
7. Executar também o [checklist de Windows limpo](RELEASE_CHECKLIST.md) e os [testes de estabilização](STABILIZATION_REPORT.md).

## Pendências e riscos

- Windows limpo sem ferramentas/runtime, instalação offline e desinstalação/atalhos efetivos ainda precisam de teste humano.
- Gravação longa, consumo/leaks com UI nativa, desconexões físicas, tray/minimização/fechamento, pouco espaço e vídeo de janela permanecem nas pendências anteriores. O usuário assumiu os testes manuais.
- Automação de navegador não prova comportamento de WebView2/asset protocol, clipboard/diálogos Tauri ou WASAPI em máquina limpa.
- Instaladores sem certificado do produto podem gerar aviso do Windows/SmartScreen.
- Compatibilidade de dados/identidade preservada e banco testado; upgrade efetivo de instalação existente ainda precisa do checklist humano.

**Esta candidata não declara o MVP pronto.**

## Artefatos e publicação

Resultados de bundle, extração, smoke do payload, hashes e link público serão consolidados após a validação final.
