#!/usr/bin/env bash
# Вахтёр доступности demo.blue-traktor.com: 1 час, каждые 20 с — мелкий запрос,
# каждые 5 минут — крупный чанк. Пишет эпизоды с таймстампами (материал для тикета).
LOG=/home/dimiam/.hermes/cache/scratch/bt-watch-$(date +%Y%m%d-%H%M).log
CHUNK=$(curl -sS --max-time 15 https://demo.blue-traktor.com/ 2>/dev/null | grep -oE '/_next/static/chunks/[a-z0-9-]+\.js' | head -1)
{
echo "# вахтёр стартовал $(date '+%F %T')  chunk=$CHUNK"
echo "# формат: время | http_code/время_ответа_с | ошибка"
} > "$LOG"
END=$(( $(date +%s) + 3600 ))
FAILS=0; N=0; BIGFAIL=0; BIG=0
while [ "$(date +%s)" -lt "$END" ]; do
  N=$((N+1))
  T=$(date '+%F %T')
  R=$(curl -sS -o /dev/null --max-time 12 -w "%{http_code}/%{time_total}" https://demo.blue-traktor.com/ 2>&1)
  case "$R" in
    *000*|*curl:*|*"Connection"*|*"timed out"*) FAILS=$((FAILS+1)); MARK=" <<< ЭПИЗОД";;
    *) MARK="";;
  esac
  echo "$T | $R$MARK" >> "$LOG"
  if [ $((N % 15)) -eq 0 ] && [ -n "$CHUNK" ]; then
    BIG=$((BIG+1))
    B=$(curl -sS -o /dev/null --max-time 20 -w "%{http_code}/%{size_download}/%{time_total}" "https://demo.blue-traktor.com$CHUNK" 2>&1)
    case "$B" in *000*|*curl:*|*"timed out"*) BIGFAIL=$((BIGFAIL+1)); MARK2=" <<< КРУПНЫЙ ОБОРВАЛСЯ";; *) MARK2="";; esac
    echo "$T | chunk: $B$MARK2" >> "$LOG"
  fi
  sleep 20
done
{
echo "--- итог $(date '+%F %T') ---"
echo "мелких запросов: $N, неудачных: $FAILS"
echo "крупных чанков: $BIG, из них провалилось: $BIGFAIL"
grep -c "ЭПИЗОД" "$LOG" | sed 's/^/строк-эпизодов: /'
} >> "$LOG"
tail -6 "$LOG"
