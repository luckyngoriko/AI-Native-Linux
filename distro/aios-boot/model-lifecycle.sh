#!/bin/sh
#
# AI-OS.NET Rev.5 – Model Lifecycle Management
# POSIX-compatible (busybox/ash). No bashisms.
#
# Usage: model-lifecycle.sh <command> [args...]
#
# Commands:
#   pull-model <name>     Pull a model via ollama
#   list-models           List local models (ollama + vllm)
#   remove-model <name>   Remove a model
#   health-check          Check ollama and vllm are responding
#   gc-models             Remove models not accessed in 30 days
#   preload-warm <name>   Preload a model into GPU
#   status                Show disk, loaded models, GPU usage

set -e

CONFIG_FILE="${AIOS_CONFIG:-/etc/aios/model-config.toml}"
OLLAMA_HOST="${OLLAMA_HOST:-127.0.0.1:11434}"
OLLAMA_MODELS_DIR="${OLLAMA_MODELS_DIR:-/var/lib/aios/models/ollama}"
VLLM_HOST="${VLLM_HOST:-127.0.0.1:8000}"
VLLM_MODELS_DIR="${VLLM_MODELS_DIR:-/var/lib/aios/models/vllm}"
RETENTION_DAYS="${AIOS_MODEL_RETENTION_DAYS:-30}"
MAX_DISK_GB="${AIOS_MODEL_MAX_DISK_GB:-50}"

log() { printf '[%s] %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*"; }
die()  { log "ERROR: $*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# pull-model <model_name>
# ---------------------------------------------------------------------------
cmd_pull_model() {
	[ $# -ge 1 ] || die "Usage: pull-model <model_name>"
	model="$1"
	log "Pulling model: $model"
	ollama pull "$model" || die "ollama pull failed for $model"
	log "Model pulled: $model"
}

# ---------------------------------------------------------------------------
# list-models – ollama list + vllm /v1/models
# ---------------------------------------------------------------------------
cmd_list_models() {
	echo "=== Ollama Models ==="
	if command -v ollama >/dev/null 2>&1; then
		ollama list 2>/dev/null || echo "(ollama not running or no models)"
	else
		echo "(ollama binary not found)"
	fi
	echo ""
	echo "=== vLLM Models ==="
	if command -v curl >/dev/null 2>&1; then
		resp="$(curl -sS --connect-timeout 5 "http://${VLLM_HOST}/v1/models" 2>/dev/null)" || true
		if [ -n "$resp" ]; then
			# extract model ids from JSON with basic grep/sed (no jq dependency)
			echo "$resp" | sed 's/,/\n/g' | grep '"id"' | sed 's/.*"id"[[:space:]]*:[[:space:]]*"//' | sed 's/"//g' || echo "(parse error)"
		else
			echo "(vllm not running or no models)"
		fi
	else
		echo "(curl not found)"
	fi
}

# ---------------------------------------------------------------------------
# remove-model <model_name>
# ---------------------------------------------------------------------------
cmd_remove_model() {
	[ $# -ge 1 ] || die "Usage: remove-model <model_name>"
	model="$1"
	log "Removing model: $model"
	ollama rm "$model" || die "ollama rm failed for $model"
	log "Model removed: $model"
}

# ---------------------------------------------------------------------------
# health-check – check both ollama and vllm API endpoints
# ---------------------------------------------------------------------------
cmd_health_check() {
	status=0

	echo "=== AIOS Model Health Check ==="
	echo ""

	# Ollama health
	echo -n "Ollama (http://${OLLAMA_HOST}): "
	if command -v curl >/dev/null 2>&1; then
		resp="$(curl -sS --connect-timeout 5 "http://${OLLAMA_HOST}/api/tags" 2>/dev/null)" || true
		if echo "$resp" | grep -q '"models"'; then
			echo "OK"
		else
			echo "FAILED"
			status=1
		fi
	else
		echo "SKIP (curl not found)"
	fi

	# vLLM health
	echo -n "vLLM  (http://${VLLM_HOST}): "
	if command -v curl >/dev/null 2>&1; then
		resp="$(curl -sS --connect-timeout 5 "http://${VLLM_HOST}/health" 2>/dev/null)" || true
		if echo "$resp" | grep -qE '"status"|"healthy"'; then
			echo "OK"
		else
			echo "FAILED"
			status=1
		fi
	else
		echo "SKIP (curl not found)"
	fi

	echo ""
	if [ "$status" -eq 0 ]; then
		log "Health check: PASSED"
	else
		log "Health check: FAILED"
	fi
	return $status
}

# ---------------------------------------------------------------------------
# gc-models – remove models not accessed in N days
# ---------------------------------------------------------------------------
cmd_gc_models() {
	log "Starting model GC (retention=${RETENTION_DAYS} days)"

	if [ ! -d "$OLLAMA_MODELS_DIR" ]; then
		log "Models dir does not exist: $OLLAMA_MODELS_DIR"
		return 0
	fi

	# Find and remove blob files older than retention
	found=0
	deleted=0

	for f in "$OLLAMA_MODELS_DIR"/blobs/*; do
		[ -f "$f" ] || continue
		if find "$f" -atime +"${RETENTION_DAYS}" 2>/dev/null | grep -q .; then
			size_kb="$(du -k "$f" 2>/dev/null | awk '{print $1}')"
			log "GC: removing stale blob $f (${size_kb} KB, not accessed in ${RETENTION_DAYS}+ days)"
			rm -f "$f"
			deleted=$((deleted + 1))
		fi
		found=$((found + 1))
	done

	log "GC complete: scanned ${found} blobs, deleted ${deleted}"
}

# ---------------------------------------------------------------------------
# preload-warm <model_name> – send a warm-up prompt
# ---------------------------------------------------------------------------
cmd_preload_warm() {
	[ $# -ge 1 ] || die "Usage: preload-warm <model_name>"
	model="$1"
	log "Preloading model: $model"

	if ! command -v curl >/dev/null 2>&1; then
		die "curl is required for preload-warm"
	fi

	curl -sS --connect-timeout 10 \
		-X POST "http://${OLLAMA_HOST}/api/generate" \
		-H "Content-Type: application/json" \
		-d "{\"model\":\"${model}\",\"prompt\":\"Hello.\",\"stream\":false}" \
		>/dev/null || die "preload warm-up request failed for $model"

	log "Preload warm-up sent for: $model"
}

# ---------------------------------------------------------------------------
# status – disk, loaded models, GPU memory
# ---------------------------------------------------------------------------
cmd_status() {
	echo "=== AIOS Model Status ==="
	echo ""

	# Disk usage
	echo "--- Disk Usage ---"
	for d in "$OLLAMA_MODELS_DIR" "$VLLM_MODELS_DIR"; do
		if [ -d "$d" ]; then
			usage="$(du -sh "$d" 2>/dev/null | awk '{print $1}')"
			echo "  $d: $usage"
		else
			echo "  $d: (not present)"
		fi
	done

	# Max disk limit
	echo "  max_disk_gb: ${MAX_DISK_GB}"
	echo ""

	# Loaded models (ollama)
	echo "--- Loaded Models (Ollama) ---"
	if command -v curl >/dev/null 2>&1; then
		resp="$(curl -sS --connect-timeout 5 "http://${OLLAMA_HOST}/api/ps" 2>/dev/null)" || true
		if [ -n "$resp" ] && echo "$resp" | grep -q '"models"'; then
			echo "$resp" | sed 's/,/\n/g' | grep '"name"' | sed 's/.*"name"[[:space:]]*:[[:space:]]*"//' | sed 's/"//g' | while IFS= read -r m; do
				echo "  $m"
			done
		else
			echo "  (none or ollama not responding)"
		fi
	else
		echo "  (curl not found)"
	fi
	echo ""

	# vLLM models
	echo "--- Loaded Models (vLLM) ---"
	if command -v curl >/dev/null 2>&1; then
		resp="$(curl -sS --connect-timeout 5 "http://${VLLM_HOST}/v1/models" 2>/dev/null)" || true
		if [ -n "$resp" ] && echo "$resp" | grep -q '"id"'; then
			echo "$resp" | sed 's/,/\n/g' | grep '"id"' | sed 's/.*"id"[[:space:]]*:[[:space:]]*"//' | sed 's/"//g' | while IFS= read -r m; do
				echo "  $m"
			done
		else
			echo "  (none or vllm not responding)"
		fi
	else
		echo "  (curl not found)"
	fi
	echo ""

	# GPU status (nvidia-smi if available)
	echo "--- GPU Status ---"
	if command -v nvidia-smi >/dev/null 2>&1; then
		nvidia-smi --query-gpu=index,name,memory.used,memory.total,utilization.gpu,temperature.gpu --format=csv,noheader 2>/dev/null | while IFS=, read -r idx name mem_used mem_total gpu_util temp; do
			echo "  GPU${idx}: ${name} | Mem: ${mem_used}/${mem_total} | Util: ${gpu_util} | Temp: ${temp}"
		done
	elif command -v rocminfo >/dev/null 2>&1; then
		echo "  ROCm detected (use rocm-smi for details)"
	elif [ -e /dev/dri/renderD128 ]; then
		echo "  GPU render node present (no nvidia-smi/rocm-smi)"
	else
		echo "  No GPU detected"
	fi
}

# ---------------------------------------------------------------------------
# Main dispatcher
# ---------------------------------------------------------------------------
usage() {
	cat <<EOF
Usage: $(basename "$0") <command> [args...]

Commands:
  pull-model <name>     Pull a model via ollama
  list-models           List local models (ollama + vllm)
  remove-model <name>   Remove a model
  health-check          Check ollama and vllm are responding
  gc-models             Remove models not accessed in ${RETENTION_DAYS} days
  preload-warm <name>   Preload a model into GPU
  status                Show disk, loaded models, GPU usage
EOF
	exit 1
}

[ $# -ge 1 ] || usage

cmd="$1"
shift

case "$cmd" in
	pull-model)     cmd_pull_model "$@" ;;
	list-models)    cmd_list_models "$@" ;;
	remove-model)   cmd_remove_model "$@" ;;
	health-check)   cmd_health_check "$@" ;;
	gc-models)      cmd_gc_models "$@" ;;
	preload-warm)   cmd_preload_warm "$@" ;;
	status)         cmd_status "$@" ;;
	-h|--help|help) usage ;;
	*)              echo "Unknown command: $cmd" >&2; usage ;;
esac
