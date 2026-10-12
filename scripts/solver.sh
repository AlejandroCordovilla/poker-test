#!/bin/zsh
# Background solver: start | stop | open | status | log
# Dashboard: http://localhost:8790 (progress, hours left, pause, speed)
cd "${0:A:h}/.." || exit 1
URL=http://localhost:8790
BIN=./target/release/solve
running() { /usr/bin/curl -s -m 1 "$URL/api/status" >/dev/null 2>&1; }

case "${1:-open}" in
  start)
    if running; then echo "already running"; open "$URL"; exit 0; fi
    if [ ! -x "$BIN" ]; then
      export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
      cargo build -q --release -p solve-cli || exit 1
    fi
    [ -f data/plan.json ] || { "$BIN" ranges && "$BIN" flops && "$BIN" plan && "$BIN" estimate --pilot 0; } || exit 1
    nohup "$BIN" run >> data/solver.log 2>&1 &
    for i in {1..20}; do running && break; sleep 0.5; done
    echo "solver started (pid $!), dashboard $URL"
    open "$URL"
    ;;
  stop)
    running && /usr/bin/curl -s -X POST "$URL/api/stop" >/dev/null && echo "stopping; finished spots are kept" || echo "not running"
    ;;
  open)
    if running; then open "$URL"; else echo "not running; use: scripts/solver.sh start"; "$BIN" status 2>/dev/null | tail -1; fi
    ;;
  status) "$BIN" status ;;
  log) tail -n 30 data/solver.log ;;
  *) echo "usage: scripts/solver.sh start|stop|open|status|log" ;;
esac
