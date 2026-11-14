#!/usr/bin/env bash
# Fish S2-Pro through SGLang-Omni on the GPU machine, then its throughput (fish_bench.py), then
# ~/job/DONE so the watchdog on the Mac pulls ~/job/out and terminates the machine. Every step is
# logged; a failure still ends in DONE (with the log saying why) so the machine never idles.
#   nohup bash ~/job/tts/fish_sglang.sh > ~/job/logs/fish_sglang.out 2>&1 &
set -uo pipefail
mkdir -p ~/job/logs ~/job/out
exec > >(tee -a ~/job/logs/fish_sglang.log) 2>&1
finish() { echo "== $(date -u) finished: $1"; touch ~/job/DONE; exit 0; }
echo "== $(date -u) on $(hostname)"; nvidia-smi --query-gpu=name,memory.total --format=csv,noheader

# The reference clips over local HTTP (the server fetches references by URL).
(cd ~/job/refs && nohup python3 -m http.server 9000 --bind 127.0.0.1 > ~/job/logs/refs_http.log 2>&1 &)

IMAGE=hongccc/sglang-omni:dev
sudo docker pull -q $IMAGE || finish "docker pull failed"
echo "image digest: $(sudo docker image inspect --format '{{index .RepoDigests 0}}' $IMAGE)"
# Serve inside the container, used as its image intends: the entrypoint (sglang-omni-entrypoint)
# clones sglang-omni into /workspace/sglang-omni, installs it editable into the image's Python (which
# already has every dependency) and starts our command there. Mounting a host checkout at that path
# breaks it (git refuses a repo owned by another user, "dubious ownership"): the first two runs died
# on that. Only the Hugging Face cache comes from the host.
sudo docker run -d --name s2pro --gpus all --ipc host --network host --shm-size 32g \
  -v ~/.cache/huggingface:/root/.cache/huggingface $IMAGE bash -c '
    set -ex
    git rev-parse --short HEAD
    python3 -c "import dac, audiotools" || python3 -m pip install "descript-audiotools==0.7.2" "descript-audio-codec==1.0.0"
    python3 -c "from huggingface_hub import snapshot_download; snapshot_download(\"fishaudio/s2-pro\")"
    sgl-omni serve --model-path fishaudio/s2-pro --config examples/configs/s2pro_tts.yaml --port 8000
  ' || finish "container failed to start"

echo "== waiting for the server"
for i in $(seq 180); do
  curl -s -o /dev/null -X POST http://127.0.0.1:8000/v1/audio/speech -H 'Content-Type: application/json' -d '{"model":"fishaudio/s2-pro","input":"Ready."}' && break
  sudo docker ps --filter name=s2pro --format "{{.Status}}" | grep -q Up || { sudo docker logs s2pro > ~/job/logs/s2pro_full.log 2>&1; tail -80 ~/job/logs/s2pro_full.log; finish "the server exited"; }
  sleep 10
done
sudo docker logs --tail 15 s2pro

# The client runs inside the server's container, which already has requests, numpy, soundfile and
# torchaudio (the AMI's python3 has no venv module, so a host venv fails); host networking reaches
# both the server and the reference clips' HTTP server.
sudo docker cp ~/job/tts/fish_bench.py s2pro:/tmp/fish_bench.py && sudo docker cp ~/job/lines.tsv s2pro:/tmp/lines.tsv
sudo docker exec s2pro python3 /tmp/fish_bench.py --lines /tmp/lines.tsv --out /tmp/fish_bench || echo "bench failed"
mkdir -p ~/job/out && sudo docker cp s2pro:/tmp/fish_bench ~/job/out/ && sudo chown -R "$(id -u):$(id -g)" ~/job/out
sudo docker logs --tail 40 s2pro > ~/job/logs/s2pro_tail.log 2>&1
nvidia-smi --query-gpu=memory.used,utilization.gpu --format=csv,noheader
finish "bench done"
