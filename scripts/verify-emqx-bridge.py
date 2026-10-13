#!/usr/bin/env python3
"""Проверка моста локальный EMQX → внешний брокер 79.174.94.236.

Генератор публикует ТОЛЬКО в локальный EMQX (config/mqtt.json: remote.enabled=false).
Всё, что доходит до внешнего брокера, форвардит EMQX-мост (rule forward_usepi → action forward_usepi
→ connector ext_usepi, QoS1, буфер 256 МБ, TTL 1 ч).

Запуск:
  uv venv ~/.hermes/cache/scratch/mqtenv && uv pip install --python ~/.hermes/cache/scratch/mqtenv/bin/python paho-mqtt
  ~/.hermes/cache/scratch/mqtenv/bin/python scripts/verify-emqx-bridge.py 15 "USEPI/#"

Ожидаемо: внешний брокер получает сопоставимое (или большее, если дренится буфер) число сообщений.
Если на внешнем 0 при ненулевом локальном — мост сломан: смотри emqx-bridge.conf
(в HOCON ссылка должна быть `connector = ext_usepi`, БЕЗ префикса "mqtt:") и
~/.hermes/scripts/emqx_bridge_reapply.sh.
"""
import time, sys, collections
import paho.mqtt.client as mqtt

LOCAL = ("127.0.0.1", 1883)
REMOTE = ("79.174.94.236", 1883)
SECS = int(sys.argv[1]) if len(sys.argv) > 1 else 25
TOPIC = sys.argv[2] if len(sys.argv) > 2 else "USEPI/#"

local_msgs = collections.Counter()
remote_msgs = collections.Counter()


def connect_retry(name, addr, store, tries=8, delay=2):
    for i in range(1, tries + 1):
        st = {"rc": None}
        c = mqtt.Client(mqtt.CallbackAPIVersion.VERSION2, client_id=f"hermes-{name}-{int(time.time())}",
                        protocol=mqtt.MQTTv311)
        c.on_connect = lambda cl, u, f, rc, p=None: st.update(rc=rc)
        c.on_message = lambda cl, u, m: store.update([m.topic])
        try:
            c.connect(*addr, keepalive=30); c.loop_start()
            for _ in range(20):
                if st["rc"] is not None:
                    break
                time.sleep(0.5)
            if st["rc"] is not None:
                print(f"  [{name}] подключён (попытка {i})")
                return c
            c.loop_stop()
        except Exception as e:
            print(f"  [{name}] попытка {i}: {type(e).__name__}")
        time.sleep(delay)
    print(f"  [{name}] подключиться не удалось")
    return None


print(f"подписка на '{TOPIC}' локально и на внешнем брокере")
lc = connect_retry("local", LOCAL, local_msgs)
rc = connect_retry("remote", REMOTE, remote_msgs)
if lc:
    lc.subscribe(TOPIC, qos=0)
if rc:
    rc.subscribe(TOPIC, qos=0)
print(f"собираю {SECS} с...")
time.sleep(SECS)
L = sum(local_msgs.values()); R = sum(remote_msgs.values())
print(f"\nлокальный EMQX: {L} сообщений")
print(f"внешний брокер: {R} сообщений")
print("топ топиков на внешнем:", remote_msgs.most_common(5))
if R:
    ratio = R / L * 100 if L else 0
    print(f"\nИТОГ: мост несёт данные на внешний брокер ✅ ({ratio:.0f}% от локального потока)")
else:
    print("\nИТОГ: на внешний брокер ничего не дошло ❌")
for c in (lc, rc):
    if c:
        c.loop_stop(); c.disconnect()
