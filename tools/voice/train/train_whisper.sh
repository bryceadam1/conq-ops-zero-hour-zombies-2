#!/usr/bin/env bash
# Whisper trained on commands, on a GPU machine (docs/COMMANDS.md section 11, step 13). The Mac
# pushes the clips, the recordings and the model to ~/job/data and the scripts to ~/job/train;
# this sets up an environment, trains into ~/job/out (pulled every few minutes by `cloud.py
# watch`), then writes ~/job/DONE. Run again after a lost machine, it resumes from ~/job/out/last.
# Every step is logged; a failure still ends in DONE so the machine never idles.
# The Mac trains while no machine has the run: after setup this writes ~/job/READY and waits for
# ~/job/GO, which the Mac writes once it has stopped and pushed its checkpoint to ~/job/out. A spot
# machine's two-minute notice stops the trainer at the next step with `last` saved, for the Mac to
# pull and carry on.
#   MAC_ROOT=<the Mac's ~/.conquerops/voice> nohup bash ~/job/train/train_whisper.sh base.en > ~/job/logs/train_whisper.out 2>&1 &
# (the clip lists name the Mac's paths; MAC_ROOT is mapped to ~/job/data)
set -uo pipefail
MODEL=${1:-base.en}
MAC_ROOT=${MAC_ROOT:?set MAC_ROOT to the Mac voice folder the clip lists name}
mkdir -p ~/job/logs ~/job/out
exec > >(tee -a ~/job/logs/train_whisper.log) 2>&1
finish() { echo "== $(date -u) finished: $1"; touch ~/job/DONE; exit 0; }
echo "== $(date -u) on $(hostname)"; nvidia-smi --query-gpu=name,memory.total --format=csv,noheader; nproc

# A fresh machine's unattended-upgrades holds dpkg's lock for its first minutes: wait for it.
sudo apt-get -o DPkg::Lock::Timeout=900 update -qq && sudo DEBIAN_FRONTEND=noninteractive apt-get -o DPkg::Lock::Timeout=900 install -y -qq ffmpeg >/dev/null || finish "apt failed"
# Encode a tenth of a second, not `-encoders | grep -q`: under pipefail, grep -q's early exit
# kills ffmpeg with SIGPIPE and fails the check with libopus present.
ffmpeg -hide_banner -loglevel error -f lavfi -i sine=d=0.1:r=16000 -c:a libopus -b:a 16k -f null - || finish "ffmpeg can't encode Opus"
command -v uv >/dev/null || curl -LsSf https://astral.sh/uv/install.sh | sh
export PATH="$HOME/.local/bin:$PATH"
PY=~/envs/whisper/bin/python
[ -x $PY ] || uv venv -q ~/envs/whisper --python 3.12 || finish "venv failed"
uv pip install -q --python $PY torch torchaudio --index-url https://download.pytorch.org/whl/cu128 || finish "torch failed"
uv pip install -q --python $PY "transformers==5.22.2" soundfile numpy || finish "transformers failed"
$PY -c "import torch; assert torch.cuda.is_available(); print('torch', torch.__version__, torch.cuda.get_device_name())" || finish "no CUDA"

touch ~/job/READY
for i in $(seq 360); do [ -f ~/job/GO ] && break; sleep 5; done
[ -f ~/job/GO ] || finish "the Mac never handed over"

D=~/job/data
RESUME=""
[ -f ~/job/out/last/state.json ] && RESUME=--resume
cd ~/job/train && { $PY train_whisper.py --whisper $D/whisper-$MODEL --train $D/augment/lists/train.tsv \
  --test $D/augment/lists/test.tsv --noisy $D/augment/test_noisy --prompts $D/corpus/prompts.tsv \
  --vocab $D/corpus/vocab.tsv --canonical $D/corpus/canonical.tsv \
  --augment $D/augment --out ~/job/out --remap "$MAC_ROOT=$D" \
  --epochs 6 --patience 4 --batch 64 --lr 5e-5 --eval-every 1000 --workers $(( $(nproc) - 1 )) --bf16 $RESUME & }
TRAINER=$!
# The spot notice (instance metadata, IMDSv2): SIGTERM the trainer, which saves at its next step.
( while kill -0 $TRAINER 2>/dev/null; do
    T=$(curl -s -X PUT http://169.254.169.254/latest/api/token -H "X-aws-ec2-metadata-token-ttl-seconds: 300")
    if [ "$(curl -s -o /dev/null -w '%{http_code}' -H "X-aws-ec2-metadata-token: $T" http://169.254.169.254/latest/meta-data/spot/instance-action)" = 200 ]; then
      echo "== $(date -u) spot notice: stopping the trainer"; kill -TERM $TRAINER; break
    fi
    sleep 5
  done ) &
wait $TRAINER; CODE=$?
[ $CODE -eq 75 ] && finish "handed off"
[ $CODE -eq 0 ] || finish "training failed ($CODE)"
finish "training done"
