# 🎛️ Signal Generator — Kanban Board

**MQTT брокер:** `79.174.94.236:1883`  
**Формат:** плоский JSON-LD (`@context`, `@id`, `generatedAt`, `value`, `unit`, `min`, `max`, `quality`)  
**Топики:** `signals/{generator_id}`

---

## 📋 TODO

- [ ] **T1** — Инициализация Rust проекта, Cargo.toml, модульная структура
- [ ] **T2** — Модели данных: `Signal` (JSON-LD), `GeneratorConfig`, типы волн
- [ ] **T3** — Генераторы сигналов (Sine, Sawtooth, Square, Noise, Random, Constant)
- [ ] **T4** — MQTT клиент (rumqttc, подключение к брокеру, реконнект, публикация)
- [ ] **T5** — REST API (actix-web: CRUD генераторов, toggle, текущее значение)
- [ ] **T6** — Веб-интерфейс (HTML+CSS+JS: таблица, график, модалка настроек)
- [ ] **T7** — Оркестратор: запуск/остановка генераторов, AppState, broadcast
- [ ] **T8** — Dockerfile + docker-compose для деплоя

---

## 🔧 IN PROGRESS

*(пусто)*

---

## ✅ DONE

*(пусто)*

---

## 📐 Архитектура

```
signal-generator/
├── Cargo.toml
├── Dockerfile
├── docker-compose.yml
├── KANBAN.md
├── static/
│   ├── index.html     ← UI
│   ├── style.css      ← стили
│   └── app.js         ← логика UI
└── src/
    ├── main.rs        ← точка входа, оркестратор
    ├── models.rs      ← модели Signal, GeneratorConfig
    ├── config.rs      ← сериализация/десериализация конфигов
    ├── generator.rs   ← генераторы сигналов
    ├── mqtt_client.rs ← MQTT клиент
    └── api.rs         ← REST API endpoints
```

## ⚙️ Быстрый старт

```bash
cd /home/dimiam/Рабочий\ стол/Генератор
cargo run --release
# → http://localhost:8080
```

## 📡 Формат JSON-LD

```json
{
  "@context": "https://www.w3.org/ns/sosa/",
  "@id": "sensor/generator-01",
  "generatedAt": "2026-07-29T20:00:00.000Z",
  "value": 25.43,
  "unit": "°C",
  "min": -10.0,
  "max": 60.0,
  "quality": 100
}
```
