# 🎛️ Signal Generator

Промышленный генератор сигналов с публикацией в MQTT и веб-интерфейсом.

## Возможности

- Генерация сигналов: Sine, Sawtooth, Square, Noise, Random, Constant + эффекты (дрейф, шум, выбросы, мёртвая зона, гистерезис, залипание, джиттер, тренд, потеря пакетов, деградация качества)
- Публикация в MQTT (rumqttc) с авто-реконнектом
- Формат: плоский JSON-LD (`@context`, `@id`, `generatedAt`, `value`, `unit`, `min`, `max`, `quality`)
- Топики: `signals/{generator_id}` + `signals/diagnostics`
- REST API (actix-web): CRUD генераторов, toggle, текущее значение
- Веб-интерфейс без JS-зависимостей: SSR-панели с живыми значениями, meta refresh, график на Canvas
- Docker + docker-compose для деплоя

## Быстрый старт

```bash
cargo run --release
# Веб-интерфейс: http://localhost:8080
# API: http://localhost:8080/api/generators
```

## Docker

```bash
docker compose up -d --build
```

## Конфигурация

Конфиги генераторов: `config/generators.json` (persistence между рестартами).

## Технологии

Rust · actix-web · rumqttc · tokio · serde · MQTT · JSON-LD
