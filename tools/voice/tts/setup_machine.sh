#!/usr/bin/env bash
# The TTS bake-off's GPU machine (docs/COMMANDS.md section 11, "Choosing a TTS"): one environment
# per model, since each wants its own Python and PyTorch. Runs on the AWS Deep Learning image
# (Ubuntu 22.04, NVIDIA driver). Each model's setup is independent: one failing leaves the others.
#   bash ~/job/tts/setup_machine.sh
set -uo pipefail
mkdir -p ~/job/logs
exec > >(tee -a ~/job/logs/setup.log) 2>&1
echo "== $(date -u) setup on $(hostname)"
nvidia-smi --query-gpu=name,memory.total,driver_version --format=csv,noheader

sudo apt-get update -qq && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq portaudio19-dev libsox-dev ffmpeg >/dev/null
command -v uv >/dev/null || curl -LsSf https://astral.sh/uv/install.sh | sh
export PATH="$HOME/.local/bin:$PATH"
TORCH_INDEX=https://download.pytorch.org/whl/cu128

setup_audio8() {
  uv venv -q ~/envs/audio8 --python 3.12 &&
  uv pip install -q --python ~/envs/audio8/bin/python torch torchaudio --index-url $TORCH_INDEX &&
  uv pip install -q --python ~/envs/audio8/bin/python "transformers>=4.57,<5" soundfile safetensors accelerate numpy &&
  ~/envs/audio8/bin/python -c "import torch; assert torch.cuda.is_available(); print('audio8 env ok', torch.__version__)"
}

setup_chatterbox() {
  uv venv -q ~/envs/chatterbox --python 3.11 &&
  uv pip install -q --python ~/envs/chatterbox/bin/python chatterbox-tts soundfile &&
  ~/envs/chatterbox/bin/python -c "import torch; assert torch.cuda.is_available(); from chatterbox.tts_turbo import ChatterboxTurboTTS; print('chatterbox env ok', torch.__version__)"
}

setup_fish() {
  [ -d ~/fish-speech ] || git clone -q https://github.com/fishaudio/fish-speech ~/fish-speech
  cd ~/fish-speech && echo "fish-speech at $(git rev-parse --short HEAD)" &&
  uv sync -q --python 3.12 --extra cu128 &&
  uv run --python 3.12 hf download fishaudio/s2-pro --local-dir checkpoints/s2-pro >/dev/null &&
  echo "fish env ok"
}

for m in audio8 chatterbox fish; do
  echo "== setting up $m"
  ( setup_$m ) > ~/job/logs/setup_$m.log 2>&1 && echo "$m: ok" || echo "$m: FAILED (see ~/job/logs/setup_$m.log)"
  tail -2 ~/job/logs/setup_$m.log
done
echo "== $(date -u) setup done"
