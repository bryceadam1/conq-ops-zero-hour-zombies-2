#!/usr/bin/env bash
# One g6e.xlarge (L40S) for two jobs (docs/COMMANDS.md section 11): Fish S2-Pro speaks the
# held-out spoken test (`fish_corpus.py listen`'s ~/job/lines.tsv and ~/job/refs) into
# ~/job/out/fish, then flan-T5 is fine-tuned on ~/job/t5data from ~/job/base into ~/job/out/t5
# (train_t5.py, bf16). The Mac pushes those and the scripts (~/job/tts, ~/job/train) and pulls
# ~/job/out with `cloud.py watch`. Every step is logged; a failure still ends in DONE.
#   nohup bash ~/job/train/t5_fish_job.sh > ~/job/logs/t5_fish_job.out 2>&1 &
set -uo pipefail
mkdir -p ~/job/logs ~/job/out/t5
exec > >(tee -a ~/job/logs/t5_fish_job.log) 2>&1
finish() { echo "== $(date -u) finished: $1"; touch ~/job/DONE; exit 0; }
echo "== $(date -u) on $(hostname)"; nvidia-smi --query-gpu=name,memory.total --format=csv,noheader; nproc

# Fish first: its server holds most of the GPU, so it goes before training.
KEEP_GOING=1 bash ~/job/tts/fish_corpus.sh > ~/job/logs/fish_corpus.out 2>&1
tail -3 ~/job/logs/fish_corpus.log
sudo docker rm -f s2pro >/dev/null 2>&1
echo "== $(date -u) Fish wrote $(ls ~/job/out/fish/*.flac 2>/dev/null | wc -l) clips"

command -v uv >/dev/null || curl -LsSf https://astral.sh/uv/install.sh | sh
export PATH="$HOME/.local/bin:$PATH"
PY=~/envs/t5/bin/python
[ -x $PY ] || uv venv -q ~/envs/t5 --python 3.12 || finish "venv failed"
uv pip install -q --python $PY torch --index-url https://download.pytorch.org/whl/cu128 || finish "torch failed"
uv pip install -q --python $PY "transformers==5.22.2" safetensors sentencepiece || finish "transformers failed"
$PY -c "import torch; assert torch.cuda.is_available(); print('torch', torch.__version__, torch.cuda.get_device_name())" || finish "no CUDA"
cd ~/job/train && $PY train_t5.py --data ~/job/t5data --out ~/job/out/t5 --model ~/job/base --prompt ~/job/base/prompt.txt \
  --choices --epochs 1 --lr 3e-4 --bf16 || finish "training failed"
finish "fish and training done"
