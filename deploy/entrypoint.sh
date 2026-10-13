#!/usr/bin/env bash
# Entrypoint: конфигурация генератора доставляется в образ ЗАШИФРОВАННОЙ
# (deploy/*.enc), ключ приходит в переменной CONFIG_KEY. Открытый вид
# создаётся только внутри контейнера, в публичный репозиторий не попадает.
set -e

if [ -n "$CONFIG_KEY" ] && [ -f deploy/generators.json.enc ]; then
  mkdir -p config
  if openssl enc -d -aes-256-cbc -pbkdf2 -iter 200000 \
       -pass env:CONFIG_KEY -in deploy/generators.json.enc -out config/generators.json 2>/dev/null; then
    echo "[entrypoint] config/generators.json расшифрован ($(wc -c < config/generators.json) байт)"
  else
    echo "[entrypoint] ОШИБКА: не удалось расшифровать generators.json (неверный CONFIG_KEY?)" >&2
    rm -f config/generators.json
  fi
  if openssl enc -d -aes-256-cbc -pbkdf2 -iter 200000 \
       -pass env:CONFIG_KEY -in deploy/mqtt.json.enc -out config/mqtt.json 2>/dev/null; then
    echo "[entrypoint] config/mqtt.json расшифрован"
  else
    echo "[entrypoint] предупреждение: mqtt.json не расшифрован" >&2
    rm -f config/mqtt.json
  fi
else
  echo "[entrypoint] CONFIG_KEY не задан — старт с конфигурацией по умолчанию"
fi

exec ./signal-generator
