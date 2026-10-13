#!/usr/bin/env python3
"""
Проверка ПАРАЛЛЕЛЬНОЙ публикации: подписка USEPI/# на каждый включённый
MQTT-брокер генератора. Оба брокера должны получать одни и те же сигналы.

Использование:
    python3 scripts/verify_parallel_publish.py
    (по умолчанию: 127.0.0.1:1883 + 79.174.94.236:1883, duration 6s)
"""
import sys, time, json
import paho.mqtt.client as mqtt

BROKERS = [
    ("ЛОКАЛЬНЫЙ", "127.0.0.1", 1883),
    ("ВНЕШНИЙ", "79.174.94.236", 1883),
]
DURATION = 6
TOPIC = "USEPI/#"


def subscribe(host, port, name):
    got = []
    about = {"host": host, "port": port, "theme": name}
    c = mqtt.Client(mqtt.CallbackAPIVersion.VERSION2)
    c.on_connect = lambda cl, u, f, rc, pr: cl.subscribe(TOPIC)
    c.on_message = lambda cl, u, m: got.append((m.topic, m.payload[:40]))
    c.connect(host, port, 20)
    c.loop_start()
    time.sleep(DURATION)
    c.loop_stop()
    c.disconnect()
    return got


def main():
    results = {}
    for name, host, port in BROKERS:
        try:
            msgs = subscribe(host, port, name)
            results[name] = msgs
            print(f"{name} {host}:{port} -> {len(msgs)} сообщений ({TOPIC})")
            for t, p in msgs[:3]:
                print(f"    {t}  {p}")
        except Exception as e:
            print(f"{name} {host}:{port} -> ОШИБКА: {e}")
            results[name] = []

    all_ok = all(len(v) > 0 for v in results.values()) and len(results) > 0
    print()
    print("ПАРАЛЛЕЛЬНАЯ ПУБЛИКАЦИЯ:", "РАБОТАЕТ ✅" if all_ok else "ПРОБЛЕМА ⚠")
    return 0 if all_ok else 1


if __name__ == "__main__":
    sys.exit(main())
