# PitScript — UX editorial aprovada

Data: 08/10/2026. O usuário aprovou as três prévias e solicitou implementação, envio ao GitHub e nova release. Referências: docs/design/mockups/{01-biblioteca,02-gravacao,03-transcricao}.png.

## Escopo e critérios

- Marca PitScript na interface; navegação Biblioteca, Nova gravação, Configurações. Monograma P, grafite #252722, branco quente #FAF8F3, âmbar #D5A34D, vermelho somente para captura/finalização. Ícones coerentes, cantos 6px, divisórias finas e Segoe UI local.
- Biblioteca com busca por título, reuniões agrupadas pelo dia local, data/hora, duração, fontes separadas, estado da gravação/transcrição. Dados vêm do SQLite via serviços existentes. Atualizar automaticamente durante processamento sem deixar requisições sobrepostas.
- Preparação curta: nome sugerido por data, fontes habilitadas, seleção dos dispositivos via APIs existentes, vídeo opcional e seleção atual de janela/monitor. Threads ficam em Configurações. Fallback permanece no Rust.
- Gravação: contador baseado no tempo monotônico do MeetingManager, estados reais de cada fonte, finalizar e aviso de minimização. Barra persistente em todas as páginas; estado compartilhado evita comandos duplicados e conserva resultado após navegar. Erros não ocultam botão de finalizar.
- Transcrição: blocos por locutor/timestamps com busca literal e navegação de todos os resultados (incluindo rótulos/timestamps), timestamps clicáveis, marcação temporal durante reprodução, texto tradicional como fallback em reuniões antigas/falhas. Copiar/exportar preservam texto e semântica existentes.
- Player fixo na área de conteúdo: play/pause, seek, +/-10s, velocidade, tempo, duração real do arquivo. Navegação entre reuniões encerra áudio anterior. Erros/rejeição de play visíveis; controles desabilitados enquanto mídia indisponível. Nenhuma waveform ou amplitude fictícia.
- Estados de carregamento/erro/vazio/cancelamento/diarização preservados. Mensagens claras com detalhes técnicos expansíveis. Foco visível, labels, uso de teclado, layout de notebook (1280x800), referência (1536x1024) e largura reduzida.

## Arquitetura e compatibilidade

React apenas apresenta e chama services/*.ts. MeetingManager continua orquestrando; adicionar elapsedSeconds ao estado é uma extensão compatível. Nenhum serviço externo, processamento em tempo real, dependência de runtime nova ou alteração de schema/storage/captura/inferência. Identificador com.meetingrecorder.app, produto instalável Meeting Recorder, nome MeetingRecorder.exe e pasta de dados preservados. Título da janela PitScript; versão binária 0.1.1 para upgrade. Publicação v0.1.1-rc.1 como candidata, sem declarar MVP pronto.

## Testes e publicação

Testes de apresentação para agrupamento local, estados de falha, clamp de seek, intervalos/overlap e pesquisa estruturada; teste Rust de elapsed em gravação, idle e finalizada. QA em navegador com IPC simulado e áudio de fixture pública, sem dados reais. Comparação das três telas com imagens aprovadas. Build/lint/test/format frontend, fmt/Clippy/test Rust, production build/bundles NSIS/MSI, integridade/extratação e smoke do payload. Uma revisão independente antes do merge. Publicar instalador, MSI se gerado, SHA256SUMS e manifesto; verificar acesso público e digest. Windows limpo, uma hora, desconexões físicas e fluxos nativos permanecem pendentes até teste humano.
