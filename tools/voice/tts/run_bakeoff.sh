#!/usr/bin/env bash
# Run the bake-off on the GPU machine (after setup_machine.sh): each model speaks ~/job/lines.tsv
# into ~/job/out/audio/<model>/, then ~/job/DONE tells the watchdog on the Mac to pull everything
# and terminate the machine. A model that fails is logged and skipped.
#   nohup bash ~/job/tts/run_bakeoff.sh &
set -uo pipefail
export PATH="$HOME/.local/bin:$PATH"
mkdir -p ~/job/logs ~/job/out/audio
exec > >(tee -a ~/job/logs/run.log) 2>&1
cd ~/job
echo "== $(date -u) bake-off"

run() {  # model, python
  echo "== $1 from $(date -u +%H:%M:%S)"
  "$2" tts/generate.py --model "$1" --lines lines.tsv --out out/audio > logs/gen_$1.log 2>&1 && echo "$1: ok" || echo "$1: FAILED"
  tail -3 logs/gen_$1.log
}

[ -x ~/envs/audio8/bin/python ] && run audio8 ~/envs/audio8/bin/python
[ -x ~/envs/chatterbox/bin/python ] && run chatterbox ~/envs/chatterbox/bin/python

if [ -d ~/fish-speech/checkpoints/s2-pro ]; then
  # fish-speech's API server, for as long as its generation runs.
  (cd ~/fish-speech && uv run --python 3.12 tools/api_server.py --listen 127.0.0.1:8080 \
     --llama-checkpoint-path checkpoints/s2-pro --decoder-checkpoint-path checkpoints/s2-pro/codec.pth \
     --decoder-config-name modded_dac_vq) > logs/fish_server.log 2>&1 &
  SERVER=$!
  for _ in $(seq 120); do curl -s -o /dev/null http://127.0.0.1:8080/ && break; sleep 5; done
  # The client needs only requests, soundfile and torchaudio: the audio8 environment has them.
  uv pip install -q --python ~/envs/audio8/bin/python requests
  run fish ~/envs/audio8/bin/python
  kill $SERVER 2>/dev/null
fi

nvidia-smi --query-gpu=memory.used --format=csv,noheader
echo "== $(date -u) bake-off done"
touch ~/job/DONE
