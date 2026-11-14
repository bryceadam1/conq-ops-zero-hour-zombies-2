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
[ -d ~/sglang-omni ] || git clone -q https://github.com/sgl-project/sglang-omni ~/sglang-omni || finish "clone failed"
echo "sglang-omni at $(cd ~/sglang-omni && git rev-parse --short HEAD)"

# Install and serve inside the container (repo mounted for its configs, HF cache kept on the host).
sudo docker run -d --name s2pro --gpus all --ipc host --network host --shm-size 32g \
  -v ~/sglang-omni:/workspace/sglang-omni -v ~/.cache/huggingface:/root/.cache/huggingface \
  -w /workspace/sglang-omni $IMAGE bash -lc '
    set -e
    pip install -q --upgrade pip uv
    uv venv -q /opt/venv -p 3.12 && . /opt/venv/bin/activate
    uv pip install -q --prerelease=allow "sglang-omni==0.2.0" "descript-audiotools==0.7.2" "descript-audio-codec==1.0.0"
    hf download fishaudio/s2-pro > /dev/null
    sgl-omni serve --model-path fishaudio/s2-pro --config examples/configs/s2pro_tts.yaml --port 8000
  ' || finish "container failed to start"

echo "== waiting for the server"
for i in $(seq 180); do
  curl -s -o /dev/null -X POST http://127.0.0.1:8000/v1/audio/speech -H 'Content-Type: application/json' -d '{"model":"fishaudio/s2-pro","input":"Ready."}' && break
  sudo docker ps --filter name=s2pro --format '{{.Status}}' | grep -q Up || { sudo docker logs --tail 60 s2pro; finish "the server exited"; }
  sleep 10
done
sudo docker logs --tail 15 s2pro

python3 -m venv ~/benchenv && ~/benchenv/bin/pip install -q requests soundfile numpy torch torchaudio --index-url https://download.pytorch.org/whl/cpu --extra-index-url https://pypi.org/simple
~/benchenv/bin/python ~/job/tts/fish_bench.py --lines ~/job/lines.tsv --out ~/job/out/fish_bench || echo "bench failed"
sudo docker logs --tail 40 s2pro > ~/job/logs/s2pro_tail.log 2>&1
nvidia-smi --query-gpu=memory.used,utilization.gpu --format=csv,noheader
finish "bench done"
