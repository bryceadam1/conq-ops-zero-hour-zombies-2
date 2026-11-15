#!/usr/bin/env bash
# The scale-up's Fish S2-Pro clips on the GPU machine: SGLang-Omni's server as fish_sglang.sh
# starts it, then fish_corpus.py speak over ~/job/lines.tsv into ~/job/out/fish (mounted into the
# container, so the watchdog on the Mac pulls clips as they land), then ~/job/DONE. Every step is
# logged; a failure still ends in DONE so the machine never idles.
#   nohup bash ~/job/tts/fish_corpus.sh > ~/job/logs/fish_corpus.out 2>&1 &
set -uo pipefail
mkdir -p ~/job/logs ~/job/out/fish
exec > >(tee -a ~/job/logs/fish_corpus.log) 2>&1
finish() { echo "== $(date -u) finished: $1"; touch ~/job/DONE; exit 0; }
echo "== $(date -u) on $(hostname)"; nvidia-smi --query-gpu=name,memory.total --format=csv,noheader

IMAGE=hongccc/sglang-omni:dev
sudo docker pull -q $IMAGE || finish "docker pull failed"
echo "image digest: $(sudo docker image inspect --format '{{index .RepoDigests 0}}' $IMAGE)"
# As fish_sglang.sh: the image's entrypoint installs sglang-omni itself; mem_fraction_static 0.6
# leaves the reference encoder room on a 48 GB L40S.
sudo docker run -d --name s2pro --gpus all --ipc host --network host --shm-size 32g \
  -v ~/.cache/huggingface:/root/.cache/huggingface -v ~/job/out/fish:/out $IMAGE bash -c '
    set -ex
    git rev-parse --short HEAD
    python3 -c "import dac, audiotools" || python3 -m pip install "descript-audiotools==0.7.2" "descript-audio-codec==1.0.0"
    python3 -c "from huggingface_hub import snapshot_download; snapshot_download(\"fishaudio/s2-pro\")"
    sgl-omni serve --model-path fishaudio/s2-pro --config examples/configs/s2pro_tts.yaml --port 8000 --mem-fraction-static 0.6
  ' || finish "container failed to start"

echo "== waiting for the server"
for i in $(seq 180); do
  curl -s -o /dev/null -X POST http://127.0.0.1:8000/v1/audio/speech -H 'Content-Type: application/json' -d '{"model":"fishaudio/s2-pro","input":"Ready."}' && break
  sudo docker ps --filter name=s2pro --format "{{.Status}}" | grep -q Up || { sudo docker logs s2pro > ~/job/logs/s2pro_full.log 2>&1; tail -80 ~/job/logs/s2pro_full.log; finish "the server exited"; }
  sleep 10
done
sudo docker logs --tail 15 s2pro

# Reference clips go in as file paths: the server refuses loopback URLs (fish_sglang.sh).
sudo docker cp ~/job/tts/fish_corpus.py s2pro:/tmp/fish_corpus.py && sudo docker cp ~/job/lines.tsv s2pro:/tmp/lines.tsv && sudo docker cp ~/job/refs s2pro:/tmp/refs
sudo docker exec s2pro python3 /tmp/fish_corpus.py speak --lines /tmp/lines.tsv --refs /tmp/refs --out /out --concurrency 32 || echo "speak failed"
sudo docker logs --tail 40 s2pro > ~/job/logs/s2pro_tail.log 2>&1
finish "corpus done"
