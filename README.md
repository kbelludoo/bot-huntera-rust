# ⚡ Huntera Native Rust Bot (Native Rust Zero-GC Engine)

Bot autônomo nativo de alta performance para **Huntera Idle**, transposto do ecossistema Go com zero alocação de GC, rodando em ~1 a 3 MB de RAM.

## 🚀 Recursos
- ⚡ **Zero-GC & Tokio Async:** Executa dezenas de contas com consumo cirúrgico de RAM.
- 🛡️ **Anti-Ban Circuit Breaker:** Disjuntor automático contra HTTP 429/403 e recusas heurísticas.
- 💤 **Auditoria de Stamina/Fadiga:** Controlador com histerese anti-thrashing.
- 🧩 **Preservação de Schema Bruto:** Envelopes íntegros contra "save malformado".
- 🌐 **Web Dashboard Embutido (Axum):** Rodando na porta **3000** (`/api/status`).
- 📺 **Console Ticker no Terminal:** Estatísticas em tempo real a cada 4 segundos.

## 🛠️ Como Executar
```bash
cargo test
cargo run
cargo build --release
```
